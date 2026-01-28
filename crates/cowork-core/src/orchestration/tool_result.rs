//! Tool result formatting
//!
//! Provides consistent formatting of tool results for LLM consumption.

/// Format a tool result for sending back to the LLM
///
/// This creates a standardized format that helps the LLM understand
/// the result is from a tool execution, not a new user request.
pub fn format_tool_result_for_llm(tool_call_id: &str, result: &str) -> String {
    format!(
        "[Tool result for {}]\n{}\n[End of tool result. Please summarize the above for the user.]",
        tool_call_id,
        result
    )
}
