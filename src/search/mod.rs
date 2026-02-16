use anyhow::Result;
use model2vec_rs::model::StaticModel;
use simsimd::SpatialSimilarity;
use std::cmp::{max, min};
use std::fs::read_to_string;

#[cfg(feature = "workspace")]
use crate::workspace::store::{DocMeta, DocumentState, RankedLine};

#[cfg(feature = "workspace")]
use crate::workspace::{
    Workspace,
    store::{LineEmbedding, Store},
};

pub const MODEL_NAME: &str = "minishlab/potion-multilingual-128M";

pub struct Document {
    pub filename: String,
    pub lines: Vec<String>,
    pub embeddings: Vec<Vec<f32>>,
}

#[cfg(feature = "workspace")]
#[derive(Debug)]
pub struct DocumentInfo {
    pub filename: String,
    pub content: String,
    pub meta: DocMeta,
}

#[derive(Default)]
pub struct SearchConfig {
    pub n_lines: usize,
    pub top_k: usize,
    pub max_distance: Option<f64>,
    pub ignore_case: bool,
}

pub struct SearchResult {
    pub filename: String,
    pub lines: Vec<String>,
    pub start: usize,
    pub end: usize,
    pub match_line: usize, // The actual line number that matched
    pub distance: f64,
}

pub(crate) fn create_document_from_content(
    filename: String,
    content: &str,
    model: &StaticModel,
    ignore_case: bool,
) -> Option<Document> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return None;
    }

    let owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

    let lines_for_embedding = if ignore_case {
        owned_lines.iter().map(|s| s.to_lowercase()).collect()
    } else {
        owned_lines.clone()
    };

    let embeddings = model.encode_with_args(&lines_for_embedding, Some(2048), 16384);
    Some(Document {
        filename,
        lines: owned_lines,
        embeddings,
    })
}

pub fn search_documents(
    documents: &[Document],
    query_embedding: &[f32],
    config: &SearchConfig,
) -> Vec<SearchResult> {
    let mut search_results = Vec::new();

    for doc in documents {
        for (idx, line_embedding) in doc.embeddings.iter().enumerate() {
            let distance = f32::cosine(query_embedding, line_embedding);
            if let Some(distance) = distance {
                let distance_threshold = config.max_distance.unwrap_or(100.0);
                if distance < distance_threshold {
                    let bottom_range = max(0, idx.saturating_sub(config.n_lines));
                    let top_range = min(doc.lines.len(), idx + config.n_lines + 1);

                    search_results.push(SearchResult {
                        filename: doc.filename.clone(),
                        lines: doc.lines[bottom_range..top_range].to_vec(),
                        distance,
                        start: bottom_range,
                        end: top_range,
                        match_line: idx,
                    })
                }
            }
        }
    }

    // Sort by distance (best matches first)
    search_results.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // If threshold is specified, return all results under threshold
    // Otherwise, limit to top_k results
    if config.max_distance.is_some() {
        search_results
    } else {
        search_results.into_iter().take(config.top_k).collect()
    }
}

pub fn search_files(
    files: &[String],
    query: &str,
    model: &StaticModel,
    config: &SearchConfig,
) -> Result<Vec<SearchResult>> {
    let mut documents = Vec::new();
    for f in files {
        let content = read_to_string(f)?;
        if let Some(doc) =
            create_document_from_content(f.clone(), &content, model, config.ignore_case)
        {
            documents.push(doc);
        }
    }

    let query_embedding = model.encode_single(query);

    let results = search_documents(&documents, &query_embedding, config);

    Ok(results)
}

#[cfg(feature = "workspace")]
pub async fn search_with_workspace(
    files: &[String],
    query: &str,
    model: &StaticModel,
    config: &SearchConfig,
    workspace_name: Option<&str>,
) -> Result<Vec<RankedLine>> {
    let query_embedding = model.encode_single(query);
    let ws = Workspace::open(workspace_name)?;
    let store = Store::open(&ws.config.root_dir)?;

    // Step 1: Analyze document states (changed/new/unchanged)
    let doc_states = store.analyze_document_states(files)?;

    // Step 2: Process documents that need embedding updates
    let mut line_embeddings_to_upsert = Vec::new();
    let mut docs_to_upsert = Vec::new();

    for state in &doc_states {
        match state {
            DocumentState::Changed(doc_info) | DocumentState::New(doc_info) => {
                // Generate line-by-line embeddings and store them
                if let Some(doc) = create_document_from_content(
                    doc_info.filename.clone(),
                    &doc_info.content,
                    model,
                    config.ignore_case,
                ) {
                    // Create LineEmbedding entries for each line
                    for (line_idx, embedding) in doc.embeddings.iter().enumerate() {
                        line_embeddings_to_upsert.push(LineEmbedding {
                            path: doc_info.filename.clone(),
                            line_number: line_idx as i32, // Store as 0-based for consistency
                            embedding: embedding.clone(),
                        });
                    }
                    // Also track document metadata for change detection
                    docs_to_upsert.push(doc_info.meta.clone());
                }
            }
            DocumentState::Unchanged(_) => {
                // Skip - already in workspace and unchanged
            }
        }
    }

    // Step 3: Update workspace with new/changed line embeddings
    if !line_embeddings_to_upsert.is_empty() {
        eprintln!(
            "Updating workspace with {} lines from new/changed docs...",
            line_embeddings_to_upsert.len()
        );
        store.upsert_line_embeddings(&line_embeddings_to_upsert)?;
    }

    // Also update document metadata for tracking changes
    if !docs_to_upsert.is_empty() {
        eprintln!(
            "Updating workspace with {} new/changed documents...",
            docs_to_upsert.len()
        );
        store.upsert_document_metadata(&docs_to_upsert)?;
    }

    // Step 4: Search line embeddings directly from the workspace
    let max_distance = config.max_distance.map(|d| d as f32);
    let ranked_lines =
        store.search_line_embeddings(&query_embedding, files, config.top_k, max_distance)?;

    Ok(ranked_lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    const MODEL_NAME: &str = "minishlab/potion-multilingual-128M";

    // Global model instance shared across all tests
    static MODEL: OnceLock<StaticModel> = OnceLock::new();

    fn get_model() -> &'static StaticModel {
        MODEL.get_or_init(|| {
            StaticModel::from_pretrained(MODEL_NAME, None, None, None)
                .expect("Failed to load model for tests")
        })
    }

    fn create_test_document_with_model(filename: &str, lines: Vec<&str>) -> Document {
        let model = get_model();
        let content = lines.join("\n");
        create_document_from_content(filename.to_string(), &content, model, false)
            .expect("Failed to create test document")
    }

    fn create_test_config() -> SearchConfig {
        SearchConfig {
            n_lines: 3,
            top_k: 3,
            max_distance: None,
            ignore_case: false,
        }
    }

    #[test]
    fn test_search_documents_basic() {
        let model = get_model();
        let doc1 = create_test_document_with_model(
            "file1.txt",
            vec!["hello world", "goodbye world", "test line"],
        );
        let doc2 =
            create_test_document_with_model("file2.txt", vec!["another test", "more content"]);
        let documents = vec![doc1, doc2];

        let query = "test query";
        let query_embedding = model.encode_single(query);
        let config = create_test_config();

        let results = search_documents(&documents, &query_embedding, &config);

        // Should return results (exact matches depend on embedding similarity)
        assert!(!results.is_empty());
        // Results should be sorted by distance
        for i in 1..results.len() {
            assert!(results[i - 1].distance <= results[i].distance);
        }
    }

    #[test]
    fn test_search_documents_with_max_distance() {
        let model = get_model();
        let doc = create_test_document_with_model("test.txt", vec!["line 1", "line 2", "line 3"]);
        let documents = vec![doc];

        let query = "test";
        let query_embedding = model.encode_single(query);
        let mut config = create_test_config();
        config.max_distance = Some(0.5); // Very restrictive threshold

        let results = search_documents(&documents, &query_embedding, &config);

        // With restrictive threshold, should have fewer or no results
        for result in &results {
            assert!(result.distance < 0.5);
        }
    }

    #[test]
    fn test_search_documents_top_k_limit() {
        let model = get_model();
        let doc = create_test_document_with_model(
            "test.txt",
            vec!["line 1", "line 2", "line 3", "line 4", "line 5"],
        );
        let documents = vec![doc];

        let query = "test";
        let query_embedding = model.encode_single(query);
        let mut config = create_test_config();
        config.top_k = 2; // Limit to 2 results
        config.max_distance = None; // Use top_k instead of threshold

        let results = search_documents(&documents, &query_embedding, &config);

        assert!(results.len() <= 2);
    }

    #[test]
    fn test_search_result_context_calculation() {
        let model = get_model();
        let doc = create_test_document_with_model(
            "test.txt",
            vec!["line 0", "line 1", "line 2", "line 3", "line 4", "line 5"],
        );
        let documents = vec![doc];

        let query = "test";
        let query_embedding = model.encode_single(query);
        let mut config = create_test_config();
        config.n_lines = 1; // 1 line of context before/after

        let results = search_documents(&documents, &query_embedding, &config);

        if !results.is_empty() {
            let result = &results[0];
            assert_eq!(result.lines.len(), 3);
        }
    }

    #[test]
    fn test_context_at_file_boundaries() {
        let model = get_model();
        let doc = create_test_document_with_model("small.txt", vec!["first", "second"]);
        let documents = vec![doc];

        let query = "first"; // Query that should match the first line
        let query_embedding = model.encode_single(query);
        let mut config = create_test_config();
        config.n_lines = 5; // More context than available

        let results = search_documents(&documents, &query_embedding, &config);

        if !results.is_empty() {
            let result = &results[0];
            // Should not exceed file boundaries
            assert_eq!(result.start, 0);
            assert_eq!(result.end, 2); // Length of file
            assert!(result.lines.len() <= 2);
        }
    }

    #[test]
    fn test_multiple_documents_search() {
        let model = get_model();
        let doc1 = create_test_document_with_model("file1.txt", vec!["apple", "banana"]);
        let doc2 = create_test_document_with_model("file2.txt", vec!["orange", "grape"]);
        let documents = vec![doc1, doc2];

        let query = "fruit";
        let query_embedding = model.encode_single(query);
        let config = create_test_config();

        let results = search_documents(&documents, &query_embedding, &config);

        // Should search across all documents
        let filenames: Vec<&String> = results.iter().map(|r| &r.filename).collect();

        // Both files should have matches
        assert!(!results.is_empty());
        assert!(filenames.contains(&&"file1.txt".to_string()));
        assert!(filenames.contains(&&"file2.txt".to_string()));
    }

    #[test]
    fn test_empty_documents_handling() {
        let model = get_model();
        let documents: Vec<Document> = vec![];
        let query = "test";
        let query_embedding = model.encode_single(query);
        let config = create_test_config();

        let results = search_documents(&documents, &query_embedding, &config);
        assert!(results.is_empty());
    }

    #[test]
    fn test_case_insensitive_search() {
        let model = get_model();

        let doc = create_test_document_with_model(
            "mixed_case.txt",
            vec!["Hello World", "GOODBYE WORLD", "Test Line"],
        );
        let documents = vec![doc];

        let query = "hello world";
        let mut config = create_test_config();
        config.ignore_case = true;

        // For case-insensitive, we need to process both query and content
        let query_lower = query.to_lowercase();
        let query_embedding = model.encode_single(&query_lower);

        let results = search_documents(&documents, &query_embedding, &config);

        // Should find matches despite case differences
        assert!(!results.is_empty());
    }

    #[test]
    fn test_create_document_from_content() {
        let model = get_model();
        let content = "Line 1\nLine 2\nLine 3";

        let doc = create_document_from_content("test.txt".to_string(), content, model, false)
            .expect("Failed to create document");

        assert_eq!(doc.filename, "test.txt");
        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.embeddings.len(), 3);
        assert_eq!(doc.lines[0], "Line 1");
        assert_eq!(doc.lines[1], "Line 2");
        assert_eq!(doc.lines[2], "Line 3");
    }

    #[test]
    fn test_create_document_from_empty_content() {
        let model = get_model();
        let content = "";

        let doc = create_document_from_content("empty.txt".to_string(), content, model, false);

        assert!(doc.is_none());
    }

    #[test]
    fn test_create_document_with_case_insensitive() {
        let model = get_model();
        let content = "Hello World\nGOODBYE world";

        let doc = create_document_from_content(
            "test.txt".to_string(),
            content,
            model,
            true, // ignore_case = true
        )
        .expect("Failed to create document");

        assert_eq!(doc.filename, "test.txt");
        assert_eq!(doc.lines.len(), 2);
        // Original lines should be preserved
        assert_eq!(doc.lines[0], "Hello World");
        assert_eq!(doc.lines[1], "GOODBYE world");
        // But embeddings should be based on lowercase versions
        assert_eq!(doc.embeddings.len(), 2);
    }
}
