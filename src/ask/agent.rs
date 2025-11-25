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

use crate::ask::tools::{ReadTool, SearchTool};
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
    let max_iterations = max_iterations.unwrap_or(10);

    // Build the tools
    let tools: Vec<ChatCompletionTools> = vec![SearchTool::definition()?, ReadTool::definition()?];

    // System prompt to encourage citing sources
    let system_prompt = "You are a helpful search assistant with access to search and read tools for exploring corpus' of documents.

CITATION REQUIREMENTS:
1. Use numbered citations [1], [2], [3] etc. throughout your response for ALL factual claims
2. At the end of your response, include a '## References' section listing each citation
3. Place citations immediately after the specific claim they support, not bundled together
4. Each distinct source or set of sources gets its own reference number
5. The chunks returned by search and read tools include file paths and line numbers - use these for your citations

REFERENCE FORMAT RULES:
- Single location: [1] file_path:line_number
- Consecutive lines: [2] file_path:start_line-end_line
- Disjoint sections in same file: [3] file_path:line1,line2,line3
- Multiple files: Use separate reference numbers

EXAMPLE FORMAT:
Graph Convolutional Networks are powerful for node classification [1]. The architecture is described in detail across several sections [2]. GraphSAGE extends this to inductive settings [3], with additional applications discussed [4].

## References
[1] papers/gcn_paper.txt:145
[2] papers/gcn_paper.txt:145-167
[3] papers/graphsage.txt:67
[4] papers/graphsage.txt:67,234,891

Remember: Every factual claim needs a citation with a specific file path and line number.";

    // Initialize messages with system prompt and user message
    let mut messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(system_prompt)
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
            if Some(max_distance).is_none() {
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
