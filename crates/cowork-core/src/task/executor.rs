//! Task executor - runs task steps

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::agent::Agent;
use crate::approval::{ApprovalPolicy, ApprovalRequest};
use crate::context::Context;
use crate::error::{Error, Result};

use super::{Task, TaskStatus, TaskSummary};

/// Events emitted during task execution
#[derive(Debug, Clone)]
pub enum TaskEvent {
    StepStarted { step_id: String },
    StepCompleted { step_id: String, success: bool },
    ApprovalRequired { request: ApprovalRequest },
    TaskCompleted { summary: TaskSummary },
    TaskFailed { error: String },
}

/// Executes tasks using agents
pub struct TaskExecutor {
    approval_policy: Arc<dyn ApprovalPolicy>,
    event_tx: Option<mpsc::Sender<TaskEvent>>,
}

impl TaskExecutor {
    pub fn new(approval_policy: Arc<dyn ApprovalPolicy>) -> Self {
        Self {
            approval_policy,
            event_tx: None,
        }
    }

    pub fn with_events(mut self, tx: mpsc::Sender<TaskEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Execute a task using the provided agent
    pub async fn execute(
        &self,
        task: &mut Task,
        agent: &dyn Agent,
        ctx: &mut Context,
    ) -> Result<TaskSummary> {
        let start = std::time::Instant::now();
        let mut completed = HashSet::new();
        let mut errors = Vec::new();

        task.status = TaskStatus::InProgress;

        for step in &task.steps {
            // Check dependencies
            for dep in &step.dependencies {
                if !completed.contains(dep) {
                    return Err(Error::Task(format!(
                        "Dependency {} not completed for step {}",
                        dep, step.id
                    )));
                }
            }

            // Emit step started event
            self.emit(TaskEvent::StepStarted {
                step_id: step.id.clone(),
            })
            .await;

            // Check if approval is needed
            if let Some(tool) = agent.tools().iter().find(|t| t.name() == step.tool_name) {
                let level = tool.approval_level();

                if self.approval_policy.requires_approval(&level) {
                    let request = ApprovalRequest::new(
                        format!("Execute {} with {:?}", step.tool_name, step.parameters),
                        level,
                    );

                    self.emit(TaskEvent::ApprovalRequired {
                        request: request.clone(),
                    })
                    .await;

                    // Wait for approval (in a real implementation)
                    task.status = TaskStatus::WaitingApproval;
                }
            }

            // Execute the step
            match agent.execute(step, ctx).await {
                Ok(result) => {
                    completed.insert(step.id.clone());
                    self.emit(TaskEvent::StepCompleted {
                        step_id: step.id.clone(),
                        success: result.output.success,
                    })
                    .await;

                    if !result.output.success
                        && let Some(err) = result.output.error {
                            errors.push(err);
                        }
                }
                Err(e) => {
                    errors.push(e.to_string());
                    self.emit(TaskEvent::StepCompleted {
                        step_id: step.id.clone(),
                        success: false,
                    })
                    .await;
                }
            }
        }

        let summary = TaskSummary {
            task_id: task.id.clone(),
            status: if errors.is_empty() {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed
            },
            steps_completed: completed.len(),
            steps_total: task.steps.len(),
            duration_ms: start.elapsed().as_millis() as u64,
            errors,
        };

        task.status = summary.status.clone();

        self.emit(TaskEvent::TaskCompleted {
            summary: summary.clone(),
        })
        .await;

        Ok(summary)
    }

    async fn emit(&self, event: TaskEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event).await;
        }
    }
}
