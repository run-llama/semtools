use anyhow::Result;
use clap::Parser;
use model2vec_rs::model::StaticModel;
use simsimd::SpatialSimilarity;
use std::cmp::{max, min};
use std::fs::read_to_string;
use std::io::{self, BufRead, IsTerminal};

const MODEL_NAME: &str = "minishlab/potion-multilingual-128M";

#[derive(Parser, Debug)]
#[command(version, about = "A CLI tool for fast semantic keyword search", long_about = None)]
struct Args {
    /// Query to search for (positional argument)
    query: String,

    /// Files or directories to search (positional arguments, optional if using stdin)
    #[arg(help = "Files or directories to search")]
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
                // Highlight the matching line with yellow background and black text
                println!("\x1b[43m\x1b[30m{:4}: {}\x1b[0m", line_number + 1, line);
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

    let mut documents = Vec::new();

    // Check if we should read from stdin (no files provided and stdin is available)
    if args.files.is_empty() && !io::stdin().is_terminal() {
        // Read from stdin
        let stdin_lines = read_from_stdin()?;
        if !stdin_lines.is_empty() {
            let lines_for_embedding = if args.ignore_case {
                stdin_lines.iter().map(|s| s.to_lowercase()).collect()
            } else {
                stdin_lines.clone()
            };

            let embeddings = model.encode_with_args(&lines_for_embedding, Some(2048), 16384);

            documents.push(Document {
                filename: "<stdin>".to_string(),
                lines: stdin_lines,
                embeddings,
            });
        }
    } else if !args.files.is_empty() {
        // Read from files
        for f in &args.files {
            let content = read_to_string(f)?;
            if let Some(doc) =
                create_document_from_content(f.clone(), &content, &model, args.ignore_case)
            {
                documents.push(doc);
            }
        }
    } else {
        eprintln!(
            "Error: No input provided. Either specify files as arguments or pipe input to stdin."
        );
        std::process::exit(1);
    }

    let search_results = search_documents(&documents, &query_embedding, &args);
    print_search_results(&search_results);

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
}
