use log::debug;
use std::collections::HashMap;
use crate::types::RegisteredResponse;

pub fn resolve_references(
    body: &mut serde_yaml::Value,
    registry: &HashMap<String, RegisteredResponse>,
) {
    match body {
        serde_yaml::Value::Mapping(map) => {
            for (_, value) in map.iter_mut() {
                resolve_references(value, registry);
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for value in seq.iter_mut() {
                resolve_references(value, registry);
            }
        }
        serde_yaml::Value::String(s) => {
            if s.starts_with("{{") && s.ends_with("}}") {
                let ref_str = s.trim_matches(|c| c == '{' || c == '}');
                // Handle environment variables first
                if let Some(env_var) = ref_str.strip_prefix("env.") {
                    let value = std::env::var(env_var).unwrap_or_else(|_| {
                        debug!("Environment variable {} not found", env_var);
                        "".to_string()
                    });
                    debug!("Resolved env var {} to '{}'", env_var, value);
                    *body = serde_yaml::Value::String(value);
                    return;
                }
                // Then handle registered response references
                debug!("Resolving reference: {}", ref_str);
                let parts: Vec<&str> = ref_str.split('.').collect();
                if parts.len() >= 2 {
                    debug!("Looking for registered response: {}", parts[0]);
                    if let Some(response) = registry.get(parts[0]) {
                        let json = &response.json;
                        debug!("Found registered response: {:?}", json);
                        let mut current = json;
                        // Skip the first part (task name) and second part (json) since the response is already JSON
                        for part in parts[2..].iter() {
                            debug!("Traversing path: {}", part);
                            if part.contains('[') && part.contains(']') {
                                let (key, index) = part.split_once('[').unwrap();
                                let index = index.trim_end_matches(']').parse::<usize>().unwrap();
                                current = &current[key][index];
                            } else {
                                current = &current[part];
                            }
                            debug!("Current value: {:?}", current);
                        }
                        // Convert the JSON value to YAML value and replace the string
                        let yaml_value = serde_yaml::to_value(current).unwrap();
                        debug!("Converting to YAML: {:?}", yaml_value);
                        *body = yaml_value;
                    } else {
                        debug!("No registered response found for: {}", parts[0]);
                    }
                }
            }
        }
        _ => {}
    }
} 