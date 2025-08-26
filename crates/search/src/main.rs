use anyhow::Result;
use clap::Parser;
use model2vec_rs::model::StaticModel;
use simsimd::SpatialSimilarity;
use std::cmp::{max, min};
use std::fs::read_to_string;
use std::io::{self, BufRead, IsTerminal};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Query to search for (positional argument)
    query: String,

    // Files or directories to search (positional arguments, optional if using stdin)
    #[arg(help = "Files or directories to search")]
    files: Vec<String>,

    // How many lines before/after to return as context
    #[arg(short, long, default_value_t = 3)]
    context: usize,

    // The top-k files or texts to return (ignored if threshold is set)
    #[arg(long, default_value_t = 3)]
    top_k: usize,

    // Distance threshold - return all results under this threshold (overrides top-k)
    #[arg(
        short,
        long,
        help = "Return all results with distance below this threshold (0.0+)"
    )]
    threshold: Option<f64>,
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

fn main() -> Result<()> {
    let args = Args::parse();

    let model = StaticModel::from_pretrained(
        "minishlab/potion-multilingual-128M", // "minishlab/potion-multilingual-128M",
        None,                                 // Optional: Hugging Face API token for private models
        None, // Optional: bool to override model's default normalization. `None` uses model's config.
        None, // Optional: subfolder if model files are not at the root of the repo/path
    )?;

    let query_embedding = model.encode_single(&args.query);

    let mut documents = Vec::new();

    // Check if we should read from stdin (no files provided and stdin is available)
    if args.files.is_empty() && !io::stdin().is_terminal() {
        // Read from stdin
        let stdin_lines = read_from_stdin()?;
        if !stdin_lines.is_empty() {
            let embeddings = model.encode_with_args(&stdin_lines, Some(2048), 1024);
            documents.push(Document {
                filename: "<stdin>".to_string(),
                lines: stdin_lines,
                embeddings,
            });
        }
    } else if !args.files.is_empty() {
        // Read from files
        for f in args.files {
            let content = read_to_string(&f)?;
            let lines: Vec<&str> = content.lines().collect();

            if lines.is_empty() {
                continue;
            }

            let owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

            let embeddings = model.encode_with_args(&owned_lines, Some(2048), 1024);
            documents.push(Document {
                filename: f,
                lines: owned_lines,
                embeddings,
            })
        }
    } else {
        eprintln!(
            "Error: No input provided. Either specify files as arguments or pipe input to stdin."
        );
        std::process::exit(1);
    }

    let mut search_results = Vec::new();
    for doc in &documents {
        for (idx, line_embedding) in doc.embeddings.iter().enumerate() {
            let distance = f32::cosine(&query_embedding, line_embedding);
            if let Some(distance) = distance {
                let distance_threshold = args.threshold.unwrap_or(100.0);
                if distance < distance_threshold {
                    let bottom_range = max(0, idx.saturating_sub(args.context));
                    let top_range = min(doc.lines.len(), idx + args.context + 1);
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
    let results_to_show = if args.threshold.is_some() {
        &search_results[..]
    } else {
        &search_results[..search_results.len().min(args.top_k)]
    };

    for search_result in results_to_show {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_document(filename: &str, lines: Vec<&str>) -> Document {
        let owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        // Create dummy embeddings for testing (in real usage, these come from the model)
        let embeddings: Vec<Vec<f32>> = owned_lines
            .iter()
            .enumerate()
            .map(|(i, _)| vec![i as f32; 128]) // Simple pattern for testing
            .collect();

        Document {
            filename: filename.to_string(),
            lines: owned_lines,
            embeddings,
        }
    }

    #[test]
    fn test_document_creation() {
        let doc = create_test_document("test.txt", vec!["line 1", "line 2", "line 3"]);
        assert_eq!(doc.filename, "test.txt");
        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.embeddings.len(), 3);
        assert_eq!(doc.lines[0], "line 1");
    }

    #[test]
    fn test_search_result_context_boundaries() {
        let lines = vec!["line 0", "line 1", "line 2", "line 3", "line 4"];
        let doc = create_test_document("test.txt", lines);

        // Test context calculation for middle line
        let context: usize = 2;
        let idx: usize = 2; // "line 2"
        let bottom_range = max(0, idx.saturating_sub(context));
        let top_range = min(doc.lines.len(), idx + context + 1);

        assert_eq!(bottom_range, 0); // max(0, 2-2) = 0
        assert_eq!(top_range, 5); // min(5, 2+2+1) = 5

        let context_lines = &doc.lines[bottom_range..top_range];
        assert_eq!(context_lines.len(), 5);
        assert_eq!(context_lines[0], "line 0");
        assert_eq!(context_lines[4], "line 4");
    }

    #[test]
    fn test_search_result_context_at_boundaries() {
        let lines = vec!["line 0", "line 1", "line 2"];
        let doc = create_test_document("test.txt", lines);

        // Test context at start of file
        let context: usize = 2;
        let idx: usize = 0;
        let bottom_range = max(0, idx.saturating_sub(context));
        let top_range = min(doc.lines.len(), idx + context + 1);

        assert_eq!(bottom_range, 0);
        assert_eq!(top_range, 3);

        // Test context at end of file
        let idx: usize = 2;
        let bottom_range = max(0, idx.saturating_sub(context));
        let top_range = min(doc.lines.len(), idx + context + 1);

        assert_eq!(bottom_range, 0);
        assert_eq!(top_range, 3);
    }

    #[test]
    fn test_empty_document_handling() {
        let doc = create_test_document("empty.txt", vec![]);
        assert_eq!(doc.lines.len(), 0);
        assert_eq!(doc.embeddings.len(), 0);
    }

    #[test]
    fn test_search_result_struct() {
        let lines = vec!["test line 1", "test line 2", "test line 3"];
        let doc = create_test_document("test.txt", lines);

        let search_result = SearchResult {
            filename: &doc.filename,
            lines: &doc.lines[1..3], // lines 1-2
            start: 1,
            end: 3,
            match_line: 2, // The actual matching line
            distance: 0.5,
        };

        assert_eq!(search_result.filename, "test.txt");
        assert_eq!(search_result.lines.len(), 2);
        assert_eq!(search_result.lines[0], "test line 2");
        assert_eq!(search_result.start, 1);
        assert_eq!(search_result.end, 3);
        assert_eq!(search_result.match_line, 2);
        assert_eq!(search_result.distance, 0.5);
    }
}
