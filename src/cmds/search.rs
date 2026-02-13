use anyhow::Result;
use model2vec_rs::model::StaticModel;
use std::io::{self, BufRead, IsTerminal};

#[cfg(feature = "workspace")]
use crate::workspace::{Workspace, store::RankedLine};

#[cfg(feature = "workspace")]
use crate::search::search_with_workspace;

use crate::json_mode::{ErrorOutput, SearchOutput, SearchResultJSON};
use crate::search::{
    Document, MODEL_NAME, SearchConfig, SearchResult, search_documents, search_files,
};

fn read_from_stdin() -> Result<Vec<String>> {
    let stdin = io::stdin();
    let lines: Result<Vec<String>, _> = stdin.lock().lines().collect();
    Ok(lines?)
}

// Convert SearchResult to SearchResultJSON
fn search_result_to_json(result: &SearchResult) -> SearchResultJSON {
    SearchResultJSON {
        filename: result.filename.clone(),
        start_line_number: result.start,
        end_line_number: result.end,
        match_line_number: result.match_line,
        distance: result.distance,
        content: result.lines.join("\n"),
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

pub async fn search_cmd(
    query: String,
    files: Vec<String>,
    n_lines: usize,
    top_k: usize,
    max_distance: Option<f64>,
    ignore_case: bool,
    json: bool,
) -> Result<()> {
    let model = StaticModel::from_pretrained(
        MODEL_NAME, // "minishlab/potion-multilingual-128M",
        None,       // Optional: Hugging Face API token for private models
        None, // Optional: bool to override model's default normalization. `None` uses model's config.
        None, // Optional: subfolder if model files are not at the root of the repo/path
    )?;

    let query = if ignore_case {
        query.to_lowercase()
    } else {
        query.clone()
    };

    let query_embedding = model.encode_single(&query);
    let config = SearchConfig {
        n_lines,
        top_k,
        max_distance,
        ignore_case,
    };

    // Handle stdin input (non-workspace mode)
    if files.is_empty() && !io::stdin().is_terminal() {
        let stdin_lines = read_from_stdin()?;
        if !stdin_lines.is_empty() {
            let lines_for_embedding = if ignore_case {
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

            if json {
                let output = SearchOutput {
                    results: search_results.iter().map(search_result_to_json).collect(),
                };
                let json_output = serde_json::to_string_pretty(&output)?;
                println!("{}", json_output);
            } else {
                print_search_results(&search_results);
            }

            return Ok(());
        }
    }

    if files.is_empty() {
        let error_msg =
            "No input provided. Either specify files as arguments or pipe input to stdin.";
        if json {
            let error_output = ErrorOutput {
                error: error_msg.to_string(),
                error_type: "NoInput".to_string(),
            };
            let json_output = serde_json::to_string_pretty(&error_output)?;
            eprintln!("{}", json_output);
        } else {
            eprintln!("Error: {}", error_msg);
        }
        std::process::exit(1);
    }

    // Handle file input with optional workspace integration
    #[cfg(feature = "workspace")]
    {
        if Workspace::active().is_ok() {
            // Workspace mode: use persisted line embeddings for speed
            let config = SearchConfig {
                n_lines,
                top_k,
                max_distance,
                ignore_case,
            };
            let ranked_lines = search_with_workspace(&files, &query, &model, &config).await?;

            if json {
                // Convert workspace results to SearchResultJSON
                let results: Vec<SearchResultJSON> = ranked_lines
                    .iter()
                    .map(|ranked_line| {
                        let match_line_number = ranked_line.line_number as usize;
                        let start = match_line_number.saturating_sub(n_lines);
                        let end = match_line_number + n_lines + 1;

                        // Read file content for the result
                        let content =
                            if let Ok(file_content) = std::fs::read_to_string(&ranked_line.path) {
                                let lines: Vec<&str> = file_content.lines().collect();
                                let actual_start = start;
                                let actual_end = end.min(lines.len());
                                lines[actual_start..actual_end].join("\n")
                            } else {
                                "[Error: Could not read file content]".to_string()
                            };

                        SearchResultJSON {
                            filename: ranked_line.path.clone(),
                            start_line_number: start,
                            end_line_number: end,
                            match_line_number,
                            distance: ranked_line.distance as f64,
                            content,
                        }
                    })
                    .collect();

                let output = SearchOutput { results };
                let json_output = serde_json::to_string_pretty(&output)?;
                println!("{}", json_output);
            } else {
                print_workspace_search_results(&ranked_lines, n_lines);
            }
        } else {
            let search_results = search_files(&files, &query, &model, &config)?;

            if json {
                let output = SearchOutput {
                    results: search_results.iter().map(search_result_to_json).collect(),
                };
                let json_output = serde_json::to_string_pretty(&output)?;
                println!("{}", json_output);
            } else {
                print_search_results(&search_results);
            }
        }
    }

    #[cfg(not(feature = "workspace"))]
    {
        let search_results = search_files(&files, &query, &model, &config)?;

        if json {
            let output = SearchOutput {
                results: search_results.iter().map(search_result_to_json).collect(),
            };
            let json_output = serde_json::to_string_pretty(&output)?;
            println!("{}", json_output);
        } else {
            print_search_results(&search_results);
        }
    }

    Ok(())
}
