//! Config tool - Runtime settings management
//!
//! Allows getting and setting configuration values at runtime.


use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::ConfigManager;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

/// Tool for managing configuration
pub struct ConfigTool {
    config_manager: Arc<RwLock<ConfigManager>>,
}

impl ConfigTool {
    pub fn new(config_manager: Arc<RwLock<ConfigManager>>) -> Self {
        Self { config_manager }
    }
}


impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "Config"
    }

    fn description(&self) -> &str {
        "Get or set configuration values. Use to read current settings or modify them. \
         Common settings include 'theme', 'model', 'permissions.defaultMode'."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "setting": {
                    "type": "string",
                    "description": "The setting key (e.g., 'theme', 'model', 'permissions.defaultMode')"
                },
                "value": {
                    "description": "The new value. Omit to get current value.",
                    "oneOf": [
                        { "type": "string" },
                        { "type": "boolean" },
                        { "type": "number" }
                    ]
                }
            },
            "required": ["setting"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        let setting = params["setting"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("setting is required".into()))?;

        let config = self.config_manager.read().await;

        // If no value provided, get current value
        if params.get("value").is_none() || params["value"].is_null() {
            let current_value = get_config_value(&config, setting);
            return Ok(ToolOutput::success(json!({
                "setting": setting,
                "value": current_value,
                "readonly": true
            })));
        }

        // Set value
        drop(config);
        let mut config = self.config_manager.write().await;
        let new_value = &params["value"];

        match set_config_value(&mut config, setting, new_value.clone()) {
            Ok(_) => {
                // Save config
                if let Err(e) = config.save() {
                    return Err(ToolError::ExecutionFailed(format!(
                        "Failed to save config: {}",
                        e
                    )));
                }

                Ok(ToolOutput::success(json!({
                    "setting": setting,
                    "value": new_value,
                    "updated": true
                })))
            }
            Err(e) => Err(ToolError::InvalidParams(e)),
        }
            })
    }
}

fn get_config_value(config: &ConfigManager, key: &str) -> Value {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // Provider settings (from default provider)
        ["provider"] | ["default_provider"] => json!(config.default_provider()),
        ["model"] => {
            if let Some(provider) = config.config().get_default_provider() {
                json!(provider.model)
            } else {
                Value::Null
            }
        }
        ["providers"] => {
            json!(config.list_providers())
        }
        // Approval settings
        ["auto_approve_level"] => json!(config.config().approval.auto_approve_level),
        ["show_dialogs"] => json!(config.config().approval.show_dialogs),
        ["approval_timeout"] => json!(config.config().approval.timeout_secs),
        // General settings
        ["log_level"] => json!(config.config().general.log_level),
        ["telemetry"] => json!(config.config().general.telemetry),
        _ => Value::Null,
    }
}

fn set_config_value(config: &mut ConfigManager, key: &str, value: Value) -> Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // Provider settings (updates default provider)
        ["default_provider"] | ["provider"] => {
            let provider_name = value.as_str().ok_or("provider must be a string")?;
            config.set_default_provider(provider_name);
        }
        ["model"] => {
            let model = value.as_str().ok_or("model must be a string")?.to_string();
            let default_provider = config.default_provider().to_string();
            if let Some(provider) = config.config_mut().get_provider_mut(&default_provider) {
                provider.model = model;
            } else {
                return Err("No default provider configured".to_string());
            }
        }
        // Approval settings
        ["auto_approve_level"] => {
            let level = value
                .as_str()
                .ok_or("auto_approve_level must be a string")?
                .to_string();
            config.config_mut().approval.auto_approve_level = level;
        }
        ["show_dialogs"] => {
            let show = value.as_bool().ok_or("show_dialogs must be a boolean")?;
            config.config_mut().approval.show_dialogs = show;
        }
        ["approval_timeout"] => {
            let timeout = value.as_u64().ok_or("approval_timeout must be a number")?;
            config.config_mut().approval.timeout_secs = timeout;
        }
        // General settings
        ["log_level"] => {
            let level = value.as_str().ok_or("log_level must be a string")?.to_string();
            config.config_mut().general.log_level = level;
        }
        ["telemetry"] => {
            let enabled = value.as_bool().ok_or("telemetry must be a boolean")?;
            config.config_mut().general.telemetry = enabled;
        }
        _ => return Err(format!("Unknown setting: {}", key)),
    }

    Ok(())
}
