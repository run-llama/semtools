use anyhow::Result;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::responses::{
    CreateResponseArgs, FunctionCallOutput, FunctionCallOutputItemParam, FunctionToolCall,
    InputItem, InputParam, Item, MessageItem, OutputItem, Role, Tool,
};
use model2vec_rs::model::StaticModel;
use serde_json::Value;

use crate::ask::system_prompt::SYSTEM_PROMPT;
use crate::ask::tools::{AgentTool, GrepTool, ReadTool, SearchTool};
use crate::search::SearchConfig;

/// Run an agent loop with the search and read tools using the Responses API
///
/// # Arguments
/// * `files` - List of file paths to search through
/// * `user_message` - The user's query/message
/// * `model` - The embedding model for semantic search
/// * `client` - OpenAI API client
/// * `api_model` - The LLM model to use (e.g., "gpt-4.1")
/// * `max_iterations` - Maximum number of agent loop iterations (default: 20)
///
/// # Returns
/// The final response from the agent as a String
pub async fn ask_agent_responses(
    files: Vec<String>,
    user_message: &str,
    model: &StaticModel,
    client: &Client<OpenAIConfig>,
    api_model: &str,
    max_iterations: Option<usize>,
) -> Result<String> {
    let max_iterations = max_iterations.unwrap_or(20);

    // Build the tools using the responses API format
    let tools: Vec<Tool> = vec![
        GrepTool::responses_definition()?,
        SearchTool::responses_definition()?,
        ReadTool::responses_definition()?,
    ];

    // Initialize input items with user message
    // Note: For Responses API, we use the instructions parameter for the system prompt
    let mut input_items: Vec<InputItem> = vec![InputItem::text_message(Role::User, user_message)];

    // Agent loop
    for _iteration in 0..max_iterations {
        // Create request with current input items
        let request = CreateResponseArgs::default()
            .max_output_tokens(4096u32)
            .model(api_model)
            .input(InputParam::Items(input_items.clone()))
            .instructions(SYSTEM_PROMPT)
            .tools(tools.clone())
            .store(false)
            .build()?;

        // Get response from LLM
        let response = client.responses().create(request).await?;

        // Convert OutputItem to InputItem for history tracking
        for output_item in response.output.iter() {
            let item = output_item_to_item(output_item)?;
            input_items.push(InputItem::Item(item));
        }

        // Check if there are function calls in the output
        let function_calls: Vec<FunctionToolCall> = response
            .output
            .iter()
            .filter_map(|output_item| {
                if let OutputItem::FunctionCall(fc) = output_item {
                    Some(fc.clone())
                } else {
                    None
                }
            })
            .collect();

        if !function_calls.is_empty() {
            // Process tool calls
            for function_call in function_calls.iter() {
                let name = &function_call.name;
                let args = &function_call.arguments;

                // Call the appropriate tool
                let response_content = call_tool(name, args, &files, model).await?;

                // Print summary of the tool response
                print_tool_summary(&response_content);

                // Add the function call output to input items
                input_items.push(InputItem::Item(Item::FunctionCallOutput(
                    FunctionCallOutputItemParam {
                        call_id: function_call.call_id.clone(),
                        output: FunctionCallOutput::Text(response_content),
                        id: None,
                        status: None,
                    },
                )));
            }
        } else {
            // No tool calls - we have a final response
            return Ok(response
                .output_text()
                .unwrap_or("<No response>".to_string()));
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
            if let Some(ref paths) = file_paths && !paths.is_empty() {
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

/// Convert an OutputItem to an Item for including in conversation history
fn output_item_to_item(output_item: &OutputItem) -> Result<Item> {
    match output_item {
        OutputItem::Message(msg) => Ok(Item::Message(MessageItem::Output(msg.clone()))),
        OutputItem::FileSearchCall(call) => Ok(Item::FileSearchCall(call.clone())),
        OutputItem::FunctionCall(call) => Ok(Item::FunctionCall(call.clone())),
        OutputItem::WebSearchCall(call) => Ok(Item::WebSearchCall(call.clone())),
        OutputItem::ComputerCall(call) => Ok(Item::ComputerCall(call.clone())),
        OutputItem::Reasoning(reasoning) => Ok(Item::Reasoning(reasoning.clone())),
        OutputItem::ImageGenerationCall(call) => Ok(Item::ImageGenerationCall(call.clone())),
        OutputItem::CodeInterpreterCall(call) => Ok(Item::CodeInterpreterCall(call.clone())),
        OutputItem::LocalShellCall(call) => Ok(Item::LocalShellCall(call.clone())),
        OutputItem::McpCall(call) => Ok(Item::McpCall(call.clone())),
        OutputItem::McpListTools(tools) => Ok(Item::McpListTools(tools.clone())),
        OutputItem::McpApprovalRequest(req) => Ok(Item::McpApprovalRequest(req.clone())),
        OutputItem::CustomToolCall(call) => Ok(Item::CustomToolCall(call.clone())),
        // Handle output types that don't directly map to Item variants
        OutputItem::ShellCall(_)
        | OutputItem::ShellCallOutput(_)
        | OutputItem::ApplyPatchCall(_)
        | OutputItem::ApplyPatchCallOutput(_) => Err(anyhow::anyhow!(
            "OutputItem variant cannot be converted to Item: {:?}",
            output_item
        )),
    }
}
