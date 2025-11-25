use anyhow::Result;
use async_openai::types::chat::{ChatCompletionTool, ChatCompletionTools, FunctionObjectArgs};
use model2vec_rs::model::StaticModel;
use serde_json::json;

use crate::search::{SearchConfig, SearchResult, search_files};

#[cfg(feature = "workspace")]
use crate::workspace::{Workspace, store::RankedLine};

#[cfg(feature = "workspace")]
use crate::search::search_with_workspace;

fn format_search_results(results: &[SearchResult]) -> String {
    let mut response = String::new();

    for search_result in results {
        let filename = search_result.filename.to_string();
        let distance = search_result.distance;
        let start = search_result.start;
        let end = search_result.end;

        response.push_str(&format!(
            "<chunk file={filename} start={start} end={end} distance={distance}>\n"
        ));

        for line in search_result.lines.iter() {
            response.push_str(&format!("{line}\n"));
        }

        response.push_str("</chunk>\n");
    }

    response
}

#[cfg(feature = "workspace")]
fn format_ranked_lines(ranked_lines: &[RankedLine], n_lines: usize) -> String {
    let mut response = String::new();

    for ranked_line in ranked_lines {
        let filename = &ranked_line.path;
        let distance = ranked_line.distance;
        // ranked_line.line_number is 0-based from database
        let match_line_number = ranked_line.line_number as usize;

        // Calculate context range (working with 0-based indices)
        let start = match_line_number.saturating_sub(n_lines);
        let end = match_line_number + n_lines + 1;

        response.push_str(&format!(
            "<chunk file={filename} start={start} end={end} distance={distance}>\n"
        ));

        // For workspace results, we need to read the file to get context lines
        // This is acceptable since we're only doing this for the final results
        if let Ok(content) = std::fs::read_to_string(filename) {
            let lines: Vec<&str> = content.lines().collect();
            let actual_start = start;
            let actual_end = end.min(lines.len());

            for line in lines[actual_start..actual_end].iter() {
                response.push_str(&format!("{line}\n"));
            }
        } else {
            // Fallback: indicate that the file couldn't be read
            response.push_str("[Error: Could not read file content]");
        }

        response.push_str("</chunk>\n");
    }

    response
}

pub struct SearchTool;

// Example
// {'$defs': {'Config': {'properties': {'some_arg': {'default': 1, 'description': 'some arg description', 'title': 'Some Arg', 'type': 'integer'}}, 'title': 'Config', 'type': 'object'}}, 'properties': {'query': {'title': 'Query', 'type': 'string'}, 'config': {'$ref': '#/$defs/Config'}}, 'required': ['query', 'config'], 'type': 'object'}

impl SearchTool {
    pub fn definition() -> Result<ChatCompletionTools> {
        Ok(ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObjectArgs::default()
                .name("search")
                .description("Search through files using semantic keyword search. Returns relevant code chunks with their file paths and line numbers.")
                .parameters(json!({
                    "$defs": {
                        "Config": {
                            "type": "object",
                            "properties": {
                                "n_lines": {
                                    "type": "integer",
                                    "description": "Number of context lines to include before and after each match",
                                    "default": 5
                                },
                                "ignore_case": {
                                    "type": "boolean",
                                    "description": "Whether to ignore case when searching",
                                    "default": false
                                },
                                "max_distance": {
                                    "type": "number",
                                    "description": "Maximum semantic distance for matches (lower is more similar)",
                                    "default": 0.5
                                },
                                "top_k": {
                                    "type": "integer",
                                    "description": "Number of top results to return",
                                    "default": 3
                                }
                            },
                            "required": ["n_lines", "ignore_case", "max_distance", "top_k"],
                            "title": "Config",
                            "additionalProperties": false
                        }
                    },
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query. Works best as a comma separated list of keywrods."
                        },
                        "config": {
                            "$ref": "#/$defs/Config",
                        }
                    },
                    "required": ["query", "config"],
                    "additionalProperties": false
                }))
                .strict(true)
                .build()?,
        }))
    }

    pub async fn search(
        files: &[String],
        query: &str,
        model: &StaticModel,
        config: SearchConfig,
    ) -> Result<String> {
        let query = if config.ignore_case {
            query.to_lowercase()
        } else {
            query.to_string()
        };

        if files.is_empty() {
            return Err(anyhow::anyhow!(
                "Error: No input provided. Either specify files as arguments or pipe input to stdin."
            ));
        }

        // Handle file input with optional workspace integration
        #[cfg(feature = "workspace")]
        if Workspace::active().is_ok() {
            // Workspace mode: use persisted line embeddings for speed
            let ranked_lines = search_with_workspace(&files, &query, &model, &config).await?;

            // Step 5: Convert results to SearchResult format and print
            let formatted = format_ranked_lines(&ranked_lines, config.n_lines);
            return Ok(formatted);
        }

        let search_results = search_files(&files, &query, &model, &config)?;
        let formatted = format_search_results(&search_results);

        Ok(formatted)
    }
}

pub struct ReadTool;

impl ReadTool {
    pub fn definition() -> Result<ChatCompletionTools> {
        Ok(ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObjectArgs::default()
                .name("read")
                .description("Read a specific range of lines from a file. Returns the content between start_line and end_line.")
                .parameters(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The file path to read from"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "The starting line number (0-based)"
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "The ending line number (exclusive, 0-based)"
                        }
                    },
                    "required": ["path", "start_line", "end_line"],
                    "additionalProperties": false
                }))
                .strict(true)
                .build()?,
        }))
    }

    pub async fn read(path: &str, start_line: usize, end_line: usize) -> Result<String> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();
        let actual_end = end_line.min(lines.len());
        let selected_lines = &lines[start_line..actual_end];

        // Build the response with the `<chunk>` tags
        let mut response = String::new();
        response.push_str(&format!(
            "<chunk file={} start={} end={}>\n",
            path, start_line, actual_end
        ));
        response.push_str(&selected_lines.join("\n"));
        response.push_str("</chunk>\n");

        Ok(response)
    }
}
