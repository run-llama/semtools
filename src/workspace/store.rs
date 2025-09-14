use anyhow::{Result, bail, Context, anyhow};
use arrow_array::types::Float32Type;
use arrow_array::{
    FixedSizeListArray, Float32Array, Float64Array, Int32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use rand::Rng;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/* LanceDB examples

let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new(
        "vector",
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 128),
        true,
    ),
]));
// Create a RecordBatch stream.
let batches = RecordBatchIterator::new(
    vec![RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int32Array::from_iter_values(0..256)),
            Arc::new(
                FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    (0..256).map(|_| Some(vec![Some(1.0); 128])),
                    128,
                ),
            ),
        ],
    )
    .unwrap()]
    .into_iter()
    .map(Ok),
    schema.clone(),
);
db.create_table("my_table", Box::new(batches))
    .execute()
    .await
    .unwrap();
Create vector index (IVF_PQ)
LanceDB is capable to automatically create appropriate indices based on the data types of the columns. For example,

If a column has a data type of FixedSizeList<Float16/Float32>, LanceDB will create a IVF-PQ vector index with default parameters.
Otherwise, it creates a BTree index by default.
use lancedb::index::Index;
tbl.create_index(&["vector"], Index::Auto)
   .execute()
   .await
   .unwrap();
User can also specify the index type explicitly, see Table::create_index.

Open table and search
let results = table
    .query()
    .nearest_to(&[1.0; 128])
    .unwrap()
    .execute()
    .await
    .unwrap()
    .try_collect::<Vec<_>>()
    .await
    .unwrap();

*/

#[derive(Debug, Clone)]
pub struct DocMeta {
    pub path: String,
    pub size_bytes: u64,
    pub mtime: i64,
}

impl DocMeta {
    pub fn id(&self) -> i32 {
        // generate a random int32 id for the document
        rand::thread_rng().gen_range(0..i32::MAX)
    }
}

#[derive(Debug, Clone)]
pub struct RankedDoc {
    pub path: String,
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

    /// Delete documents by path
    pub async fn delete_documents(&self, paths: &[String]) -> Result<()> {
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
            tbl
                .delete(&filter_expr)
                .await
                .with_context(|| format!("failed to delete documents with filter: {filter_expr}"))?;
        }

        Ok(())
    }

    pub async fn upsert_documents(&self, metas: &[DocMeta], embeddings: &[Vec<f32>]) -> Result<()> {
        // Validate inputs
        if metas.len() != embeddings.len() {
            bail!(
                "metas and embeddings length mismatch: {} vs {}",
                metas.len(),
                embeddings.len()
            );
        }
        if embeddings.is_empty() {
            return Ok(()); // nothing to do
        }
        let dim = embeddings[0].len();
        if dim == 0 {
            bail!("embeddings must be non-empty vectors");
        }
        if embeddings.iter().any(|e| e.len() != dim) {
            bail!("all embeddings must have equal length");
        }

        // First, delete any existing documents with the same paths
        let paths: Vec<String> = metas.iter().map(|m| m.path.clone()).collect();
        self.delete_documents(&paths).await?;

        // Define schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("size_bytes", DataType::UInt64, false),
            Field::new("mtime", DataType::Int64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dim as i32,
                ),
                true,
            ),
        ]));

        // Build a single RecordBatch
        let id_array = Int32Array::from_iter_values(metas.iter().map(|meta| meta.id()));
        let path_array =
            StringArray::from(metas.iter().map(|m| m.path.as_str()).collect::<Vec<_>>());
        let size_bytes_array = UInt64Array::from_iter_values(metas.iter().map(|m| m.size_bytes));
        let mtime_array = Int64Array::from_iter_values(metas.iter().map(|m| m.mtime));
        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            embeddings
                .iter()
                .map(|embedding| Some(embedding.iter().cloned().map(Some))),
            dim as i32,
        );

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(path_array),
                Arc::new(size_bytes_array),
                Arc::new(mtime_array),
                Arc::new(vector_array),
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
            self
                .db
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
            tbl
                .add(Box::new(batches))
                .execute()
                .await
                .context("failed to append batches to 'documents' table")?;
        }

        // Handle index creation/optimization after data is added
        self.ensure_vector_index().await?;

        Ok(())
    }

    /// Ensures vector index exists and is optimized for current data size
    async fn ensure_vector_index(&self) -> Result<()> {
        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;

        // Check if vector index exists
        let indices = tbl
            .list_indices()
            .await
            .context("failed to list indices for 'documents' table")?;
        let has_vector_index = indices.iter().any(|idx| {
            idx.columns.contains(&"vector".to_string())
        });

        if !has_vector_index {
            // Create new index - handle case where there are too few rows for PQ index
            match tbl.create_index(&["vector"], Index::Auto)
                .execute()
                .await 
            {
                Ok(_) => {
                    // Index created successfully
                },
                Err(e) => {
                    // Check if this is a PQ training error due to insufficient rows
                    let error_msg = e.to_string();
                    if error_msg.contains("Not enough rows to train PQ") || error_msg.contains("Requires 256 rows") {
                        // Log a warning but continue - the database will still work without the index
                        // It will just use brute-force search instead of approximate search
                        eprintln!("Warning: Skipping vector index creation due to insufficient data (need at least 256 rows for PQ index). Database will use brute-force search.");
                    } else {
                        // For other errors, we should still fail
                        return Err(e.into());
                    }
                }
            }
        } else {
            // Optimize existing index to include new data
            // This is much faster than recreating the entire index
            if let Err(_) = tbl.optimize(Default::default()).await {
                // If optimization fails, we could fall back to recreating the index
                // but for now just log and continue
                eprintln!("Warning: Failed to optimize vector index");
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
        let indices = tbl
            .list_indices()
            .await
            .context("failed to list indices for 'documents' table")?;
        let has_vector_index = indices.iter().any(|idx| {
            idx.columns.contains(&"vector".to_string())
        });

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

    pub async fn ann_filter_top_k(
        &self,
        query_vec: &[f32],
        subset_paths: &[String],
        doc_top_k: usize,
        in_batch_size: usize,
    ) -> Result<Vec<RankedDoc>> {
        // Use good default parameters for balanced recall/latency
        // refine_factor=5: improves recall by re-ranking more candidates
        // nprobes=10: searches more index partitions for better recall
        self.ann_filter_top_k_with_params(query_vec, subset_paths, doc_top_k, in_batch_size, Some(5), Some(10)).await
    }

    /// ANN search with configurable search parameters for recall/latency tradeoff
    pub async fn ann_filter_top_k_with_params(
        &self,
        query_vec: &[f32],
        subset_paths: &[String],
        doc_top_k: usize,
        in_batch_size: usize,
        refine_factor: Option<u32>,
        nprobes: Option<u32>,
    ) -> Result<Vec<RankedDoc>> {
        // Short-circuit on empty subsets
        if subset_paths.is_empty() || doc_top_k == 0 {
            return Ok(Vec::new());
        }

        let tbl = self
            .db
            .open_table("documents")
            .execute()
            .await
            .context("failed to open 'documents' table")?;

        // Aggregate best (lowest) distance per path across batches
        let mut best_by_path: HashMap<String, f32> = HashMap::new();

        // Chunk the subset paths to avoid overly long IN(...) filters
        for chunk in subset_paths.chunks(in_batch_size.max(1)) {
            let filter_expr = build_in_filter(chunk);

            let mut query = tbl
                .query()
                .only_if(filter_expr)
                .nearest_to(query_vec)
                .context("failed to set nearest_to on query")?
                .distance_type(lancedb::DistanceType::Cosine)
                .limit(doc_top_k);

            // Apply search parameters for better recall/latency control
            if let Some(rf) = refine_factor {
                query = query.refine_factor(rf);
            }
            if let Some(np) = nprobes {
                query = query.nprobes(np as usize);
            }

            let stream = query
                .execute()
                .await
                .context("failed to execute ANN query batch")?;

            let batches: Vec<RecordBatch> = stream
                .try_collect()
                .await
                .context("failed to collect ANN query batches")?;

            for batch in batches {
                let schema = batch.schema();

                // Locate indices for path and distance columns dynamically
                let path_idx = schema
                    .index_of("path")
                    .context("missing 'path' column in ANN result schema")?;
                let distance_idx = schema
                    .index_of("_distance")
                    .or_else(|_| schema.index_of("distance"))
                    .context("missing 'distance' column in ANN result schema")?;

                let path_col = batch.column(path_idx);
                let dist_col = batch.column(distance_idx);

                let path_array = path_col
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| anyhow!("unexpected type for 'path' column in ANN result"))?;

                // Distance may be f32 or f64 depending on engine; handle both
                if let Some(dist_array) = dist_col.as_any().downcast_ref::<Float32Array>() {
                    for i in 0..batch.num_rows() {
                        let path = path_array.value(i).to_string();
                        let distance = dist_array.value(i);
                        match best_by_path.get_mut(&path) {
                            Some(existing) => {
                                if distance < *existing {
                                    *existing = distance;
                                }
                            }
                            None => {
                                best_by_path.insert(path, distance);
                            }
                        }
                    }
                } else if let Some(dist_array) = dist_col.as_any().downcast_ref::<Float64Array>() {
                    for i in 0..batch.num_rows() {
                        let path = path_array.value(i).to_string();
                        let distance_f32 = dist_array.value(i) as f32;
                        match best_by_path.get_mut(&path) {
                            Some(existing) => {
                                if distance_f32 < *existing {
                                    *existing = distance_f32;
                                }
                            }
                            None => {
                                best_by_path.insert(path, distance_f32);
                            }
                        }
                    }
                } else {
                    bail!("unsupported distance column type");
                }
            }
        }

        // Collect, sort by distance, and take global top-k
        let mut ranked: Vec<RankedDoc> = best_by_path
            .into_iter()
            .map(|(path, distance)| RankedDoc { path, distance })
            .collect();
        ranked.sort_by(|a, b| a
            .distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(doc_top_k);

        Ok(ranked)
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
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
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
        let (docs, embeddings) = create_test_docs();

        // Initially should be empty
        let paths = store
            .get_all_document_paths()
            .await
            .expect("Failed to get document paths");
        assert!(paths.is_empty());

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
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
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
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
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
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
    async fn test_ann_filter_top_k() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
            .await
            .expect("Failed to upsert documents");

        // Test ANN search
        let query_vec = vec![0.2, 0.3, 0.4, 0.5];
        let subset_paths = vec![
            "/test/doc1.txt".to_string(),
            "/test/doc2.txt".to_string(),
            "/test/doc3.txt".to_string(),
        ];

        let results = store
            .ann_filter_top_k(&query_vec, &subset_paths, 2, 1000)
            .await
            .expect("Failed to perform ANN search");

        // Should return results (exact ranking depends on embeddings)
        assert!(!results.is_empty());
        assert!(results.len() <= 2);

        // Results should be sorted by distance
        for i in 1..results.len() {
            assert!(results[i - 1].distance <= results[i].distance);
        }
    }

    #[tokio::test]
    async fn test_ann_filter_top_k_with_custom_params() {
        let (store, _temp_dir) = create_test_store().await;
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_documents(&docs, &embeddings)
            .await
            .expect("Failed to upsert documents");

        // Test ANN search with custom parameters
        let query_vec = vec![0.2, 0.3, 0.4, 0.5];
        let subset_paths = vec![
            "/test/doc1.txt".to_string(),
            "/test/doc2.txt".to_string(),
            "/test/doc3.txt".to_string(),
        ];

        let results = store
            .ann_filter_top_k_with_params(&query_vec, &subset_paths, 2, 1000, Some(3), Some(5))
            .await
            .expect("Failed to perform ANN search with custom params");

        // Should return results
        assert!(!results.is_empty());
        assert!(results.len() <= 2);
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
        let initial_embedding = vec![vec![1.0, 2.0, 3.0, 4.0]];

        store
            .upsert_documents(&[initial_doc], &initial_embedding)
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
        let updated_embedding = vec![vec![5.0, 6.0, 7.0, 8.0]];

        store
            .upsert_documents(&[updated_doc], &updated_embedding)
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
        let doc = DocMeta {
            path: "test.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
        };

        let id1 = doc.id();
        let id2 = doc.id();

        // IDs should be different (random generation)
        assert_ne!(id1, id2);
        // IDs should be valid i32 values
        assert!(id1 >= 0);
        assert!(id2 >= 0);
    }
}
