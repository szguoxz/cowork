//! Browser Agent - specialized for web automation

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::Context;
use crate::error::Result;
use crate::task::{StepResult, TaskStep, TaskType};
use crate::tools::browser::{BrowserController, ClickElement, NavigateTo, TakeScreenshot};
use crate::tools::Tool;

use super::Agent;

/// Agent specialized for browser automation
pub struct BrowserAgent {
    id: String,
    controller: BrowserController,
    tools: Vec<Arc<dyn Tool>>,
}

impl BrowserAgent {
    pub fn new(output_dir: PathBuf, headless: bool) -> Self {
        let controller = BrowserController::new(headless);
        let session = controller.session();

        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(NavigateTo::new(session.clone())),
            Arc::new(TakeScreenshot::new(session.clone(), output_dir)),
            Arc::new(ClickElement::new(session)),
        ];

        Self {
            id: "browser_agent".to_string(),
            controller,
            tools,
        }
    }

    pub fn controller(&self) -> &BrowserController {
        &self.controller
    }
}

#[async_trait]
impl Agent for BrowserAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Browser Agent"
    }

    fn description(&self) -> &str {
        "Specialized agent for web browser automation including navigation, \
         screenshots, and page interaction."
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
            .map_err(|e| crate::error::Error::Tool(e))?;

        Ok(StepResult {
            step_id: step.id.clone(),
            output,
            next_steps: Vec::new(),
        })
    }

    fn can_handle(&self, task_type: &TaskType) -> bool {
        matches!(task_type, TaskType::WebAutomation | TaskType::Screenshot)
    }

    fn system_prompt(&self) -> &str {
        r#"You are a Browser Agent specialized in web automation.

Your capabilities include:
- Navigating to URLs
- Taking screenshots of pages or elements
- Clicking elements on pages
- Filling forms
- Extracting page content

Always wait for pages to load before interacting.
Use specific CSS selectors when targeting elements.
Take screenshots to verify page state when debugging.
Handle navigation timeouts gracefully."#
    }
}
