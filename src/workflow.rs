use log::info;
use prettytable::{Table, row};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

use crate::env::EnvResolver;
use crate::http::execute_task;
use crate::types::{RegisteredResponse, Workflow};

fn display_registered_variables(registry: &HashMap<String, RegisteredResponse>) {
    if registry.is_empty() {
        info!("No variables have been registered.");
        return;
    }

    let mut table = Table::new();
    table.add_row(row!["Variable Name", "Value"]);

    for (name, response) in registry {
        let value = serde_json::to_string_pretty(&response.json)
            .unwrap_or_else(|_| "Error serializing value".to_string());
        table.add_row(row![name, value]);
    }

    info!("\nRegistered Variables:");
    table.printstd();
}

fn display_specific_variables(
    registry: &HashMap<String, RegisteredResponse>,
    variables: &[String],
) {
    let mut table = Table::new();
    table.add_row(row!["Variable Name", "Value"]);

    for var_name in variables {
        if let Some(response) = registry.get(var_name) {
            let value = serde_json::to_string_pretty(&response.json)
                .unwrap_or_else(|_| "Error serializing value".to_string());
            table.add_row(row![var_name, value]);
        } else {
            info!("Warning: Variable '{}' not found in registry", var_name);
        }
    }

    info!("\nDisplaying Selected Variables:");
    table.printstd();
}

pub async fn execute_workflow(
    workflow_path: &str,
) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    let yaml_str = fs::read_to_string(workflow_path)?;
    let wf: Workflow = serde_yaml::from_str(&yaml_str)?;
    info!("Running workflow: {}", wf.name);

    let client = reqwest::Client::new();
    let mut results: HashMap<String, Value> = HashMap::new();
    let mut registry: HashMap<String, RegisteredResponse> = HashMap::new();

    for mut task in wf.tasks {
        match task.kind.as_str() {
            "http" => {
                // Resolve environment variables in the task
                task.resolve_env_vars();

                for dep in &task.depends_on {
                    if !results.contains_key(dep) {
                        panic!("Missing dependency: {}", dep);
                    }
                }

                let (json, text) = execute_task(&mut task, &client, &registry).await?;
                results.insert(task.name.clone(), json.clone());

                // Register the response if requested
                if let Some(register_name) = &task.register {
                    registry.insert(
                        register_name.clone(),
                        RegisteredResponse {
                            json: json.clone(),
                            text: text.clone(),
                        },
                    );
                    info!("Registered response as '{}'", register_name);
                }
            }
            "display" => {
                if let Some(variables) = task.variables {
                    display_specific_variables(&registry, &variables);
                } else {
                    display_registered_variables(&registry);
                }
            }
            _ => {
                info!("Unknown task type: {}", task.kind);
            }
        }
    }

    info!("Workflow complete. Results: {:?}", results.keys());
    Ok(results)
}
