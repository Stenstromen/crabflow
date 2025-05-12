use log::debug;
use std::collections::HashMap;
use crate::types::RegisteredResponse;

pub fn resolve_references(
    body: &mut serde_yaml::Value,
    registry: &HashMap<String, RegisteredResponse>
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
                let ref_str = s.trim_matches(|c| (c == '{' || c == '}'));
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_yaml::Value;

    fn create_test_registry() -> HashMap<String, RegisteredResponse> {
        let mut registry = HashMap::new();
        registry.insert("task1".to_string(), RegisteredResponse {
            json: json!({
                    "data": {
                        "users": [
                            {"name": "John", "age": 30},
                            {"name": "Jane", "age": 25}
                        ],
                        "settings": {
                            "enabled": true
                        }
                    }
                }),
            text: "Test response text".to_string(),
        });
        registry.insert("urlencoded".to_string(), RegisteredResponse {
            json: json!({
                    "args": {
                        "foo": ["bar"]
                    }
                }),
            text: "Test response text".to_string(),
        });
        registry
    }

    #[test]
    fn test_resolve_env_variable() {
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
        }
        let yaml_str = r#"
            key: "{{env.TEST_VAR}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = HashMap::new();

        resolve_references(&mut value, &registry);

        assert_eq!(value["key"], "test_value");
    }

    #[test]
    fn test_resolve_env_in_url() {
        // Clear any existing value first
        unsafe {
            std::env::remove_var("PASSWD");
            std::env::set_var("PASSWD", "secret123");
        }

        assert_eq!(std::env::var("PASSWD").unwrap(), "secret123");

        let yaml_str =
            r#"
            url:
              base: "http://localhost:8080/basic-auth/user/"
              password: "{{env.PASSWD}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = HashMap::new();

        resolve_references(&mut value, &registry);

        assert_eq!(value["url"]["base"], "http://localhost:8080/basic-auth/user/");
        assert_eq!(value["url"]["password"], "secret123");
    }

    #[test]
    fn test_resolve_env_in_headers() {
        unsafe {
            std::env::set_var("X_API_KEY", "api-key-123");
        }
        let yaml_str =
            r#"
            headers:
              X-Api-Key: "{{env.X_API_KEY}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = HashMap::new();

        resolve_references(&mut value, &registry);

        assert_eq!(value["headers"]["X-Api-Key"], "api-key-123");
    }

    #[test]
    fn test_resolve_env_in_form_data() {
        unsafe {
            std::env::set_var("FOO", "bar-value");
        }
        let yaml_str = r#"
            body:
              foo: "{{env.FOO}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = HashMap::new();

        resolve_references(&mut value, &registry);

        assert_eq!(value["body"]["foo"], "bar-value");
    }

    #[test]
    fn test_resolve_registered_response() {
        let yaml_str =
            r#"
            user: "{{task1.json.data.users[0].name}}"
            setting: "{{task1.json.data.settings.enabled}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = create_test_registry();

        resolve_references(&mut value, &registry);

        assert_eq!(value["user"], "John");
        assert_eq!(value["setting"], true);
    }

    #[test]
    fn test_resolve_nested_structure() {
        let yaml_str =
            r#"
            config:
              user_info:
                name: "{{task1.json.data.users[1].name}}"
                age: "{{task1.json.data.users[1].age}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = create_test_registry();

        resolve_references(&mut value, &registry);

        assert_eq!(value["config"]["user_info"]["name"], "Jane");
        assert_eq!(value["config"]["user_info"]["age"], 25);
    }

    #[test]
    fn test_resolve_array() {
        let yaml_str =
            r#"
            users:
              - "{{task1.json.data.users[0].name}}"
              - "{{task1.json.data.users[1].name}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = create_test_registry();

        resolve_references(&mut value, &registry);

        assert_eq!(value["users"][0], "John");
        assert_eq!(value["users"][1], "Jane");
    }

    #[test]
    fn test_resolve_simple_response_reference() {
        let yaml_str = r#"
            args: "{{urlencoded.json.args}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = create_test_registry();

        resolve_references(&mut value, &registry);

        assert_eq!(value["args"]["foo"][0], "bar");
    }

    #[test]
    fn test_resolve_nested_array_reference() {
        let yaml_str = r#"
            foo: "{{urlencoded.json.args.foo[0]}}"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = create_test_registry();

        resolve_references(&mut value, &registry);

        assert_eq!(value["foo"], "bar");
    }

    #[test]
    fn test_non_reference_string() {
        let yaml_str = r#"
            key: "regular string"
        "#;
        let mut value: Value = serde_yaml::from_str(yaml_str).unwrap();
        let registry = HashMap::new();

        resolve_references(&mut value, &registry);

        assert_eq!(value["key"], "regular string");
    }
}
