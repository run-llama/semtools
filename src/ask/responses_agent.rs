use anyhow::Result;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::responses::{
    CreateResponseArgs, EasyInputContent, EasyInputMessage, FunctionCallOutput,
    FunctionCallOutputItemParam, FunctionToolCall, InputItem, InputParam, Item, MessageItem,
    MessageType, OutputItem, Role, Tool,
};
use model2vec_rs::model::StaticModel;

use crate::ask::system_prompt::{STDIN_SYSTEM_PROMPT, SYSTEM_PROMPT};
use crate::ask::tool_calling::{call_tool, print_tool_summary};
use crate::ask::tools::{AgentTool, GrepTool, ReadTool, SearchTool};
use crate::json_mode::AskOutput;

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
/// AskOutput containing the query, response, and files searched
pub async fn ask_agent_responses(
    files: Vec<String>,
    user_message: &str,
    model: &StaticModel,
    client: &Client<OpenAIConfig>,
    api_model: &str,
    max_iterations: Option<usize>,
) -> Result<AskOutput> {
    let max_iterations = max_iterations.unwrap_or(20);
    let mut result = AskOutput {
        query: user_message.to_string(),
        response: String::new(),
        files_searched: vec![],
    };

    // Build the tools using the responses API format
    let tools: Vec<Tool> = vec![
        GrepTool::responses_definition()?,
        SearchTool::responses_definition()?,
        ReadTool::responses_definition()?,
    ];

    // Initialize input items with user message
    // Note: For Responses API, we use the instructions parameter for the system prompt
    let mut input_items: Vec<InputItem> = vec![InputItem::EasyMessage(EasyInputMessage {
        r#type: MessageType::Message,
        role: Role::User,
        content: EasyInputContent::Text(user_message.to_string()),
    })];

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
                let response_content = call_tool(name, args, &files, model, &mut result).await?;

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
            let response_text = response
                .output_text()
                .unwrap_or("<No response>".to_string());

            return Ok(AskOutput {
                query: user_message.to_string(),
                response: response_text,
                files_searched: result.files_searched,
            });
        }
    }

    // If we reach here, max iterations was hit
    Ok(AskOutput {
        query: user_message.to_string(),
        response: format!(
            "Max iterations ({}) reached without final response",
            max_iterations
        ),
        files_searched: result.files_searched,
    })
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

/// Run an agent with stdin content injected directly using Responses API (no tools available)
///
/// # Arguments
/// * `stdin_content` - The content from stdin to include in the prompt
/// * `user_message` - The user's query/message
/// * `client` - OpenAI API client
/// * `api_model` - The LLM model to use (e.g., "gpt-4.1")
///
/// # Returns
/// AskOutput containing the query, response, and "<stdin>" as the file searched
pub async fn ask_agent_responses_with_stdin(
    stdin_content: &str,
    user_message: &str,
    client: &Client<OpenAIConfig>,
    api_model: &str,
) -> Result<AskOutput> {
    // Construct the user message with stdin content
    let full_message = format!(
        "<stdin_content>\n{}\n</stdin_content>\n\n{}",
        stdin_content, user_message
    );

    // Initialize input items with user message (no tools)
    let input_items: Vec<InputItem> = vec![InputItem::EasyMessage(EasyInputMessage {
        r#type: MessageType::Message,
        role: Role::User,
        content: EasyInputContent::Text(full_message),
    })];

    // Create request without tools
    let request = CreateResponseArgs::default()
        .max_output_tokens(4096u32)
        .model(api_model)
        .input(InputParam::Items(input_items))
        .instructions(STDIN_SYSTEM_PROMPT)
        .store(false)
        .build()?;

    // Get response from LLM
    let response = client.responses().create(request).await?;

    // Return AskOutput with stdin as the file searched
    let response_text = response
        .output_text()
        .unwrap_or("<No response>".to_string());

    Ok(AskOutput {
        query: user_message.to_string(),
        response: response_text,
        files_searched: vec!["<stdin>".to_string()],
    })
}
