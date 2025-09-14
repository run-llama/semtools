use anyhow::{Context, Result, anyhow, bail};
use arrow_array::types::Float32Type;
use arrow_array::{
    FixedSizeListArray, Float32Array, Float64Array, Int32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DocMeta {
    pub path: String,
    pub size_bytes: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone)]
pub struct LineEmbedding {
    pub path: String,
    pub line_number: i32,
    pub embedding: Vec<f32>,
}

impl DocMeta {
    pub fn id(&self) -> i32 {
        // Generate deterministic ID based on path hash for consistent upserts
        let mut hasher = DefaultHasher::new();
        self.path.hash(&mut hasher);
        // Use absolute value to ensure positive ID, avoid i32::MIN edge case
        (hasher.finish() as i32).abs().max(1)
    }
}

impl LineEmbedding {
    pub fn id(&self) -> i32 {
        // Generate deterministic ID based on path + line number for consistent upserts
        let mut hasher = DefaultHasher::new();
        self.path.hash(&mut hasher);
        self.line_number.hash(&mut hasher);
        // Use absolute value to ensure positive ID, avoid i32::MIN edge case
        (hasher.finish() as i32).abs().max(1)
    }
}

#[derive(Debug, Clone)]
pub struct RankedLine {
    pub path: String,
    pub line_number: i32,
    pub distance: f32,
}

#[derive(Debug, Clone)]
pub struct WorkspaceStats {
    pub total_documents: usize,
    pub has_index: bool,
    pub index_type: Option<String>,
}

pub struct Store {
    db: lancedb::Connection,
}

impl Store {
    pub async fn open(workspace_dir: &str) -> Result<Self> {
        let db_path = Path::new(workspace_dir)
            .join("documents.lance")
            .to_string_lossy()
            .to_string();
        let db = lancedb::connect(&db_path)
            .execute()
            .await
            .with_context(|| format!("failed to open LanceDB connection at {db_path}"))?;

        Ok(Self { db })
    }

    /// Get existing document metadata for the given paths
    pub async fn get_existing_docs(&self, paths: &[String]) -> Result<HashMap<String, DocMeta>> {
        let mut existing = HashMap::new();

        // Check if documents table exists
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        if !tables.contains(&"documents".to_string()) {
            return Ok(existing);
        }

        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;

        // Query in chunks to avoid overly long IN(...) filters
        for chunk in paths.chunks(1000) {
            let filter_expr = build_in_filter(chunk);

            let stream = tbl
                .query()
                .only_if(filter_expr)
                .execute()
                .await
                .context("failed to execute documents query")?;

            let batches: Vec<RecordBatch> = stream
                .try_collect()
                .await
                .context("failed to collect query result batches")?;

            for batch in batches {
                let schema = batch.schema();
                let path_idx = schema
                    .index_of("path")
                    .context("missing 'path' column in documents schema")?;
                let size_idx = schema
                    .index_of("size_bytes")
                    .context("missing 'size_bytes' column in documents schema")?;
                let mtime_idx = schema
                    .index_of("mtime")
                    .context("missing 'mtime' column in documents schema")?;

                let path_array = batch
                    .column(path_idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| anyhow!("unexpected type for 'path' column"))?;
                let size_array = batch
                    .column(size_idx)
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .ok_or_else(|| anyhow!("unexpected type for 'size_bytes' column"))?;
                let mtime_array = batch
                    .column(mtime_idx)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .ok_or_else(|| anyhow!("unexpected type for 'mtime' column"))?;

                for i in 0..batch.num_rows() {
                    let path = path_array.value(i).to_string();
                    let size_bytes = size_array.value(i);
                    let mtime = mtime_array.value(i);

                    existing.insert(
                        path.clone(),
                        DocMeta {
                            path,
                            size_bytes,
                            mtime,
                        },
                    );
                }
            }
        }

        Ok(existing)
    }

    /// Delete documents and all associated line embeddings by path
    pub async fn delete_documents(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        // Delete from both tables to maintain synchronization
        self.delete_document_metadata(paths).await?;
        self.delete_line_embeddings(paths).await?;

        Ok(())
    }

    /// Delete only document metadata by path (internal method)
    async fn delete_document_metadata(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        if !tables.contains(&"documents".to_string()) {
            return Ok(()); // Nothing to delete
        }

        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;

        // Delete in chunks
        for chunk in paths.chunks(1000) {
            let filter_expr = build_in_filter(chunk);
            tbl.delete(&filter_expr).await.with_context(|| {
                format!("failed to delete documents with filter: {filter_expr}")
            })?;
        }

        Ok(())
    }

    /// Delete line embeddings by path
    pub async fn delete_line_embeddings(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        if !tables.contains(&"line_embeddings".to_string()) {
            return Ok(()); // Nothing to delete
        }

        let tbl = self
            .db
            .open_table("line_embeddings")
            .execute()
            .await
            .context("failed to open 'line_embeddings' table")?;

        // Delete in chunks
        for chunk in paths.chunks(1000) {
            let filter_expr = build_in_filter(chunk);
            tbl.delete(&filter_expr).await.with_context(|| {
                format!("failed to delete line embeddings with filter: {filter_expr}")
            })?;
        }

        Ok(())
    }

    /// Upsert document metadata for tracking file changes (no embeddings stored)
    pub async fn upsert_document_metadata(&self, metas: &[DocMeta]) -> Result<()> {
        if metas.is_empty() {
            return Ok(());
        }

        // First, delete any existing documents with the same paths
        let paths: Vec<String> = metas.iter().map(|m| m.path.clone()).collect();
        self.delete_document_metadata(&paths).await?;

        // Define schema for metadata only
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("size_bytes", DataType::UInt64, false),
            Field::new("mtime", DataType::Int64, false),
        ]));

        // Build a single RecordBatch
        let id_array = Int32Array::from_iter_values(metas.iter().map(|meta| meta.id()));
        let path_array =
            StringArray::from(metas.iter().map(|m| m.path.as_str()).collect::<Vec<_>>());
        let size_bytes_array = UInt64Array::from_iter_values(metas.iter().map(|m| m.size_bytes));
        let mtime_array = Int64Array::from_iter_values(metas.iter().map(|m| m.mtime));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(path_array),
                Arc::new(size_bytes_array),
                Arc::new(mtime_array),
            ],
        )?;

        // Wrap into a RecordBatchReader
        let batches = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema.clone());

        // Create table if needed, otherwise open and append
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        let table_existed = tables.contains(&"documents".to_string());

        if !table_existed {
            // Create table with initial data
            self.db
                .create_table("documents", Box::new(batches))
                .execute()
                .await
                .context("failed to create 'documents' table")?;
        } else {
            let tbl = self
                .db
                .open_table("documents")
                .execute()
                .await
                .context("failed to open 'documents' table")?;
            tbl.add(Box::new(batches))
                .execute()
                .await
                .context("failed to append batches to 'documents' table")?;
        }

        Ok(())
    }

    /// Upsert line-level embeddings for documents
    pub async fn upsert_line_embeddings(&self, line_embeddings: &[LineEmbedding]) -> Result<()> {
        if line_embeddings.is_empty() {
            return Ok(());
        }

        let dim = line_embeddings[0].embedding.len();
        if dim == 0 {
            bail!("embeddings must be non-empty vectors");
        }
        if line_embeddings.iter().any(|e| e.embedding.len() != dim) {
            bail!("all embeddings must have equal length");
        }

        // First, delete any existing lines with the same paths
        let paths: Vec<String> = line_embeddings.iter().map(|le| le.path.clone()).collect();
        let unique_paths: std::collections::HashSet<String> = paths.into_iter().collect();
        let unique_paths: Vec<String> = unique_paths.into_iter().collect();
        self.delete_line_embeddings(&unique_paths).await?;

        // Define schema for line embeddings
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("line_number", DataType::Int32, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dim as i32,
                ),
                true,
            ),
        ]));

        // Build RecordBatch
        let id_array = Int32Array::from_iter_values(line_embeddings.iter().map(|le| le.id()));
        let path_array = StringArray::from(
            line_embeddings
                .iter()
                .map(|le| le.path.as_str())
                .collect::<Vec<_>>(),
        );
        let line_number_array =
            Int32Array::from_iter_values(line_embeddings.iter().map(|le| le.line_number));
        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            line_embeddings
                .iter()
                .map(|le| Some(le.embedding.iter().cloned().map(Some))),
            dim as i32,
        );

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(path_array),
                Arc::new(line_number_array),
                Arc::new(vector_array),
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema.clone());

        // Create or append to line_embeddings table
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        let table_existed = tables.contains(&"line_embeddings".to_string());

        if !table_existed {
            self.db
                .create_table("line_embeddings", Box::new(batches))
                .execute()
                .await
                .context("failed to create 'line_embeddings' table")?;
        } else {
            let tbl = self
                .db
                .open_table("line_embeddings")
                .execute()
                .await
                .context("failed to open 'line_embeddings' table")?;
            tbl.add(Box::new(batches))
                .execute()
                .await
                .context("failed to append batches to 'line_embeddings' table")?;
        }

        // Ensure vector index exists
        self.ensure_line_vector_index().await?;

        Ok(())
    }

    /// Ensures vector index exists for line embeddings table
    async fn ensure_line_vector_index(&self) -> Result<()> {
        let tbl = self
            .db
            .open_table("line_embeddings")
            .execute()
            .await
            .context("failed to open 'line_embeddings' table")?;

        // Check if vector index exists
        let indices = tbl
            .list_indices()
            .await
            .context("failed to list indices for 'line_embeddings' table")?;
        let has_vector_index = indices
            .iter()
            .any(|idx| idx.columns.contains(&"vector".to_string()));

        if !has_vector_index {
            // Create new index - handle case where there are too few rows for PQ index
            match tbl.create_index(&["vector"], Index::Auto).execute().await {
                Ok(_) => {
                    // Index created successfully
                }
                Err(e) => {
                    // Check if this is a PQ training error due to insufficient rows
                    let error_msg = e.to_string();
                    if error_msg.contains("Not enough rows to train PQ")
                        || error_msg.contains("Requires 256 rows")
                    {
                        // Log a warning but continue - the database will still work without the index
                        // It will just use brute-force search instead of approximate search
                        eprintln!(
                            "Warning: Skipping line embeddings vector index creation due to insufficient data (need at least 256 rows for PQ index). Database will use brute-force search."
                        );
                    } else if error_msg.contains("No space left on device") {
                        return Err(anyhow!(
                            "Insufficient disk space to create vector index. Consider freeing up space or using a different workspace location."
                        ));
                    } else if error_msg.contains("Permission denied") {
                        return Err(anyhow!(
                            "Permission denied while creating vector index. Check workspace directory permissions."
                        ));
                    } else {
                        // For other errors, we should still fail
                        return Err(e.into());
                    }
                }
            }
        } else {
            // Optimize existing index to include new data
            // This is much faster than recreating the entire index
            if tbl.optimize(Default::default()).await.is_err() {
                // If optimization fails, we could fall back to recreating the index
                // but for now just log and continue
                eprintln!("Warning: Failed to optimize line embeddings vector index");
            }
        }

        Ok(())
    }

    /// Get statistics about the workspace store
    pub async fn get_stats(&self) -> Result<WorkspaceStats> {
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;

        if !tables.contains(&"documents".to_string()) {
            return Ok(WorkspaceStats {
                total_documents: 0,
                has_index: false,
                index_type: None,
            });
        }

        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;

        // Get document count
        let stream = tbl
            .query()
            .execute()
            .await
            .context("failed to execute count query on 'documents'")?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .context("failed to collect result batches for stats")?;
        let total_documents = batches.iter().map(|batch| batch.num_rows()).sum();

        // Check if vector index exists
        let line_tbl = self
            .db
            .open_table("line_embeddings")
            .execute()
            .await
            .context("failed to open 'line_embeddings' table")?;
        let indices = line_tbl
            .list_indices()
            .await
            .context("failed to list indices for 'line_embeddings' table")?;
        let has_vector_index = indices
            .iter()
            .any(|idx| idx.columns.contains(&"vector".to_string()));

        let index_type = if has_vector_index {
            // LanceDB Auto index creates IVF_PQ for vector columns by default
            Some("IVF_PQ".to_string())
        } else {
            None
        };

        Ok(WorkspaceStats {
            total_documents,
            has_index: has_vector_index,
            index_type,
        })
    }

    /// Get all document paths in the workspace
    pub async fn get_all_document_paths(&self) -> Result<Vec<String>> {
        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;

        if !tables.contains(&"documents".to_string()) {
            return Ok(Vec::new());
        }

        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;
        let stream = tbl
            .query()
            .execute()
            .await
            .context("failed to execute query for all document paths")?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .context("failed to collect batches for all document paths")?;

        let mut paths = Vec::new();
        for batch in batches {
            let schema = batch.schema();
            let path_idx = schema
                .index_of("path")
                .context("missing 'path' column in documents schema")?;
            let path_array = batch
                .column(path_idx)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("unexpected type for 'path' column"))?;

            for i in 0..batch.num_rows() {
                paths.push(path_array.value(i).to_string());
            }
        }

        Ok(paths)
    }

    /// Search line embeddings directly for precise results
    pub async fn search_line_embeddings(
        &self,
        query_vec: &[f32],
        subset_paths: &[String],
        top_k: usize,
        max_distance: Option<f32>,
    ) -> Result<Vec<RankedLine>> {
        // Short-circuit on empty subsets
        if subset_paths.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        let tables = self
            .db
            .table_names()
            .execute()
            .await
            .context("failed to list LanceDB tables")?;
        if !tables.contains(&"line_embeddings".to_string()) {
            return Ok(Vec::new());
        }

        let tbl = self
            .db
            .open_table("line_embeddings")
            .execute()
            .await
            .context("failed to open 'line_embeddings' table")?;

        let mut all_results = Vec::new();

        // Search in chunks to avoid overly long IN(...) filters
        for chunk in subset_paths.chunks(1000) {
            let filter_expr = build_in_filter(chunk);

            let query = tbl
                .query()
                .only_if(filter_expr)
                .nearest_to(query_vec)
                .context("failed to set nearest_to on line embeddings query")?
                .distance_type(lancedb::DistanceType::Cosine)
                .limit(top_k * 2); // Get more results per chunk to improve global ranking

            let stream = query
                .execute()
                .await
                .context("failed to execute line embeddings search")?;

            let batches: Vec<RecordBatch> = stream
                .try_collect()
                .await
                .context("failed to collect line embeddings search batches")?;

            for batch in batches {
                let schema = batch.schema();

                let path_idx = schema
                    .index_of("path")
                    .context("missing 'path' column in line embeddings result")?;
                let line_number_idx = schema
                    .index_of("line_number")
                    .context("missing 'line_number' column in line embeddings result")?;
                let distance_idx = schema
                    .index_of("_distance")
                    .or_else(|_| schema.index_of("distance"))
                    .context("missing 'distance' column in line embeddings result")?;

                let path_array = batch
                    .column(path_idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| anyhow!("unexpected type for 'path' column"))?;
                let line_number_array = batch
                    .column(line_number_idx)
                    .as_any()
                    .downcast_ref::<Int32Array>()
                    .ok_or_else(|| anyhow!("unexpected type for 'line_number' column"))?;
                let dist_col = batch.column(distance_idx);

                // Handle both f32 and f64 distance types
                if let Some(dist_array) = dist_col.as_any().downcast_ref::<Float32Array>() {
                    for i in 0..batch.num_rows() {
                        let distance = dist_array.value(i);
                        if let Some(max_dist) = max_distance
                            && distance > max_dist
                        {
                            continue;
                        }

                        all_results.push(RankedLine {
                            path: path_array.value(i).to_string(),
                            line_number: line_number_array.value(i),
                            distance,
                        });
                    }
                } else if let Some(dist_array) = dist_col.as_any().downcast_ref::<Float64Array>() {
                    for i in 0..batch.num_rows() {
                        let distance = dist_array.value(i) as f32;
                        if let Some(max_dist) = max_distance
                            && distance > max_dist
                        {
                            continue;
                        }

                        all_results.push(RankedLine {
                            path: path_array.value(i).to_string(),
                            line_number: line_number_array.value(i),
                            distance,
                        });
                    }
                } else {
                    bail!("unsupported distance column type in line embeddings search");
                }
            }
        }

        // Sort by distance and take global top-k
        all_results.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(top_k);

        Ok(all_results)
    }
}

pub fn build_in_filter(paths: &[String]) -> String {
    let escaped: Vec<String> = paths
        .iter()
        .map(|p| p.replace('\'', "''"))
        .map(|p| format!("'{p}'"))
        .collect();
    format!("path IN ({})", escaped.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper function to create a test store
    async fn create_test_store() -> (Store, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = Store::open(temp_dir.path().to_str().unwrap())
            .await
            .expect("Failed to create store");
        (store, temp_dir)
    }

    // Helper function to create test documents
    fn create_test_docs() -> (Vec<DocMeta>, Vec<Vec<f32>>) {
        let docs = vec![
            DocMeta {
                path: "/test/doc1.txt".to_string(),
                size_bytes: 100,
                mtime: 1234567890,
            },
            DocMeta {
                path: "/test/doc2.txt".to_string(),
                size_bytes: 200,
                mtime: 1234567891,
            },
            DocMeta {
                path: "/test/doc3.txt".to_string(),
                size_bytes: 150,
                mtime: 1234567892,
            },
        ];

        let embeddings = vec![
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.5, 0.6, 0.7, 0.8],
            vec![0.9, 1.0, 1.1, 1.2],
        ];

        (docs, embeddings)
    }

    #[tokio::test]
    async fn test_store_creation_and_stats_empty() {
        let (store, _temp_dir) = create_test_store().await;

        let stats = store.get_stats().await.expect("Failed to get stats");

        assert_eq!(stats.total_documents, 0);
        assert!(!stats.has_index);
        assert_eq!(stats.index_type, None);
    }

    #[tokio::test]
    async fn test_upsert_documents_and_stats() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, _embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .await
            .expect("Failed to upsert documents");

        // Check stats
        let stats = store.get_stats().await.expect("Failed to get stats");

        assert_eq!(stats.total_documents, 3);
        // Index may or may not be created depending on number of documents
        // (LanceDB requires 256+ rows for PQ index training)
        if stats.has_index {
            assert_eq!(stats.index_type, Some("IVF_PQ".to_string()));
        }
    }

    #[tokio::test]
    async fn test_get_all_document_paths() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, _embeddings) = create_test_docs();

        // Initially should be empty
        let paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get document paths");
        assert!(paths.is_empty());

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .await
            .expect("Failed to upsert documents");

        // Should now have paths
        let paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get document paths");

        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"/test/doc1.txt".to_string()));
        assert!(paths.contains(&"/test/doc2.txt".to_string()));
        assert!(paths.contains(&"/test/doc3.txt".to_string()));
    }

    #[tokio::test]
    async fn test_get_existing_docs() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, _embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .await
            .expect("Failed to upsert documents");

        // Test getting existing docs
        let query_paths = vec![
            "/test/doc1.txt".to_string(),
            "/test/doc2.txt".to_string(),
            "/test/nonexistent.txt".to_string(),
        ];

        let existing = store
            .get_existing_docs(&query_paths)
            .await
            .expect("Failed to get existing docs");

        assert_eq!(existing.len(), 2);
        assert!(existing.contains_key("/test/doc1.txt"));
        assert!(existing.contains_key("/test/doc2.txt"));
        assert!(!existing.contains_key("/test/nonexistent.txt"));

        // Verify metadata
        let doc1_meta = existing.get("/test/doc1.txt").unwrap();
        assert_eq!(doc1_meta.size_bytes, 100);
        assert_eq!(doc1_meta.mtime, 1234567890);
    }

    #[tokio::test]
    async fn test_delete_documents() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, _embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .await
            .expect("Failed to upsert documents");

        // Verify all documents exist
        let all_paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get document paths");
        assert_eq!(all_paths.len(), 3);

        // Delete some documents
        let to_delete = vec!["/test/doc1.txt".to_string(), "/test/doc3.txt".to_string()];
        store
            .delete_documents(&to_delete)
            .await
            .expect("Failed to delete documents");

        // Verify only doc2 remains
        let remaining_paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get document paths");
        assert_eq!(remaining_paths.len(), 1);
        assert!(remaining_paths.contains(&"/test/doc2.txt".to_string()));
    }

    #[tokio::test]
    async fn test_upsert_replaces_existing() {
        let (store, _temp_dir) = create_test_store().await;

        // Insert initial document
        let initial_doc = DocMeta {
            path: "/test/doc.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
        };
        let _initial_embedding = [vec![1.0, 2.0, 3.0, 4.0]];

        store
            .upsert_document_metadata(&[initial_doc])
            .await
            .expect("Failed to insert initial document");

        // Verify document exists
        let paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get paths");
        assert_eq!(paths.len(), 1);

        // Update the same document
        let updated_doc = DocMeta {
            path: "/test/doc.txt".to_string(),
            size_bytes: 200,
            mtime: 2000,
        };
        let _updated_embedding = [vec![5.0, 6.0, 7.0, 8.0]];

        store
            .upsert_document_metadata(&[updated_doc])
            .await
            .expect("Failed to update document");

        // Should still have only one document
        let paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get paths");
        assert_eq!(paths.len(), 1);

        // Verify metadata was updated
        let existing = store
            .get_existing_docs(&["/test/doc.txt".to_string()])
            .await
            .expect("Failed to get existing docs");
        let doc_meta = existing.get("/test/doc.txt").unwrap();
        assert_eq!(doc_meta.size_bytes, 200);
        assert_eq!(doc_meta.mtime, 2000);
    }

    #[test]
    fn test_build_in_filter() {
        let paths = vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "file with spaces.txt".to_string(),
            "file'with'quotes.txt".to_string(),
        ];

        let filter = build_in_filter(&paths);

        assert!(filter.starts_with("path IN ("));
        assert!(filter.ends_with(")"));
        assert!(filter.contains("'file1.txt'"));
        assert!(filter.contains("'file2.txt'"));
        assert!(filter.contains("'file with spaces.txt'"));
        // Single quotes should be escaped
        assert!(filter.contains("'file''with''quotes.txt'"));
    }

    #[test]
    fn test_doc_meta_id_generation() {
        let doc1 = DocMeta {
            path: "test1.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
        };
        let doc2 = DocMeta {
            path: "test2.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
        };

        let id1 = doc1.id();
        let id2 = doc2.id();

        // IDs should be different (random generation)
        assert_ne!(id1, id2);
        // IDs should be valid i32 values
        assert!(id1 >= 0);
        assert!(id2 >= 0);
    }
}
