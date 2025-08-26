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

    // The top-k files or texts to return
    #[arg(long, default_value_t = 3)]
    top_k: usize,
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
        eprintln!("Error: No input provided. Either specify files as arguments or pipe input to stdin.");
        std::process::exit(1);
    }

    let mut search_results = Vec::new();
    for doc in &documents {
        for (idx, line_embedding) in doc.embeddings.iter().enumerate() {
            let distance = f32::cosine(&query_embedding, line_embedding);
            if let Some(distance) = distance {
                if distance < 0.5 {
                    let bottom_range = max(0, idx.saturating_sub(args.context));
                    let top_range = min(doc.lines.len(), idx + args.context + 1);
                    search_results.push(SearchResult {
                        filename: &doc.filename,
                        lines: &doc.lines[bottom_range..top_range],
                        distance,
                        start: bottom_range,
                        end: top_range,
                    })
                }
            }
        }
    }

    // Sort by distance (best matches first)
    search_results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));

    // Limit to top_k results
    for search_result in search_results.iter().take(args.top_k) {
        let filename = search_result.filename.to_string();
        let lines_str = search_result.lines.join("\n");
        let distance = search_result.distance;
        let start = search_result.start;
        let end = search_result.end;

        let message = format!("{filename}:{start}::{end} ({distance})\n{lines_str}\n\n");
        println!("{}", message);
    }

    Ok(())
}
