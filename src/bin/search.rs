use anyhow::Result;
use clap::Parser;
use model2vec_rs::model::StaticModel;
use simsimd::SpatialSimilarity;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::fs::read_to_string;
use std::io::{self, BufRead, IsTerminal};

#[cfg(feature = "workspace")]
use semtools::workspace::{
    Workspace,
    store::{DocMeta, Store},
};

const MODEL_NAME: &str = "minishlab/potion-multilingual-128M";

#[derive(Parser, Debug)]
#[command(version, about = "A CLI tool for fast semantic keyword search", long_about = None)]
struct Args {
    /// Query to search for (positional argument)
    query: String,

    /// Files to search (positional arguments, optional if using stdin)
    #[arg(help = "Files to search, optional if using stdin")]
    files: Vec<String>,

    /// How many lines before/after to return as context
    #[arg(short = 'n', long = "n-lines", alias = "context", default_value_t = 3)]
    n_lines: usize,

    /// The top-k files or texts to return (ignored if max_distance is set)
    #[arg(long, default_value_t = 3)]
    top_k: usize,

    /// Return all results with distance below this threshold (0.0+)
    #[arg(short = 'm', long = "max-distance", alias = "threshold")]
    max_distance: Option<f64>,

    /// Perform case-insensitive search (default is false)
    #[arg(short, long, default_value_t = false)]
    ignore_case: bool,
}

pub struct Document {
    filename: String,
    lines: Vec<String>,
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug)]
pub struct DocumentInfo {
    filename: String,
    content: String,
    meta: DocMeta,
}

#[derive(Debug)]
pub enum DocumentState {
    Unchanged(String),     // Just the filename, no need to process
    Changed(DocumentInfo), // Full document info for processing
    New(DocumentInfo),     // Full document info for processing
}

pub struct SearchResult<'a> {
    filename: &'a String,
    lines: &'a [String],
    start: usize,
    end: usize,
    match_line: usize, // The actual line number that matched
    distance: f64,
}

fn read_from_stdin() -> Result<Vec<String>> {
    let stdin = io::stdin();
    let lines: Result<Vec<String>, _> = stdin.lock().lines().collect();
    Ok(lines?)
}

#[cfg(feature = "workspace")]
async fn analyze_document_states(
    file_paths: &[String],
    store: &Store,
) -> Result<Vec<DocumentState>> {
    // Get existing document metadata from workspace
    let existing_docs = store.get_existing_docs(file_paths).await?;

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

// Extracted function to create document from file content
fn create_document_from_content(
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

// Extracted function to perform search on documents
fn search_documents<'a>(
    documents: &'a [Document],
    query_embedding: &[f32],
    args: &Args,
) -> Vec<SearchResult<'a>> {
    let mut search_results = Vec::new();

    for doc in documents {
        for (idx, line_embedding) in doc.embeddings.iter().enumerate() {
            let distance = f32::cosine(query_embedding, line_embedding);
            if let Some(distance) = distance {
                let distance_threshold = args.max_distance.unwrap_or(100.0);
                if distance < distance_threshold {
                    let bottom_range = max(0, idx.saturating_sub(args.n_lines));
                    let top_range = min(doc.lines.len(), idx + args.n_lines + 1);
                    search_results.push(SearchResult {
                        filename: &doc.filename,
                        lines: &doc.lines[bottom_range..top_range],
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
    if args.max_distance.is_some() {
        search_results
    } else {
        search_results.into_iter().take(args.top_k).collect()
    }
}

// Extracted function to format and print results
fn print_search_results(results: &[SearchResult]) {
    let is_tty = io::stdout().is_terminal();
    for search_result in results {
        let filename = search_result.filename.to_string();
        let distance = search_result.distance;
        let start = search_result.start;
        let end = search_result.end;

        println!("{filename}:{start}::{end} ({distance})");

        // Print each line, highlighting the actual match
        for (i, line) in search_result.lines.iter().enumerate() {
            let line_number = start + i;

            if line_number == search_result.match_line {
                if is_tty {
                    // Highlight the matching line with yellow background and black text
                    println!("\x1b[43m\x1b[30m{:4}: {}\x1b[0m", line_number + 1, line);
                } else {
                    println!("{:4}: {}", line_number + 1, line);
                }
            } else {
                // Regular context line
                println!("{:4}: {}", line_number + 1, line);
            }
        }
        println!(); // Empty line between results
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let model = StaticModel::from_pretrained(
        MODEL_NAME, // "minishlab/potion-multilingual-128M",
        None,       // Optional: Hugging Face API token for private models
        None, // Optional: bool to override model's default normalization. `None` uses model's config.
        None, // Optional: subfolder if model files are not at the root of the repo/path
    )?;

    let query = if args.ignore_case {
        args.query.to_lowercase()
    } else {
        args.query.clone()
    };
    let query_embedding = model.encode_single(&query);

    // Handle stdin input (non-workspace mode)
    if args.files.is_empty() && !io::stdin().is_terminal() {
        let stdin_lines = read_from_stdin()?;
        if !stdin_lines.is_empty() {
            let lines_for_embedding = if args.ignore_case {
                stdin_lines.iter().map(|s| s.to_lowercase()).collect()
            } else {
                stdin_lines.clone()
            };

            let embeddings = model.encode_with_args(&lines_for_embedding, Some(2048), 16384);

            let documents = vec![Document {
                filename: "<stdin>".to_string(),
                lines: stdin_lines,
                embeddings,
            }];

            let search_results = search_documents(&documents, &query_embedding, &args);
            print_search_results(&search_results);
            return Ok(());
        }
    }

    if args.files.is_empty() {
        eprintln!(
            "Error: No input provided. Either specify files as arguments or pipe input to stdin."
        );
        std::process::exit(1);
    }

    // Handle file input with optional workspace integration
    #[cfg(feature = "workspace")]
    if Workspace::active().is_ok() {
        // Workspace mode: implement two-stage retrieval
        let ws = Workspace::open()?;
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let store = rt.block_on(Store::open(&ws.config.root_dir))?;

        // Step 1: Analyze document states (changed/new/unchanged)
        let doc_states = rt.block_on(analyze_document_states(&args.files, &store))?;

        // Step 2: Process documents that need embedding updates
        let mut docs_to_upsert = Vec::new();
        let mut doc_embeddings_to_upsert = Vec::new();

        for state in &doc_states {
            match state {
                DocumentState::Changed(doc_info) | DocumentState::New(doc_info) => {
                    // Generate document-level embedding (average of line embeddings)
                    if let Some(doc) = create_document_from_content(
                        doc_info.filename.clone(),
                        &doc_info.content,
                        &model,
                        args.ignore_case,
                    ) {
                        if !doc.embeddings.is_empty() {
                            let dim = doc.embeddings[0].len();
                            let mut sum = vec![0.0f32; dim];
                            for e in &doc.embeddings {
                                for (i, v) in e.iter().enumerate() {
                                    sum[i] += *v;
                                }
                            }
                            let count = doc.embeddings.len() as f32;
                            for v in &mut sum {
                                *v /= count;
                            }

                            docs_to_upsert.push(doc_info.meta.clone());
                            doc_embeddings_to_upsert.push(sum);
                        }
                    }
                }
                DocumentState::Unchanged(_) => {
                    // Skip - already in workspace and unchanged
                }
            }
        }

        // Step 3: Update workspace with new/changed documents
        if !docs_to_upsert.is_empty() {
            rt.block_on(store.upsert_documents(&docs_to_upsert, &doc_embeddings_to_upsert))?;
        }

        // Step 4: Two-stage retrieval if we have many documents
        let working_set_paths = if args.files.len() > ws.config.doc_top_k {
            // Stage 1: ANN filter to get top documents from workspace
            let candidates = rt
                .block_on(store.ann_filter_top_k(
                    &query_embedding,
                    &args.files,
                    ws.config.doc_top_k,
                    ws.config.in_batch_size,
                ))
                .unwrap_or_default();
            candidates.into_iter().map(|r| r.path).collect()
        } else {
            // Small dataset - use all files
            args.files.clone()
        };

        // Step 5: Generate line-by-line embeddings only for working set
        // Reuse already-read content for Changed/New docs to avoid double reads
        let mut content_cache: HashMap<&str, &str> = HashMap::new();
        let mut owned_content_cache: HashMap<String, String> = HashMap::new();

        for state in &doc_states {
            match state {
                DocumentState::Changed(info) | DocumentState::New(info) => {
                    owned_content_cache.insert(info.filename.clone(), info.content.clone());
                }
                DocumentState::Unchanged(_) => {}
            }
        }

        // Convert to &str cache for quick lookup
        for (k, v) in &owned_content_cache {
            content_cache.insert(k.as_str(), v.as_str());
        }

        let mut documents = Vec::new();
        for file_path in &working_set_paths {
            if let Some(content) = content_cache.get(file_path.as_str()) {
                if let Some(doc) = create_document_from_content(
                    file_path.clone(),
                    content,
                    &model,
                    args.ignore_case,
                ) {
                    documents.push(doc);
                }
            } else {
                let content = read_to_string(file_path)?;
                if let Some(doc) = create_document_from_content(
                    file_path.clone(),
                    &content,
                    &model,
                    args.ignore_case,
                ) {
                    documents.push(doc);
                }
            }
        }

        // Step 6: Perform line-by-line search on working set
        let search_results = search_documents(&documents, &query_embedding, &args);
        print_search_results(&search_results);
    } else {
        // Non-workspace mode: traditional search
        let mut documents = Vec::new();
        for f in &args.files {
            let content = read_to_string(f)?;
            if let Some(doc) =
                create_document_from_content(f.clone(), &content, &model, args.ignore_case)
            {
                documents.push(doc);
            }
        }

        let search_results = search_documents(&documents, &query_embedding, &args);
        print_search_results(&search_results);
    }

    #[cfg(not(feature = "workspace"))]
    {
        // Non-workspace mode: traditional search
        let mut documents = Vec::new();
        for f in &args.files {
            let content = read_to_string(f)?;
            if let Some(doc) =
                create_document_from_content(f.clone(), &content, &model, args.ignore_case)
            {
                documents.push(doc);
            }
        }

        let search_results = search_documents(&documents, &query_embedding, &args);
        print_search_results(&search_results);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

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

    fn create_test_args(query: &str) -> Args {
        Args {
            query: query.to_string(),
            files: vec![],
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

        let args = create_test_args("test query");
        let query_embedding = model.encode_single(&args.query);

        let results = search_documents(&documents, &query_embedding, &args);

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

        let mut args = create_test_args("test");
        args.max_distance = Some(0.5); // Very restrictive threshold

        let query_embedding = model.encode_single(&args.query);
        let results = search_documents(&documents, &query_embedding, &args);

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

        let mut args = create_test_args("test");
        args.top_k = 2; // Limit to 2 results
        args.max_distance = None; // Use top_k instead of threshold

        let query_embedding = model.encode_single(&args.query);
        let results = search_documents(&documents, &query_embedding, &args);

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

        let mut args = create_test_args("test");
        args.n_lines = 1; // 1 line of context before/after

        let query_embedding = model.encode_single(&args.query);
        let results = search_documents(&documents, &query_embedding, &args);

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

        let mut args = create_test_args("first"); // Query that should match the first line
        args.n_lines = 5; // More context than available

        let query_embedding = model.encode_single(&args.query);
        let results = search_documents(&documents, &query_embedding, &args);

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

        let args = create_test_args("fruit");
        let query_embedding = model.encode_single(&args.query);

        let results = search_documents(&documents, &query_embedding, &args);

        // Should search across all documents
        let filenames: Vec<&String> = results.iter().map(|r| r.filename).collect();

        // Both files should have matches
        assert!(!results.is_empty());
        assert!(filenames.contains(&&"file1.txt".to_string()));
        assert!(filenames.contains(&&"file2.txt".to_string()));
    }

    #[test]
    fn test_empty_documents_handling() {
        let model = get_model();
        let documents: Vec<Document> = vec![];
        let args = create_test_args("test");
        let query_embedding = model.encode_single(&args.query);

        let results = search_documents(&documents, &query_embedding, &args);
        assert!(results.is_empty());
    }

    #[test]
    fn test_args_parsing_functionality() {
        // Test that our Args struct has the expected defaults
        let args = create_test_args("test query");
        assert_eq!(args.query, "test query");
        assert_eq!(args.n_lines, 3);
        assert_eq!(args.top_k, 3);
        assert_eq!(args.max_distance, None);
        assert!(!args.ignore_case);
    }

    #[test]
    fn test_case_insensitive_search() {
        let model = get_model();

        let doc = create_test_document_with_model(
            "mixed_case.txt",
            vec!["Hello World", "GOODBYE WORLD", "Test Line"],
        );
        let documents = vec![doc];

        let mut args = create_test_args("hello world");
        args.ignore_case = true;

        // For case-insensitive, we need to process both query and content
        let query = args.query.to_lowercase();
        let query_embedding = model.encode_single(&query);

        let results = search_documents(&documents, &query_embedding, &args);

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

    #[cfg(feature = "workspace")]
    mod workspace_tests {
        use super::*;
        use semtools::workspace::store::{DocMeta, Store};
        use std::fs;
        use std::time::UNIX_EPOCH;
        use tempfile::TempDir;

        // Helper to create test files
        fn create_test_files(temp_dir: &TempDir) -> Vec<String> {
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

        #[tokio::test]
        async fn test_analyze_document_states_all_new() {
            let temp_dir = TempDir::new().unwrap();
            let file_paths = create_test_files(&temp_dir);

            // Create empty store
            let store = Store::open(temp_dir.path().to_str().unwrap())
                .await
                .unwrap();

            let states = analyze_document_states(&file_paths, &store).await.unwrap();

            assert_eq!(states.len(), 3);

            // All should be new documents
            for state in &states {
                match state {
                    DocumentState::New(doc_info) => {
                        assert!(file_paths.contains(&doc_info.filename));
                        assert!(!doc_info.content.is_empty());
                        assert!(doc_info.meta.size_bytes > 0);
                        assert!(doc_info.meta.mtime > 0);
                    }
                    _ => panic!("Expected New document state"),
                }
            }
        }

        #[tokio::test]
        async fn test_analyze_document_states_unchanged() {
            let temp_dir = TempDir::new().unwrap();
            let file_paths = create_test_files(&temp_dir);

            // Create store and add documents
            let store = Store::open(temp_dir.path().to_str().unwrap())
                .await
                .unwrap();

            // Insert documents with current metadata
            let mut docs = Vec::new();
            let mut embeddings = Vec::new();
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
                };
                docs.push(doc_meta);
                embeddings.push(vec![1.0, 2.0, 3.0, 4.0]); // Dummy embedding
            }
            store.upsert_documents(&docs, &embeddings).await.unwrap();

            // Analyze states - should all be unchanged
            let states = analyze_document_states(&file_paths, &store).await.unwrap();

            assert_eq!(states.len(), 3);

            for state in &states {
                match state {
                    DocumentState::Unchanged(filename) => {
                        assert!(file_paths.contains(filename));
                    }
                    _ => panic!("Expected Unchanged document state"),
                }
            }
        }

        #[tokio::test]
        async fn test_analyze_document_states_changed() {
            let temp_dir = TempDir::new().unwrap();
            let file_paths = create_test_files(&temp_dir);

            // Create store and add documents with old metadata
            let store = Store::open(temp_dir.path().to_str().unwrap())
                .await
                .unwrap();

            let mut docs = Vec::new();
            let mut embeddings = Vec::new();
            for path in &file_paths {
                let doc_meta = DocMeta {
                    path: path.clone(),
                    size_bytes: 10, // Different from actual size
                    mtime: 1000,    // Old timestamp
                };
                docs.push(doc_meta);
                embeddings.push(vec![1.0, 2.0, 3.0, 4.0]); // Dummy embedding
            }
            store.upsert_documents(&docs, &embeddings).await.unwrap();

            // Analyze states - should all be changed
            let states = analyze_document_states(&file_paths, &store).await.unwrap();

            assert_eq!(states.len(), 3);

            for state in &states {
                match state {
                    DocumentState::Changed(doc_info) => {
                        assert!(file_paths.contains(&doc_info.filename));
                        assert!(!doc_info.content.is_empty());
                    }
                    _ => panic!("Expected Changed document state"),
                }
            }
        }

        #[tokio::test]
        async fn test_analyze_document_states_mixed() {
            let temp_dir = TempDir::new().unwrap();
            let file_paths = create_test_files(&temp_dir);

            // Create store and add only the first document
            let store = Store::open(temp_dir.path().to_str().unwrap())
                .await
                .unwrap();

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
            };
            let embedding = vec![vec![1.0, 2.0, 3.0, 4.0]];
            store
                .upsert_documents(&[doc_meta], &embedding)
                .await
                .unwrap();

            // Analyze states
            let states = analyze_document_states(&file_paths, &store).await.unwrap();

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
        }

        #[tokio::test]
        async fn test_analyze_document_states_nonexistent_file() {
            let temp_dir = TempDir::new().unwrap();
            let mut file_paths = create_test_files(&temp_dir);

            // Add a nonexistent file to the list
            file_paths.push("/nonexistent/file.txt".to_string());

            let store = Store::open(temp_dir.path().to_str().unwrap())
                .await
                .unwrap();

            let states = analyze_document_states(&file_paths, &store).await.unwrap();

            // Should only have states for existing files
            assert_eq!(states.len(), 3);

            for state in &states {
                match state {
                    DocumentState::New(doc_info) => {
                        assert_ne!(doc_info.filename, "/nonexistent/file.txt");
                    }
                    _ => {}
                }
            }
        }
    }
}
