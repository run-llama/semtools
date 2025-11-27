use anyhow::Result;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage, ChatCompletionTools,
};
use async_openai::{Client, types::chat::CreateChatCompletionRequestArgs};
use model2vec_rs::model::StaticModel;
use serde_json::Value;

use crate::ask::system_prompt::{STDIN_SYSTEM_PROMPT, SYSTEM_PROMPT};
use crate::ask::tools::{AgentTool, GrepTool, ReadTool, SearchTool};
use crate::search::SearchConfig;

/// Run an agent loop with the search and read tools
///
/// # Arguments
/// * `files` - List of file paths to search through
/// * `user_message` - The user's query/message
/// * `model` - The embedding model for semantic search
/// * `client` - OpenAI API client
/// * `api_model` - The LLM model to use (e.g., "gpt-4o-mini")
/// * `max_iterations` - Maximum number of agent loop iterations (default: 10)
///
/// # Returns
/// The final response from the agent as a String
pub async fn ask_agent(
    files: Vec<String>,
    user_message: &str,
    model: &StaticModel,
    client: &Client<OpenAIConfig>,
    api_model: &str,
    max_iterations: Option<usize>,
) -> Result<String> {
    let max_iterations = max_iterations.unwrap_or(20);

    // Build the tools
    let tools: Vec<ChatCompletionTools> = vec![
        GrepTool::chat_definition()?,
        SearchTool::chat_definition()?,
        ReadTool::chat_definition()?,
    ];

    // Initialize messages with system prompt and user message
    let mut messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(SYSTEM_PROMPT)
            .build()?
            .into(),
        ChatCompletionRequestUserMessage::from(user_message).into(),
    ];

    // Agent loop
    for _iteration in 0..max_iterations {
        // Create request with current messages
        let request = CreateChatCompletionRequestArgs::default()
            .model(api_model)
            .messages(messages.clone())
            .tools(tools.clone())
            .build()?;

        // Get response from LLM
        let response_message = client
            .chat()
            .create(request)
            .await?
            .choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?
            .message
            .clone();

        // Check if there are tool calls
        if let Some(tool_calls) = response_message.tool_calls.clone() {
            // Process tool calls
            let mut function_responses = Vec::new();

            for tool_call_enum in tool_calls.iter() {
                if let ChatCompletionMessageToolCalls::Function(tool_call) = tool_call_enum {
                    let name = &tool_call.function.name;
                    let args = &tool_call.function.arguments;

                    // Call the appropriate tool
                    let response_content = call_tool(name, args, &files, model).await?;

                    // Print summary of the tool response
                    print_tool_summary(&response_content);

                    function_responses.push((tool_call.clone(), response_content));
                }
            }

            // Add assistant message with tool calls to history
            let assistant_message: ChatCompletionRequestMessage =
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_calls)
                    .build()?
                    .into();
            messages.push(assistant_message);

            // Add tool responses to history
            let tool_messages: Vec<ChatCompletionRequestMessage> = function_responses
                .iter()
                .map(|(tool_call, response_content)| {
                    ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                        content: response_content.to_string().into(),
                        tool_call_id: tool_call.id.clone(),
                    })
                })
                .collect();
            messages.extend(tool_messages);
        } else {
            // No tool calls - we have a final response
            if let Some(content) = response_message.content {
                return Ok(content);
            } else {
                return Err(anyhow::anyhow!("No content in final response"));
            }
        }
    }

    Err(anyhow::anyhow!(
        "Max iterations ({}) reached without final response",
        max_iterations
    ))
}

/// Call a tool by name with the given arguments
async fn call_tool(
    name: &str,
    args: &str,
    files: &[String],
    model: &StaticModel,
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
            if max_distance.is_none() {
                println!("    top_k: {}", top_k);
            } else {
                println!("    max_distance: {:?}", max_distance.unwrap());
            }

            SearchTool::search(files, query, model, config).await
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
fn print_tool_summary(response: &str) {
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

/// Run an agent with stdin content injected directly (no tools available)
///
/// # Arguments
/// * `stdin_content` - The content from stdin to include in the prompt
/// * `user_message` - The user's query/message
/// * `client` - OpenAI API client
/// * `api_model` - The LLM model to use (e.g., "gpt-4o-mini")
///
/// # Returns
/// The response from the agent as a String
pub async fn ask_agent_with_stdin(
    stdin_content: &str,
    user_message: &str,
    client: &Client<OpenAIConfig>,
    api_model: &str,
) -> Result<String> {
    // Construct the user message with stdin content
    let full_message = format!(
        "<stdin_content>\n{}\n</stdin_content>\n\n{}",
        stdin_content, user_message
    );

    // Initialize messages with system prompt and user message (no tools)
    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(STDIN_SYSTEM_PROMPT)
            .build()?
            .into(),
        ChatCompletionRequestUserMessage::from(full_message.as_str()).into(),
    ];

    // Create request without tools
    let request = CreateChatCompletionRequestArgs::default()
        .model(api_model)
        .messages(messages)
        .build()?;

    // Get response from LLM
    let response_message = client
        .chat()
        .create(request)
        .await?
        .choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("No choices in response"))?
        .message
        .clone();

    // Return the content
    if let Some(content) = response_message.content {
        Ok(content)
    } else {
        Err(anyhow::anyhow!("No content in response"))
    }
}
