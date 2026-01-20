//! File Agent - specialized for filesystem operations

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::Context;
use crate::error::Result;
use crate::task::{StepResult, TaskStep, TaskType};
use crate::tools::filesystem::{
    DeleteFile, ListDirectory, MoveFile, ReadFile, SearchFiles, WriteFile,
};
use crate::tools::Tool;

use super::Agent;

/// Agent specialized for file operations
pub struct FileAgent {
    id: String,
    #[allow(dead_code)]
    workspace: PathBuf,
    tools: Vec<Arc<dyn Tool>>,
}

impl FileAgent {
    pub fn new(workspace: PathBuf) -> Self {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(ReadFile::new(workspace.clone())),
            Arc::new(WriteFile::new(workspace.clone())),
            Arc::new(ListDirectory::new(workspace.clone())),
            Arc::new(DeleteFile::new(workspace.clone())),
            Arc::new(MoveFile::new(workspace.clone())),
            Arc::new(SearchFiles::new(workspace.clone())),
        ];

        Self {
            id: "file_agent".to_string(),
            workspace,
            tools,
        }
    }
}

#[async_trait]
impl Agent for FileAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "File Agent"
    }

    fn description(&self) -> &str {
        "Specialized agent for filesystem operations including reading, writing, \
         searching, and organizing files."
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }

    async fn execute(&self, step: &TaskStep, _ctx: &mut Context) -> Result<StepResult> {
        // Find the appropriate tool
        let tool_name = &step.tool_name;
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_name)
            .ok_or_else(|| crate::error::Error::Agent(format!("Tool not found: {}", tool_name)))?;

        // Execute the tool
        let output = tool
            .execute(step.parameters.clone())
            .await
            .map_err(crate::error::Error::Tool)?;

        Ok(StepResult {
            step_id: step.id.clone(),
            output,
            next_steps: Vec::new(),
        })
    }

    fn can_handle(&self, task_type: &TaskType) -> bool {
        matches!(task_type, TaskType::FileOperation | TaskType::Search)
    }

    fn system_prompt(&self) -> &str {
        r#"You are a File Agent specialized in filesystem operations.

Your capabilities include:
- Reading file contents
- Writing and creating files
- Listing directory contents
- Searching for files by name or content
- Moving and renaming files
- Deleting files (with user approval)

Always work within the designated workspace. Be careful with destructive operations.
When searching, use specific patterns to minimize results.
Report errors clearly and suggest alternatives when operations fail."#
    }
}
