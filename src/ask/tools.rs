use anyhow::Result;
use async_openai::types::chat::{ChatCompletionTool, ChatCompletionTools, FunctionObjectArgs};
use async_openai::types::responses::{FunctionTool, Tool};
use model2vec_rs::model::StaticModel;
use serde_json::json;

use crate::search::{SearchConfig, SearchResult, search_files};

#[cfg(feature = "workspace")]
use crate::workspace::{Workspace, store::RankedLine};

#[cfg(feature = "workspace")]
use crate::search::search_with_workspace;

/// Trait for tools that can work with both Chat Completions and Responses API
pub trait AgentTool {
    /// Get the tool definition for Chat Completions API
    fn chat_definition() -> Result<ChatCompletionTools>;

    /// Get the tool definition for Responses API
    fn responses_definition() -> Result<Tool>;
}

/// Helper function to convert JSON schema to Responses API FunctionTool
fn create_function_tool(name: &str, description: &str, parameters: serde_json::Value) -> Tool {
    Tool::Function(FunctionTool {
        name: name.to_string(),
        description: Some(description.to_string()),
        parameters: Some(parameters),
        strict: None,
    })
}

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

impl AgentTool for SearchTool {
    fn chat_definition() -> Result<ChatCompletionTools> {
        Ok(ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObjectArgs::default()
                .name("search")
                .description("Search through files using semantic keyword search. Returns relevant document chunks with their file paths and line numbers. If top-k is not specified, returns all relevant results within the max distance threshold.")
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
                            "required": [],
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
                .strict(false)
                .build()?,
        }))
    }

    fn responses_definition() -> Result<Tool> {
        let parameters = json!({
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
                    "required": [],
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
        });

        Ok(create_function_tool(
            "search",
            "Search through files using semantic keyword search. Returns relevant document chunks with their file paths and line numbers. If top-k is not specified, returns all relevant results within the max distance threshold.",
            parameters,
        ))
    }
}

impl SearchTool {
    pub async fn search(
        files: &[String],
        query: &str,
        model: &StaticModel,
        config: SearchConfig,
        files_searched: &mut Vec<String>,
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
            let ranked_lines = search_with_workspace(files, &query, model, &config).await?;

            // Track files that were searched (have results)
            for ranked_line in &ranked_lines {
                if !files_searched.contains(&ranked_line.path) {
                    files_searched.push(ranked_line.path.clone());
                }
            }

            // Convert results to SearchResult format and format
            let formatted = format_ranked_lines(&ranked_lines, config.n_lines);
            return Ok(formatted);
        }

        let search_results = search_files(files, &query, model, &config)?;

        // Track files that were searched (have results)
        for result in &search_results {
            if !files_searched.contains(&result.filename) {
                files_searched.push(result.filename.clone());
            }
        }

        let formatted = format_search_results(&search_results);

        Ok(formatted)
    }
}

pub struct ReadTool;

impl AgentTool for ReadTool {
    fn chat_definition() -> Result<ChatCompletionTools> {
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
                .strict(false)
                .build()?,
        }))
    }

    fn responses_definition() -> Result<Tool> {
        let parameters = json!({
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
        });

        Ok(create_function_tool(
            "read",
            "Read a specific range of lines from a file. Returns the content between start_line and end_line.",
            parameters,
        ))
    }
}

impl ReadTool {
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

pub struct GrepTool;

impl AgentTool for GrepTool {
    fn chat_definition() -> Result<ChatCompletionTools> {
        Ok(ChatCompletionTools::Function(ChatCompletionTool {
            function: FunctionObjectArgs::default()
                .name("grep")
                .description("Search for exact patterns or regular expressions in files. Use this when you know the exact string, function name, class name, or regex pattern to search for. Best for exhaustive searches of exact strings/patterns.")
                .parameters(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "The exact string or regular expression pattern to search for"
                        },
                        "file_paths": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "Optional list of specific file paths to search. If empty or not provided, searches all available files.",
                            "default": []
                        },
                        "is_regex": {
                            "type": "boolean",
                            "description": "Whether the pattern is a regular expression",
                            "default": false
                        },
                        "case_sensitive": {
                            "type": "boolean",
                            "description": "Whether the search should be case sensitive",
                            "default": true
                        },
                        "context_lines": {
                            "type": "integer",
                            "description": "Number of lines to show before and after each match for context",
                            "default": 5
                        }
                    },
                    "required": ["pattern"],
                    "additionalProperties": false
                }))
                .strict(false)
                .build()?,
        }))
    }

    fn responses_definition() -> Result<Tool> {
        let parameters = json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The exact string or regular expression pattern to search for"
                },
                "file_paths": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional list of specific file paths to search. If empty or not provided, searches all available files.",
                    "default": []
                },
                "is_regex": {
                    "type": "boolean",
                    "description": "Whether the pattern is a regular expression",
                    "default": false
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Whether the search should be case sensitive",
                    "default": true
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of lines to show before and after each match for context",
                    "default": 5
                }
            },
            "required": ["pattern"],
            "additionalProperties": false
        });

        Ok(create_function_tool(
            "grep",
            "Search for exact patterns or regular expressions in files. Use this when you know the exact string, function name, class name, or regex pattern to search for. Best for exhaustive searches of exact strings/patterns.",
            parameters,
        ))
    }
}

impl GrepTool {
    pub async fn grep(
        all_files: &[String],
        pattern: &str,
        file_paths: Option<Vec<String>>,
        is_regex: bool,
        case_sensitive: bool,
        context_lines: usize,
    ) -> Result<String> {
        use grep::regex::RegexMatcher;
        use grep::searcher::{BinaryDetection, SearcherBuilder};
        use regex;
        use std::collections::HashMap;
        use std::path::Path;

        // Determine which files to search
        let files_to_search = if let Some(paths) = file_paths {
            if paths.is_empty() {
                all_files.to_vec()
            } else {
                paths
            }
        } else {
            all_files.to_vec()
        };

        if files_to_search.is_empty() {
            return Err(anyhow::anyhow!("No files to search"));
        }

        // Build the regex matcher
        let pattern_with_flags = if is_regex {
            if case_sensitive {
                pattern.to_string()
            } else {
                format!("(?i){}", pattern)
            }
        } else {
            // Escape special regex characters for literal search
            let escaped = regex::escape(pattern);
            if case_sensitive {
                escaped
            } else {
                format!("(?i){}", escaped)
            }
        };

        let matcher = RegexMatcher::new(&pattern_with_flags)
            .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

        // Build the searcher with context lines
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build();

        // Store results per file
        let mut all_results: HashMap<String, Vec<GrepMatch>> = HashMap::new();

        // Search each file
        for file_path in &files_to_search {
            let path = Path::new(file_path);

            // Skip if file doesn't exist or isn't a file
            if !path.exists() || !path.is_file() {
                continue;
            }

            let mut matches = Vec::new();
            let mut sink = GrepSink::new(&mut matches);

            // Perform the search
            if let Err(e) = searcher.search_path(&matcher, path, &mut sink) {
                // Skip files that can't be read (binary, permissions, etc.)
                eprintln!("Warning: Could not search {}: {}", file_path, e);
                continue;
            }

            if !matches.is_empty() {
                all_results.insert(file_path.clone(), matches);
            }
        }

        // Format the results
        if all_results.is_empty() {
            return Ok("No matches found.".to_string());
        }

        let mut response = String::new();

        for (file_path, matches) in all_results.iter() {
            // Read the file to get context lines
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let lines: Vec<&str> = content.lines().collect();

            for grep_match in matches {
                // Calculate context range (0-based indexing)
                let match_line_idx = grep_match.line_number - 1; // Convert to 0-based
                let start = match_line_idx.saturating_sub(context_lines);
                let end = (match_line_idx + context_lines + 1).min(lines.len());

                response.push_str(&format!(
                    "<chunk file={} start={} end={}>\n",
                    file_path, start, end
                ));

                for line in &lines[start..end] {
                    response.push_str(&format!("{}\n", line));
                }

                response.push_str("</chunk>\n");
            }
        }

        Ok(response)
    }
}

struct GrepMatch {
    line_number: usize,
}

struct GrepSink<'a> {
    matches: &'a mut Vec<GrepMatch>,
}

impl<'a> GrepSink<'a> {
    fn new(matches: &'a mut Vec<GrepMatch>) -> Self {
        GrepSink { matches }
    }
}

impl<'a> grep_searcher::Sink for GrepSink<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &grep_searcher::SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        self.matches.push(GrepMatch {
            line_number: mat.line_number().unwrap_or(0) as usize,
        });
        Ok(true)
    }
}
