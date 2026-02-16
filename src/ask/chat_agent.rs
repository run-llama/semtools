use anyhow::Result;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage, ChatCompletionTools,
};
use async_openai::{Client, types::chat::CreateChatCompletionRequestArgs};
use model2vec_rs::model::StaticModel;

use crate::ask::system_prompt::{STDIN_SYSTEM_PROMPT, SYSTEM_PROMPT};
use crate::ask::tool_calling::{call_tool, print_tool_summary};
use crate::ask::tools::{AgentTool, GrepTool, ReadTool, SearchTool};
use crate::json_mode::AskOutput;

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
    workspace_name: Option<&str>,
) -> Result<AskOutput> {
    let max_iterations = max_iterations.unwrap_or(20);
    let mut result = AskOutput {
        query: user_message.to_string(),
        response: String::new(),
        files_searched: vec![],
    };

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
                    let response_content =
                        call_tool(name, args, &files, model, &mut result, workspace_name).await?;

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
                result.response = content.clone();
            } else {
                result.response = "<No response>".to_string();
            }

            return Ok(result);
        }
    }

    result.response = format!(
        "Max iterations ({}) reached without final response",
        max_iterations
    );
    Ok(result)
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
) -> Result<AskOutput> {
    // Construct the user message with stdin content
    let full_message = format!(
        "<stdin_content>\n{}\n</stdin_content>\n\n{}",
        stdin_content, user_message
    );
    let mut result = AskOutput {
        query: user_message.to_string(),
        response: String::new(),
        files_searched: vec!["<stdin>".to_string()],
    };

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
        result.response = content;
        Ok(result)
    } else {
        Err(anyhow::anyhow!("No content in response"))
    }
}
