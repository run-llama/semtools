//! Qdrant Edge storage wrapper
use anyhow::{Result, anyhow};

use crate::search::DocumentInfo;
use edge::EdgeShard;
use ordered_float::OrderedFloat;
use segment::data_types::vectors::{NamedQuery, VectorInternal, VectorStructInternal};
use segment::json_path::JsonPath;
use segment::types::{
    AnyVariants, Condition, Distance, ExtendedPointId, FieldCondition, Filter, Match, Payload,
    PayloadStorageType, SegmentConfig, ValueVariants, VectorDataConfig, VectorStorageType,
    WithPayloadInterface, WithVector,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shard::count::CountRequestInternal;
use shard::operations::CollectionUpdateOperations;
use shard::operations::point_ops::{
    PointInsertOperationsInternal, PointOperations, PointStructPersisted,
};
use shard::query::query_enum::QueryEnum;
use shard::query::{ScoringQuery, ShardQueryRequest};
use shard::scroll::ScrollRequestInternal;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

/// Current embedding/version number for stored document metadata.
/// Bump this when the embedding model or preprocessing pipeline changes in a
/// way that invalidates previously stored line embeddings.
/// Backwards compatibility: if a workspace DB is missing the `_version` column,
/// we treat all existing documents as version 1.
pub const CURRENT_EMBEDDING_VERSION: u32 = 2;

/// Embedding size (needed to inform Qdrant collection when it is instantiated)
pub const LINE_EMBEDDING_SIZE: usize = 256;
/// We are not actually storing document-level embeddings,
/// but Qdrant requires a vector size to be defined for the collection, so we use a dummy size of 1.
/// This collection is being used for document-level metadata
pub const DOCUMENT_EMBEDDING_SIZE: usize = 1;

/// Vector name used in the documents shard
const DOCUMENTS_VECTOR_NAME: &str = "documents";

/// Vector name used in the line embeddings shard
const LINE_EMBEDDINGS_VECTOR_NAME: &str = "line_embeddings";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocMeta {
    pub path: String,
    pub size_bytes: u64,
    pub mtime: i64,
    pub _version: u32, // used to help manage new embedding models
}

#[derive(Debug)]
pub enum DocumentState {
    Unchanged(String),     // Just the filename, no need to process
    Changed(DocumentInfo), // Full document info for processing
    New(DocumentInfo),     // Full document info for processing
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEmbedding {
    pub path: String,
    pub line_number: i32,
    #[serde(skip)]
    pub embedding: Vec<f32>,
}

impl DocMeta {
    pub fn id(&self) -> u64 {
        // Generate deterministic ID based on path hash for consistent upserts
        fnv1a_hash(self.path.as_bytes())
    }
}

impl LineEmbedding {
    pub fn id(&self) -> u64 {
        // Generate deterministic ID based on path + line number for consistent upserts
        let mut bytes = self.path.as_bytes().to_vec();
        bytes.extend_from_slice(&self.line_number.to_le_bytes());
        fnv1a_hash(&bytes)
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

/// Storage wrapper around Qdrant Edge.
pub struct Store {
    documents_shard: EdgeShard,
    line_embeddings_shard: EdgeShard,
}

impl Store {
    /// Initialize or load storage for a workspace directory
    pub fn open(workspace_dir: &str) -> Result<Self> {
        let document_shard_path = Path::new(workspace_dir).join("documents.qdrant");

        let line_embeddings_shard_path = Path::new(workspace_dir).join("line_embeddings.qdrant");

        // Create shard directories
        std::fs::create_dir_all(&document_shard_path)?;
        std::fs::create_dir_all(&line_embeddings_shard_path)?;

        // Create segment config for the shard
        let mut vector_data_document_shard = HashMap::new();
        vector_data_document_shard.insert(
            DOCUMENTS_VECTOR_NAME.to_string(),
            VectorDataConfig {
                size: DOCUMENT_EMBEDDING_SIZE,
                distance: Distance::Cosine,
                storage_type: VectorStorageType::ChunkedMmap,
                index: Default::default(),
                quantization_config: None,
                multivector_config: None,
                datatype: None,
            },
        );

        let segment_config_document_shard = SegmentConfig {
            vector_data: vector_data_document_shard,
            sparse_vector_data: HashMap::new(),
            payload_storage_type: PayloadStorageType::Mmap,
        };

        let documents_shard =
            EdgeShard::load(&document_shard_path, Some(segment_config_document_shard))?;

        let mut vector_data_line_embeddings_shard = HashMap::new();
        vector_data_line_embeddings_shard.insert(
            LINE_EMBEDDINGS_VECTOR_NAME.to_string(),
            VectorDataConfig {
                size: LINE_EMBEDDING_SIZE,
                distance: Distance::Cosine,
                storage_type: VectorStorageType::ChunkedMmap,
                index: Default::default(),
                quantization_config: None,
                multivector_config: None,
                datatype: None,
            },
        );

        let segment_config_line_embeddings_shard = SegmentConfig {
            vector_data: vector_data_line_embeddings_shard,
            sparse_vector_data: HashMap::new(),
            payload_storage_type: PayloadStorageType::Mmap,
        };

        let line_embeddings_shard = EdgeShard::load(
            &line_embeddings_shard_path,
            Some(segment_config_line_embeddings_shard),
        )?;

        Ok(Self {
            documents_shard,
            line_embeddings_shard,
        })
    }

    pub fn get_existing_docs(&self, paths: &[String]) -> Result<HashMap<String, DocMeta>> {
        let mut existing = HashMap::new();

        for chunk in paths.chunks(1000) {
            let scroll_result = self.documents_shard.scroll(ScrollRequestInternal {
                offset: None,
                order_by: None,
                with_vector: WithVector::Bool(false),
                with_payload: Some(WithPayloadInterface::Bool(true)),
                filter: Some(Filter {
                    must: Some(vec![Condition::Field(FieldCondition::new_match(
                        JsonPath::from_str("path").map_err(|_| {
                            anyhow!("An error occurred while creating JSONPath from 'path'")
                        })?,
                        Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                    ))]),
                    must_not: None,
                    should: None,
                    min_should: None,
                }),
                limit: None,
            });
            let records = match scroll_result {
                Ok(r) => {
                    let (recs, _) = r;
                    recs
                }
                Err(e) => return Err(anyhow!(e.to_string())),
            };
            for record in records {
                match record.payload {
                    None => {}
                    Some(r) => {
                        let meta = payload_to_doc_meta(&r)?;
                        let path = meta.clone().path;
                        existing.insert(path, meta);
                    }
                }
            }
        }

        Ok(existing)
    }

    /// Delete document metadata by path
    pub fn delete_document_metadata(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let mut point_ids: Vec<ExtendedPointId> = vec![];

        // collect all point IDs to be deleted
        for chunk in paths.chunks(1000) {
            let scroll_result = self.documents_shard.scroll(ScrollRequestInternal {
                offset: None,
                order_by: None,
                with_vector: WithVector::Bool(false),
                with_payload: Some(WithPayloadInterface::Bool(true)),
                filter: Some(Filter {
                    must: Some(vec![
                        Condition::Field(FieldCondition::new_match(
                            JsonPath::from_str("path").map_err(|_| {
                                anyhow!("An error occurred while creating JSONPath from 'path'")
                            })?,
                            Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                        )),
                        Condition::Field(FieldCondition::new_match(
                            JsonPath::from_str("_version").map_err(|_| {
                                anyhow!("An error occurred while creating JSONPath from '_version'")
                            })?,
                            Match::new_value(ValueVariants::Integer(
                                CURRENT_EMBEDDING_VERSION as i64,
                            )),
                        )),
                    ]),
                    must_not: None,
                    should: None,
                    min_should: None,
                }),
                limit: None,
            });
            let records = match scroll_result {
                Ok(r) => {
                    let (recs, _) = r;
                    recs
                }
                Err(e) => return Err(anyhow!(e.to_string())),
            };
            for record in records {
                point_ids.push(record.id);
            }
        }

        let operation = CollectionUpdateOperations::PointOperation(PointOperations::DeletePoints {
            ids: point_ids,
        });

        self.documents_shard
            .update(operation)
            .map_err(|e| anyhow!(e.to_string()))?;

        // Flush changes to disk
        self.flush_documents();

        Ok(())
    }

    /// Delete line embeddings by path
    pub fn delete_line_embeddings(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let mut point_ids: Vec<ExtendedPointId> = vec![];

        // collect all point IDs to be deleted
        for chunk in paths.chunks(1000) {
            let scroll_result = self.line_embeddings_shard.scroll(ScrollRequestInternal {
                offset: None,
                order_by: None,
                with_vector: WithVector::Bool(false),
                with_payload: Some(WithPayloadInterface::Bool(true)),
                filter: Some(Filter::new_must(Condition::Field(
                    FieldCondition::new_match(
                        JsonPath::from_str("path").map_err(|_| {
                            anyhow!("An error occurred while creating JSONPath from 'path'")
                        })?,
                        Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                    ),
                ))),
                limit: None,
            });
            let records = match scroll_result {
                Ok(r) => {
                    let (recs, _) = r;
                    recs
                }
                Err(e) => return Err(anyhow!(e.to_string())),
            };
            for record in records {
                point_ids.push(record.id);
            }
        }

        let operation = CollectionUpdateOperations::PointOperation(PointOperations::DeletePoints {
            ids: point_ids,
        });

        self.line_embeddings_shard
            .update(operation)
            .map_err(|e| anyhow!(e.to_string()))?;

        // flush changes to disk
        self.flush_line_embeddings();

        Ok(())
    }

    /// Delete documents and all associated line embeddings by path
    pub fn delete_documents(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }

        // Delete from both tables to maintain synchronization
        self.delete_document_metadata(paths)?;
        self.delete_line_embeddings(paths)?;

        Ok(())
    }

    /// Upsert documents metadata (no embeddings stored)
    pub fn upsert_document_metadata(&self, metas: &[DocMeta]) -> Result<()> {
        if metas.is_empty() {
            return Ok(());
        }

        for chunk in metas.chunks(1000) {
            let mut points: Vec<PointStructPersisted> = vec![];
            for meta in chunk {
                let payload_json =
                    serde_json::to_value(meta).map_err(|e| anyhow!(e.to_string()))?;
                let vector: Vec<f32> = vec![1_f32];
                let point = make_point(meta.id(), vector, payload_json, DOCUMENTS_VECTOR_NAME);
                points.push(point);
            }
            let operation = CollectionUpdateOperations::PointOperation(
                PointOperations::UpsertPoints(PointInsertOperationsInternal::PointsList(points)),
            );
            self.documents_shard
                .update(operation)
                .map_err(|e| anyhow!(e.to_string()))?;

            // flush to disk
            self.flush_documents();
        }

        Ok(())
    }

    /// Upsert line embeddings
    pub fn upsert_line_embeddings(&self, line_embeddings: &[LineEmbedding]) -> Result<()> {
        if line_embeddings.is_empty() {
            return Ok(());
        }

        for chunk in line_embeddings.chunks(1000) {
            let mut points: Vec<PointStructPersisted> = vec![];

            for line_embedding in chunk {
                let payload_json =
                    serde_json::to_value(line_embedding).map_err(|e| anyhow!(e.to_string()))?;
                let point = make_point(
                    line_embedding.id(),
                    line_embedding.embedding.clone(),
                    payload_json,
                    LINE_EMBEDDINGS_VECTOR_NAME,
                );
                points.push(point);
            }

            let operation = CollectionUpdateOperations::PointOperation(
                PointOperations::UpsertPoints(PointInsertOperationsInternal::PointsList(points)),
            );
            self.line_embeddings_shard
                .update(operation)
                .map_err(|e| anyhow!(e.to_string()))?;

            // flush to disk
            self.flush_line_embeddings();
        }

        Ok(())
    }

    /// Get workspace statistics
    pub fn get_stats(&self) -> Result<WorkspaceStats> {
        let total_documents = self.count_documents()?;

        Ok(WorkspaceStats {
            total_documents,
            has_index: true,
            index_type: Some("HNSW".to_string()),
        })
    }

    /// Get paths for all stored documents
    pub fn get_all_document_paths(&self) -> Result<Vec<String>> {
        let scroll_result = self
            .documents_shard
            .scroll(ScrollRequestInternal {
                offset: None,
                order_by: None,
                with_vector: WithVector::Bool(false),
                with_payload: Some(WithPayloadInterface::Bool(true)),
                filter: None,
                limit: None,
            })
            .map_err(|e| anyhow!(e.to_string()))?;

        let (records, _) = scroll_result;
        let mut paths: Vec<String> = vec![];

        for record in records {
            if let Some(p) = record.payload {
                let doc_meta = payload_to_doc_meta(&p)?;
                paths.push(doc_meta.path);
            }
        }

        Ok(paths)
    }

    /// Search within line embeddings
    pub fn search_line_embeddings(
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

        let mut all_results: Vec<RankedLine> = vec![];

        for chunk in subset_paths.chunks(1000) {
            let query: Vec<f32> = query_vec.into();
            let vector: VectorInternal = query.into();
            let score_threshold: Option<OrderedFloat<f32>> =
                max_distance.map(|max_dist| OrderedFloat(1_f32 - max_dist));
            let results = self
                .line_embeddings_shard
                .query(ShardQueryRequest {
                    prefetches: vec![],
                    query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                        query: vector,
                        using: Some(LINE_EMBEDDINGS_VECTOR_NAME.to_string()),
                    }))),
                    filter: Some(Filter::new_must(Condition::Field(
                        FieldCondition::new_match(
                            JsonPath::from_str("path").map_err(|_| {
                                anyhow!("An error occurred while creating JSONPath from 'path'")
                            })?,
                            Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                        ),
                    ))),
                    score_threshold,
                    limit: top_k * 2,
                    offset: 0,
                    params: None,
                    with_vector: WithVector::Bool(false),
                    with_payload: WithPayloadInterface::Bool(true),
                })
                .map_err(|e| anyhow!(e.to_string()))?;

            for result in results {
                if let Some(p) = result.payload {
                    let line_embd = payload_to_line_embedding(&p)?;
                    let ranked_line = RankedLine {
                        line_number: line_embd.line_number,
                        path: line_embd.path,
                        distance: 1_f32 - result.score,
                    };
                    all_results.push(ranked_line);
                }
            }
        }

        all_results.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(top_k);

        Ok(all_results)
    }

    /// Analyze the state of documents within the workspace
    pub fn analyze_document_states(&self, file_paths: &[String]) -> Result<Vec<DocumentState>> {
        // Get existing document metadata from workspace
        let existing_docs = self.get_existing_docs(file_paths)?;

        let mut states = Vec::new();

        for file_path in file_paths {
            // Read current file metadata
            let current_meta = match std::fs::metadata(file_path) {
                Ok(metadata) => {
                    let size_bytes = metadata.len();
                    let mtime = metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    DocMeta {
                        path: file_path.clone(),
                        size_bytes,
                        mtime,
                        _version: CURRENT_EMBEDDING_VERSION,
                    }
                }
                Err(_) => {
                    // File doesn't exist, skip it
                    continue;
                }
            };

            // Check if document exists in workspace and has changed
            match existing_docs.get(file_path) {
                Some(existing_meta) => {
                    if existing_meta.size_bytes != current_meta.size_bytes
                        || existing_meta.mtime != current_meta.mtime
                        || existing_meta._version != CURRENT_EMBEDDING_VERSION
                    {
                        // Document has changed
                        let content = std::fs::read_to_string(file_path)?;
                        states.push(DocumentState::Changed(DocumentInfo {
                            filename: file_path.clone(),
                            content,
                            meta: current_meta,
                        }));
                    } else {
                        // Document unchanged
                        states.push(DocumentState::Unchanged(file_path.clone()));
                    }
                }
                None => {
                    // New document
                    let content = std::fs::read_to_string(file_path)?;
                    states.push(DocumentState::New(DocumentInfo {
                        filename: file_path.clone(),
                        content,
                        meta: current_meta,
                    }));
                }
            }
        }

        Ok(states)
    }

    /// Get the number of indexed points in the documents shard
    pub fn count_documents(&self) -> Result<usize> {
        let count = self
            .documents_shard
            .count(CountRequestInternal {
                filter: None,
                exact: true,
            })
            .map_err(|e| anyhow!(e.to_string()))?;

        Ok(count)
    }

    /// Flush all documents data to disk.
    pub fn flush_documents(&self) {
        self.documents_shard.flush();
    }

    /// Flush all line embeddings data to disk.
    pub fn flush_line_embeddings(&self) {
        self.line_embeddings_shard.flush();
    }
}

/// Generate a stable hash for a byte slice using the FNV-1a algorithm.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Create a point struct for upserting.
fn make_point(
    id: u64,
    vector: Vec<f32>,
    payload: Value,
    vector_name: &str,
) -> PointStructPersisted {
    let mut vectors = HashMap::new();
    vectors.insert(vector_name.to_string(), VectorInternal::from(vector));

    PointStructPersisted {
        id: ExtendedPointId::NumId(id),
        vector: VectorStructInternal::Named(vectors).into(),
        payload: Some(json_to_payload(payload)),
    }
}

/// Convert JSON value (DocMeta or LineEmbedding struct) to Qdrant Payload.
fn json_to_payload(value: Value) -> Payload {
    if let Value::Object(map) = value {
        let mut payload = Payload::default();
        for (k, v) in map {
            payload.0.insert(k, v);
        }
        payload
    } else {
        Payload::default()
    }
}

/// Convert Qdrant Payload back to DocMeta
fn payload_to_doc_meta(payload: &Payload) -> Result<DocMeta> {
    let json_map: serde_json::Map<String, Value> = payload
        .0
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let json_value = Value::Object(json_map);
    serde_json::from_value(json_value).map_err(|e| anyhow!(e.to_string()))
}

/// Convert Qdrant Payload back to LineEmbedding
fn payload_to_line_embedding(payload: &Payload) -> Result<LineEmbedding> {
    let json_map: serde_json::Map<String, Value> = payload
        .0
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let json_value = Value::Object(json_map);
    serde_json::from_value(json_value).map_err(|e| anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    // Helper function to create a test store
    fn create_test_store() -> (Store, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = Store::open(temp_dir.path().to_str().unwrap()).expect("Failed to create store");
        (store, temp_dir)
    }

    // Helper function to create test documents
    fn create_test_docs() -> (Vec<DocMeta>, Vec<Vec<f32>>) {
        let docs = vec![
            DocMeta {
                path: "/test/doc1.txt".to_string(),
                size_bytes: 100,
                mtime: 1234567890,
                _version: CURRENT_EMBEDDING_VERSION,
            },
            DocMeta {
                path: "/test/doc2.txt".to_string(),
                size_bytes: 200,
                mtime: 1234567891,
                _version: CURRENT_EMBEDDING_VERSION,
            },
            DocMeta {
                path: "/test/doc3.txt".to_string(),
                size_bytes: 150,
                mtime: 1234567892,
                _version: CURRENT_EMBEDDING_VERSION,
            },
        ];

        let embeddings = vec![
            vec![0.1; 256],  // All 0.1
            vec![0.5; 256],  // All 0.5
            vec![0.75; 256], // All 0.5
        ];

        (docs, embeddings)
    }

    #[test]
    fn test_store_creation_and_stats_empty() {
        let (store, _temp_dir) = create_test_store();

        let stats = store.get_stats().expect("Failed to get stats");

        assert_eq!(stats.total_documents, 0);
        assert!(stats.has_index);
        assert_eq!(stats.index_type, Some("HNSW".to_string()));

        // explicitly drop store before _temp_dir to avoid
        // EdgeShard panicking when trying to flush to a non-existing dir
        // (caused by _temp_dir being dropped before store)
        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_upsert_documents_and_stats() {
        let (store, _temp_dir) = create_test_store();
        let (docs, embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .expect("Failed to upsert documents");

        let line_embeddings: Vec<LineEmbedding> = docs
            .iter()
            .enumerate()
            .map(|(i, doc)| LineEmbedding {
                path: doc.path.clone(),
                line_number: i as i32,
                embedding: embeddings[i].clone(),
            })
            .collect();

        store
            .upsert_line_embeddings(&line_embeddings)
            .expect("Failed to upsert line embeddings");

        // Check stats
        let stats = store.get_stats().expect("Failed to get stats");

        assert_eq!(stats.total_documents, 3);
        assert!(stats.has_index);
        assert_eq!(stats.index_type, Some("HNSW".to_string()));

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_search_line_embeddings() {
        let (store, _temp_dir) = create_test_store();
        let (docs, embeddings) = create_test_docs();

        let line_embeddings: Vec<LineEmbedding> = docs
            .iter()
            .enumerate()
            .map(|(i, doc)| LineEmbedding {
                path: doc.path.clone(),
                line_number: i as i32,
                embedding: embeddings[i].clone(),
            })
            .collect();

        store
            .upsert_line_embeddings(&line_embeddings)
            .expect("Failed to upsert line embeddings");

        // Perform search
        let exact_match_query: Vec<f32> = vec![0.1; 256];
        let search_results = store
            .search_line_embeddings(
                exact_match_query.as_slice(),
                &["/test/doc1.txt".to_string()],
                1,
                Some(0.1_f32),
            )
            .expect("Should be able to retrieve search results");
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].line_number, 0);
        assert_eq!(search_results[0].path, docs[0].path);
        assert!(search_results[0].distance < 0.1);

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_get_all_document_paths() {
        let (store, _temp_dir) = create_test_store();
        let (docs, _embeddings) = create_test_docs();

        // Initially should be empty
        let paths = store
            .get_all_document_paths()
            .expect("Failed to get document paths");
        assert!(paths.is_empty());

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .expect("Failed to upsert documents");

        // Should now have paths
        let paths = store
            .get_all_document_paths()
            .expect("Failed to get document paths");

        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"/test/doc1.txt".to_string()));
        assert!(paths.contains(&"/test/doc2.txt".to_string()));
        assert!(paths.contains(&"/test/doc3.txt".to_string()));

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_get_existing_docs() {
        let (store, _temp_dir) = create_test_store();
        let (docs, _embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .expect("Failed to upsert documents");

        // Test getting existing docs
        let query_paths = vec![
            "/test/doc1.txt".to_string(),
            "/test/doc2.txt".to_string(),
            "/test/nonexistent.txt".to_string(),
        ];

        let existing = store
            .get_existing_docs(&query_paths)
            .expect("Failed to get existing docs");

        assert_eq!(existing.len(), 2);
        assert!(existing.contains_key("/test/doc1.txt"));
        assert!(existing.contains_key("/test/doc2.txt"));
        assert!(!existing.contains_key("/test/nonexistent.txt"));

        // Verify metadata
        let doc1_meta = existing.get("/test/doc1.txt").unwrap();
        assert_eq!(doc1_meta.size_bytes, 100);
        assert_eq!(doc1_meta.mtime, 1234567890);

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_delete_documents() {
        let (store, _temp_dir) = create_test_store();
        let (docs, _embeddings) = create_test_docs();

        // Insert documents
        store
            .upsert_document_metadata(&docs)
            .expect("Failed to upsert documents");

        // Verify all documents exist
        let all_paths = store
            .get_all_document_paths()
            .expect("Failed to get document paths");
        assert_eq!(all_paths.len(), 3);

        // Delete some documents
        let to_delete = vec!["/test/doc1.txt".to_string(), "/test/doc3.txt".to_string()];
        store
            .delete_documents(&to_delete)
            .expect("Failed to delete documents");

        // Verify only doc2 remains
        let remaining_paths = store
            .get_all_document_paths()
            .expect("Failed to get document paths");
        assert_eq!(remaining_paths.len(), 1);
        assert!(remaining_paths.contains(&"/test/doc2.txt".to_string()));

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_upsert_replaces_existing() {
        let (store, _temp_dir) = create_test_store();

        // Insert initial document
        let initial_doc = DocMeta {
            path: "/test/doc.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
            _version: CURRENT_EMBEDDING_VERSION,
        };
        let _initial_embedding = [vec![1.0, 2.0, 3.0, 4.0]];

        store
            .upsert_document_metadata(&[initial_doc])
            .expect("Failed to insert initial document");

        // Verify document exists
        let paths = store.get_all_document_paths().expect("Failed to get paths");
        assert_eq!(paths.len(), 1);

        // Update the same document
        // NOTE: this works because the id of the data point depends on the hashing of the path.
        // same path = same hash -> the update results in a replacement rather than an append
        let updated_doc = DocMeta {
            path: "/test/doc.txt".to_string(),
            size_bytes: 200,
            mtime: 2000,
            _version: CURRENT_EMBEDDING_VERSION,
        };
        let _updated_embedding = [vec![5.0, 6.0, 7.0, 8.0]];

        store
            .upsert_document_metadata(&[updated_doc])
            .expect("Failed to update document");

        // Should still have only one document
        let paths = store.get_all_document_paths().expect("Failed to get paths");
        assert_eq!(paths.len(), 1);

        // Verify metadata was updated
        let existing = store
            .get_existing_docs(&["/test/doc.txt".to_string()])
            .expect("Failed to get existing docs");
        let doc_meta = existing.get("/test/doc.txt").unwrap();
        assert_eq!(doc_meta.size_bytes, 200);
        assert_eq!(doc_meta.mtime, 2000);

        drop(store);
        drop(_temp_dir);
    }

    #[test]
    fn test_doc_meta_id_generation() {
        let doc1 = DocMeta {
            path: "test1.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
            _version: CURRENT_EMBEDDING_VERSION,
        };
        let doc2 = DocMeta {
            path: "test2.txt".to_string(),
            size_bytes: 100,
            mtime: 1000,
            _version: CURRENT_EMBEDDING_VERSION,
        };

        let id1 = doc1.id();
        let id2 = doc2.id();

        // IDs should be different (random generation)
        assert_ne!(id1, id2);
    }

    // Helper to create test files for analyze_document_states tests
    fn create_test_files(temp_dir: &tempfile::TempDir) -> Vec<String> {
        use std::fs;

        let file1_path = temp_dir.path().join("test1.txt");
        let file2_path = temp_dir.path().join("test2.txt");
        let file3_path = temp_dir.path().join("test3.txt");

        fs::write(&file1_path, "This is test file 1\nWith multiple lines").unwrap();
        fs::write(&file2_path, "This is test file 2\nWith different content").unwrap();
        fs::write(&file3_path, "This is test file 3\nWith more content").unwrap();

        vec![
            file1_path.to_string_lossy().to_string(),
            file2_path.to_string_lossy().to_string(),
            file3_path.to_string_lossy().to_string(),
        ]
    }

    #[test]
    fn test_analyze_document_states_all_new() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_paths = create_test_files(&temp_dir);

        // Create empty store
        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        let states = store.analyze_document_states(&file_paths).unwrap();

        assert_eq!(states.len(), 3);

        // All should be new documents
        for state in &states {
            if let DocumentState::New(doc_info) = state {
                assert!(file_paths.contains(&doc_info.filename));
                assert!(!doc_info.content.is_empty());
                assert!(doc_info.meta.size_bytes > 0);
                assert!(doc_info.meta.mtime > 0);
            } else {
                panic!("Expected New document state");
            }
        }

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_analyze_document_states_unchanged() {
        use std::fs;
        use std::time::UNIX_EPOCH;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_paths = create_test_files(&temp_dir);

        // Create store and add documents
        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        // Insert documents with current metadata
        let mut docs = Vec::new();
        for path in &file_paths {
            let metadata = fs::metadata(path).unwrap();
            let doc_meta = DocMeta {
                path: path.clone(),
                size_bytes: metadata.len(),
                mtime: metadata
                    .modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                _version: CURRENT_EMBEDDING_VERSION,
            };
            docs.push(doc_meta);
        }
        store.upsert_document_metadata(&docs).unwrap();

        // Analyze states - should all be unchanged
        let states = store.analyze_document_states(&file_paths).unwrap();

        assert_eq!(states.len(), 3);

        for state in &states {
            if let DocumentState::Unchanged(filename) = state {
                assert!(file_paths.contains(filename));
            } else {
                panic!("Expected Unchanged document state");
            }
        }

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_analyze_document_states_changed() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_paths = create_test_files(&temp_dir);

        // Create store and add documents with old metadata
        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        let mut docs = Vec::new();
        for path in &file_paths {
            let doc_meta = DocMeta {
                path: path.clone(),
                size_bytes: 10, // Different from actual size
                mtime: 1000,    // Old timestamp
                _version: 1,    // simulate old version
            };
            docs.push(doc_meta);
        }
        store.upsert_document_metadata(&docs).unwrap();

        // Analyze states - should all be changed
        let states = store.analyze_document_states(&file_paths).unwrap();

        assert_eq!(states.len(), 3);

        for state in &states {
            if let DocumentState::Changed(doc_info) = state {
                assert!(file_paths.contains(&doc_info.filename));
                assert!(!doc_info.content.is_empty());
            } else {
                panic!("Expected Changed document state");
            }
        }

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_analyze_document_states_mixed() {
        use std::fs;
        use std::time::UNIX_EPOCH;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_paths = create_test_files(&temp_dir);

        // Create store and add only the first document
        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        let metadata = fs::metadata(&file_paths[0]).unwrap();
        let doc_meta = DocMeta {
            path: file_paths[0].clone(),
            size_bytes: metadata.len(),
            mtime: metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            _version: CURRENT_EMBEDDING_VERSION,
        };
        store.upsert_document_metadata(&[doc_meta]).unwrap();

        // Analyze states
        let states = store.analyze_document_states(&file_paths).unwrap();

        assert_eq!(states.len(), 3);

        // First should be unchanged, others should be new
        let mut unchanged_count = 0;
        let mut new_count = 0;

        for state in &states {
            match state {
                DocumentState::Unchanged(filename) => {
                    assert_eq!(filename, &file_paths[0]);
                    unchanged_count += 1;
                }
                DocumentState::New(doc_info) => {
                    assert!(file_paths[1..].contains(&doc_info.filename));
                    new_count += 1;
                }
                _ => panic!("Unexpected document state"),
            }
        }

        assert_eq!(unchanged_count, 1);
        assert_eq!(new_count, 2);

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_analyze_document_states_version_mismatch() {
        use std::fs;
        use std::time::UNIX_EPOCH;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_paths = create_test_files(&temp_dir);

        // Create store and add documents with old version but correct size/mtime
        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        let mut old_docs = Vec::new();
        for path in &file_paths {
            let metadata = fs::metadata(path).unwrap();
            let doc_meta = DocMeta {
                path: path.clone(),
                size_bytes: metadata.len(),
                mtime: metadata
                    .modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                _version: 1, // older version than CURRENT_EMBEDDING_VERSION (2)
            };
            old_docs.push(doc_meta);
        }
        store.upsert_document_metadata(&old_docs).unwrap();

        let states = store.analyze_document_states(&file_paths).unwrap();
        assert_eq!(states.len(), 3);
        for state in &states {
            match state {
                DocumentState::Changed(info) => {
                    assert!(file_paths.contains(&info.filename));
                }
                _ => panic!("Expected Changed state due to version mismatch"),
            }
        }

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_analyze_document_states_nonexistent_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut file_paths = create_test_files(&temp_dir);

        // Add a nonexistent file to the list
        file_paths.push("/nonexistent/file.txt".to_string());

        let store = Store::open(temp_dir.path().to_str().unwrap()).unwrap();

        let states = store.analyze_document_states(&file_paths).unwrap();

        // Should only have states for existing files
        assert_eq!(states.len(), 3);

        for state in &states {
            if let DocumentState::New(doc_info) = state {
                assert_ne!(doc_info.filename, "/nonexistent/file.txt");
            }
        }

        drop(store);
        drop(temp_dir);
    }

    #[test]
    fn test_json_to_payload_doc_meta() {
        let doc_meta = DocMeta {
            path: "hello.txt".to_string(),
            size_bytes: 1200_u64,
            mtime: 1234567890,
            _version: CURRENT_EMBEDDING_VERSION,
        };
        let doc_meta_json =
            serde_json::to_value(doc_meta).expect("Should be able to conver DocMeta to JSON Value");
        let qdrant_payload = json_to_payload(doc_meta_json);
        assert!(qdrant_payload.contains_key("path"));
        assert!(qdrant_payload.contains_key("size_bytes"));
        assert!(qdrant_payload.contains_key("mtime"));
        assert!(qdrant_payload.contains_key("_version"));
        for (k, v) in qdrant_payload.0.iter() {
            match k.as_str() {
                "path" => assert_eq!(v, &Value::from("hello.txt")),
                "size_bytes" => assert_eq!(v, &Value::from(1200)),
                "mtime" => assert_eq!(v, &Value::from(1234567890)),
                "_version" => assert_eq!(v, &Value::from(CURRENT_EMBEDDING_VERSION)),
                _ => panic!("Unexpected key: {}", k),
            }
        }
    }

    #[test]
    fn test_json_to_payload_line_embedding() {
        let line_embedding = LineEmbedding {
            path: "hello.txt".to_string(),
            line_number: 12,
            embedding: vec![0.1, 0.3, 0.4, 0.5],
        };
        let doc_meta_json = serde_json::to_value(line_embedding)
            .expect("Should be able to conver LineEmbedding to JSON Value");
        let qdrant_payload = json_to_payload(doc_meta_json);
        assert!(qdrant_payload.contains_key("path"));
        assert!(qdrant_payload.contains_key("line_number"));
        assert!(!qdrant_payload.contains_key("embedding"));
        for (k, v) in qdrant_payload.0.iter() {
            match k.as_str() {
                "path" => assert_eq!(v, &Value::from("hello.txt")),
                "line_number" => assert_eq!(v, &Value::from(12)),
                _ => panic!("Unexpected key: {}", k),
            }
        }
    }

    #[test]
    fn test_payload_to_doc_meta() {
        let json_value = json!({
            "path": "hello.txt",
            "size_bytes": 1000_u64,
            "mtime": 1234567890_i64,
            "_version": CURRENT_EMBEDDING_VERSION,
        });
        let map: serde_json::Map<String, Value> = json_value
            .as_object()
            .expect("Should be able to convert JSON value to map")
            .clone();
        let payload = Payload::from(map);
        let doc_meta =
            payload_to_doc_meta(&payload).expect("Should be able to convert Payload to DocMeta");
        assert_eq!(doc_meta.path, "hello.txt");
        assert_eq!(doc_meta.size_bytes, 1000_u64);
        assert_eq!(doc_meta.mtime, 1234567890_i64);
        assert_eq!(doc_meta._version, CURRENT_EMBEDDING_VERSION);
    }

    #[test]
    fn test_payload_to_line_embedding() {
        let json_value = json!({
            "path": "hello.txt",
            "line_number": 12_i32,
        });
        let map: serde_json::Map<String, Value> = json_value
            .as_object()
            .expect("Should be able to convert JSON value to map")
            .clone();
        let payload = Payload::from(map);
        let line_embedding = payload_to_line_embedding(&payload)
            .expect("Should be able to convert Payload to DocMeta");
        assert_eq!(line_embedding.path, "hello.txt");
        assert_eq!(line_embedding.line_number, 12_i32);
        assert!(line_embedding.embedding.is_empty());
    }
}
