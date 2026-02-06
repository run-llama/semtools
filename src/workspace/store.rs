//! Qdrant Edge storage wrapper
use anyhow::{Result, anyhow};

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
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::search::DocumentInfo;

/// Current embedding/version number for stored document metadata.
/// Bump this when the embedding model or preprocessing pipeline changes in a
/// way that invalidates previously stored line embeddings.
/// Backwards compatibility: if a workspace DB is missing the `_version` column,
/// we treat all existing documents as version 1.
pub const CURRENT_EMBEDDING_VERSION: u32 = 2;

/// Embedding size (needed to inform Qdrant collection when it is instantiated)
pub const EMBEDDING_SIZE: usize = 256;

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

/// Metadata stored with each vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPayload {
    /// Relative file path from repo root
    pub path: String,
    /// Chunk index within the file
    pub chunk_index: usize,
    /// Starting line number
    pub start_line: usize,
    /// Ending line number
    pub end_line: usize,
    /// The actual text content
    pub text: String,
    /// File hash for change detection
    pub file_hash: String,
}

/// A search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Score (similarity)
    pub score: f32,
    /// The payload
    pub payload: ChunkPayload,
}

/// Storage wrapper around Qdrant Edge.
pub struct Store {
    documents_shard: EdgeShard,
    line_embeddings_shard: EdgeShard,
}

impl Store {
    /// Initialize or load storage for a workspace directory
    pub fn open(workspace_dir: &str) -> Result<Self> {
        let document_shard_path = Path::new(workspace_dir).join("documents");

        let line_embeddings_shard_path = Path::new(workspace_dir).join("line_embeddings");

        // Create shard directories
        std::fs::create_dir_all(&document_shard_path)?;
        std::fs::create_dir_all(&line_embeddings_shard_path)?;

        // Create segment config for the shard
        let mut vector_data_document_shard = HashMap::new();
        vector_data_document_shard.insert(
            DOCUMENTS_VECTOR_NAME.to_string(),
            VectorDataConfig {
                size: EMBEDDING_SIZE,
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
            DOCUMENTS_VECTOR_NAME.to_string(),
            VectorDataConfig {
                size: EMBEDDING_SIZE,
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
                    must: Some(vec![
                        Condition::Field(FieldCondition::new_match(
                            JsonPath {
                                first_key: "path".to_string(),
                                rest: vec![],
                            },
                            Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                        )),
                        Condition::Field(FieldCondition::new_match(
                            JsonPath {
                                first_key: "version".to_string(),
                                rest: vec![],
                            },
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
                            JsonPath {
                                first_key: "path".to_string(),
                                rest: vec![],
                            },
                            Match::from(AnyVariants::Strings(chunk.iter().cloned().collect())),
                        )),
                        Condition::Field(FieldCondition::new_match(
                            JsonPath {
                                first_key: "version".to_string(),
                                rest: vec![],
                            },
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
                        JsonPath {
                            first_key: "path".to_string(),
                            rest: vec![],
                        },
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

        let mut point_id = self.count_documents()? as u64;

        for chunk in metas.chunks(1000) {
            let mut points: Vec<PointStructPersisted> = vec![];
            for meta in chunk {
                point_id += 1_u64;
                let payload_json =
                    serde_json::to_value(meta).map_err(|e| anyhow!(e.to_string()))?;
                let vector: Vec<f32> = vec![];
                let point = make_point(point_id, vector, payload_json, DOCUMENTS_VECTOR_NAME);
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
        let mut point_id = self.count_line_embeddings()? as u64;

        for chunk in line_embeddings.chunks(1000) {
            let mut points: Vec<PointStructPersisted> = vec![];

            for line_embedding in chunk {
                point_id += 1_u64;
                let payload_json =
                    serde_json::to_value(line_embedding).map_err(|e| anyhow!(e.to_string()))?;
                let point = make_point(
                    point_id,
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
                            JsonPath {
                                first_key: "path".to_string(),
                                rest: vec![],
                            },
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
                        distance: result.score,
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

    /// Get the number of indexed points in the documents shard
    pub fn count_line_embeddings(&self) -> Result<usize> {
        let count = self
            .line_embeddings_shard
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
