use log::info;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

use crate::types::{Workflow, RegisteredResponse};
use crate::http::execute_task;
use crate::env::EnvResolver;

pub async fn execute_workflow(workflow_path: &str) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    let yaml_str = fs::read_to_string(workflow_path)?;
    let wf: Workflow = serde_yaml::from_str(&yaml_str)?;
    info!("Running workflow: {}", wf.name);

    let client = reqwest::Client::new();
    let mut results: HashMap<String, Value> = HashMap::new();
    let mut registry: HashMap<String, RegisteredResponse> = HashMap::new();

    for mut task in wf.tasks {
        if task.kind != "http" {
            continue;
        }

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

    info!("Workflow complete. Results: {:?}", results.keys());
    Ok(results)
} 