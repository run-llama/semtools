use anyhow::Result;
use clap::Parser;
use model2vec_rs::model::StaticModel;
use std::io::{self, BufRead, IsTerminal};

#[cfg(feature = "workspace")]
use semtools::workspace::{
    Workspace,
    store::{RankedLine},
};

#[cfg(feature = "workspace")]
use semtools::search::{search_with_workspace};

use semtools::search::{Document, SearchResult, SearchConfig, search_files, search_documents};


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

fn read_from_stdin() -> Result<Vec<String>> {
    let stdin = io::stdin();
    let lines: Result<Vec<String>, _> = stdin.lock().lines().collect();
    Ok(lines?)
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

#[cfg(feature = "workspace")]
fn print_workspace_search_results(ranked_lines: &[RankedLine], n_lines: usize) {
    let is_tty = io::stdout().is_terminal();

    for ranked_line in ranked_lines {
        let filename = &ranked_line.path;
        let distance = ranked_line.distance;
        // ranked_line.line_number is 0-based from database
        let match_line_number = ranked_line.line_number as usize;

        // Calculate context range (working with 0-based indices)
        let start = match_line_number.saturating_sub(n_lines);
        let end = match_line_number + n_lines + 1;

        println!("{filename}:{start}::{end} ({distance})");

        // For workspace results, we need to read the file to get context lines
        // This is acceptable since we're only doing this for the final results
        if let Ok(content) = std::fs::read_to_string(filename) {
            let lines: Vec<&str> = content.lines().collect();
            let actual_start = start;
            let actual_end = end.min(lines.len());

            for (i, line) in lines[actual_start..actual_end].iter().enumerate() {
                let line_number = actual_start + i;

                if line_number == match_line_number {
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
        } else {
            // Fallback: indicate that the file couldn't be read
            println!("    [Error: Could not read file content]");
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
    let config = SearchConfig {
        n_lines: args.n_lines,
        top_k: args.top_k,
        max_distance: args.max_distance,
        ignore_case: args.ignore_case,
    };

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
            
            let search_results = search_documents(&documents, &query_embedding, &config);
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
        // Workspace mode: use persisted line embeddings for speed
        let config = SearchConfig {
            n_lines: args.n_lines,
            top_k: args.top_k,
            max_distance: args.max_distance,
            ignore_case: args.ignore_case,
        };
        let ranked_lines = search_with_workspace(
            &args.files,
            &query,
            &model,
            &config,
        )?;

        // Step 5: Convert results to SearchResult format and print
        print_workspace_search_results(&ranked_lines, args.n_lines);
    } else {
        let search_results = search_files(&args.files, &query, &model, &config)?;
        print_search_results(&search_results);
    }

    Ok(())
}
