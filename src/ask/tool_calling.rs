use anyhow::Result;
use model2vec_rs::model::StaticModel;
use serde_json::Value;

use crate::ask::tools::{GrepTool, ReadTool, SearchTool};
use crate::json_mode::AskOutput;
use crate::search::SearchConfig;

/// Call a tool by name with the given arguments
pub async fn call_tool(
    name: &str,
    args: &str,
    files: &[String],
    model: &StaticModel,
    cur_output: &mut AskOutput,
) -> Result<String> {
    let function_args: Value = serde_json::from_str(args)?;

    match name {
        "grep" => {
            let pattern = function_args["pattern"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

            let file_paths: Option<Vec<String>> =
                function_args["file_paths"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });

            // Update files_searched in cur_output
            if let Some(paths) = file_paths.clone() {
                for path in paths {
                    if !cur_output.files_searched.contains(&path) {
                        cur_output.files_searched.push(path);
                    }
                }
            }

            let is_regex = function_args["is_regex"].as_bool().unwrap_or(false);
            let case_sensitive = function_args["case_sensitive"].as_bool().unwrap_or(true);
            let context_lines = function_args["context_lines"].as_u64().unwrap_or(3) as usize;

            // Log the tool call
            println!("\n[Tool Call: grep]");
            println!("  pattern: \"{}\"", pattern);
            println!("  is_regex: {}", is_regex);
            println!("  case_sensitive: {}", case_sensitive);
            println!("  context_lines: {}", context_lines);
            if let Some(ref paths) = file_paths
                && !paths.is_empty()
            {
                println!("  file_paths: {:?}", paths);
            }

            GrepTool::grep(
                files,
                pattern,
                file_paths,
                is_regex,
                case_sensitive,
                context_lines,
            )
            .await
        }
        "search" => {
            let query = function_args["query"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

            let config_json = &function_args["config"];
            let n_lines = config_json["n_lines"].as_u64().unwrap_or(5) as usize;
            let ignore_case = config_json["ignore_case"].as_bool().unwrap_or(false);
            let max_distance = config_json["max_distance"].as_f64();
            let top_k = config_json["top_k"].as_u64().unwrap_or(3) as usize;

            let config = SearchConfig {
                n_lines,
                ignore_case,
                max_distance,
                top_k,
            };

            // Log the tool call with formatted parameters
            println!("\n[Tool Call: search]");
            println!("  query: \"{}\"", query);
            println!("  config:");
            println!("    n_lines: {}", n_lines);
            println!("    ignore_case: {}", ignore_case);

            // Max distance and top_k are mutually exclusive
            if let Some(md) = max_distance {
                println!("    max_distance: {:?}", md);
            } else {
                println!("    top_k: {}", top_k);
            }

            SearchTool::search(files, query, model, config, &mut cur_output.files_searched).await
        }
        "read" => {
            let path = function_args["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
            let start_line = function_args["start_line"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Missing 'start_line' parameter"))?
                as usize;
            let end_line = function_args["end_line"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Missing 'end_line' parameter"))?
                as usize;

            // Log the tool call with formatted parameters
            println!("\n[Tool Call: read]");
            println!("  path: {}", path);
            println!("  start_line: {}", start_line);
            println!("  end_line: {}", end_line);

            ReadTool::read(path, start_line, end_line).await
        }
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}

/// Print a summary of the tool response
pub fn print_tool_summary(response: &str) {
    // Count the number of <chunk> tags
    let chunk_count = response.matches("<chunk").count();

    // Count total lines in all chunks (excluding the chunk tags themselves)
    let total_lines: usize = response
        .split("<chunk")
        .skip(1) // Skip content before first chunk
        .filter_map(|chunk| {
            // Find the content between the opening tag and </chunk>
            chunk
                .split_once(">")
                .and_then(|(_, rest)| rest.split_once("</chunk>"))
                .map(|(content, _)| content.lines().count())
        })
        .sum();

    if chunk_count > 0 {
        println!(
            "  → Returned {} chunk(s) with {} total lines",
            chunk_count, total_lines
        );
    } else if response.contains("No matches found") {
        println!("  → No matches found");
    } else {
        println!("  → Returned {} lines", response.lines().count());
    }
}
