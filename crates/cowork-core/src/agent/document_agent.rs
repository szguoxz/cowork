//! Document Agent - specialized for document processing

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::Context;
use crate::error::Result;
use crate::task::{StepResult, TaskStep, TaskType};
use crate::tools::document::{ReadOfficeDoc, ReadPdf};
use crate::tools::Tool;

use super::Agent;

/// Agent specialized for document processing
pub struct DocumentAgent {
    id: String,
    tools: Vec<Arc<dyn Tool>>,
}

impl DocumentAgent {
    pub fn new(workspace: PathBuf) -> Self {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(ReadPdf::new(workspace.clone())),
            Arc::new(ReadOfficeDoc::new(workspace)),
        ];

        Self {
            id: "document_agent".to_string(),
            tools,
        }
    }
}

#[async_trait]
impl Agent for DocumentAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Document Agent"
    }

    fn description(&self) -> &str {
        "Specialized agent for processing documents including PDFs and Office files."
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }

    async fn execute(&self, step: &TaskStep, _ctx: &mut Context) -> Result<StepResult> {
        let tool_name = &step.tool_name;
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_name)
            .ok_or_else(|| crate::error::Error::Agent(format!("Tool not found: {}", tool_name)))?;

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
        matches!(task_type, TaskType::DocumentProcessing)
    }

    fn system_prompt(&self) -> &str {
        r#"You are a Document Agent specialized in document processing.

Your capabilities include:
- Reading and extracting text from PDFs
- Processing Word documents
- Reading Excel spreadsheets
- Processing PowerPoint presentations

When extracting content, preserve structure where possible.
Handle large documents by processing in chunks if needed.
Report document metadata along with content."#
    }
}
