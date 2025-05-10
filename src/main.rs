use env_logger::Builder;
use log::{LevelFilter, debug, error, info, trace};
use serde::Deserialize;
use serde::de::Deserializer;
use serde_json::Value;
use std::{collections::HashMap, fs};
use tokio::time::{Duration, sleep};

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
enum Expect {
    Status { code: u16 },
    JsonPath { path: String, value: String },
    Raw { contains: String },
}

fn deserialize_expect<'de, D>(deserializer: D) -> Result<Vec<Expect>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SingleOrVec {
        Single(Expect),
        Vec(Vec<Expect>),
    }

    match SingleOrVec::deserialize(deserializer)? {
        SingleOrVec::Single(expect) => Ok(vec![expect]),
        SingleOrVec::Vec(expects) => Ok(expects),
    }
}

#[derive(Debug, Deserialize)]
struct Workflow {
    name: String,
    tasks: Vec<Task>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
enum BodyType {
    FormUrlencoded,
    Json,
    Raw,
    FormMultipart,
}

#[derive(Debug, Deserialize, Clone)]
struct Task {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    method: String,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<serde_yaml::Value>,
    #[serde(default)]
    body_type: Option<BodyType>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default = "default_retries")]
    retries: u32,
    #[serde(default = "default_retry_delay")]
    retry_delay: u64,
    #[serde(default, deserialize_with = "deserialize_expect")]
    expect: Vec<Expect>,
    #[serde(default)]
    register: Option<String>,
}

fn default_retries() -> u32 {
    1
}

fn default_retry_delay() -> u64 {
    5
}

// Add this struct to store registered responses
#[derive(Clone)]
#[allow(dead_code)]
struct RegisteredResponse {
    json: Value,
    text: String,
}

// Add this function to resolve references in the body
fn resolve_references(
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger with Info as default only if RUST_LOG is not set
    let mut builder = Builder::from_default_env();
    if std::env::var("RUST_LOG").is_err() {
        builder.filter_level(LevelFilter::Info);
    }
    builder.format_timestamp_millis().init();

    let yaml_str = fs::read_to_string("workflow.yml")?;
    let wf: Workflow = serde_yaml::from_str(&yaml_str)?;
    info!("Running workflow: {}", wf.name);

    let client = reqwest::Client::new();
    let mut results: HashMap<String, Value> = HashMap::new();
    let mut registry: HashMap<String, RegisteredResponse> = HashMap::new();

    for task in wf.tasks {
        if task.kind != "http" {
            continue;
        }

        let task_name = task.name.clone();
        let task_headers = task.headers.clone();
        let task_body = task.body.clone();
        let task_body_type = task.body_type.clone();
        let task_retries = task.retries;
        let task_retry_delay = task.retry_delay;
        let task_expect = task.expect.clone();

        for dep in &task.depends_on {
            if !results.contains_key(dep) {
                panic!("Missing dependency: {}", dep);
            }
        }

        let mut attempt = 0;
        loop {
            attempt += 1;
            info!("Executing task `{}` (attempt {})...", task_name, attempt);

            let mut headers = reqwest::header::HeaderMap::new();

            headers.insert(
                reqwest::header::HeaderName::from_static("user-agent"),
                reqwest::header::HeaderValue::from_str("crabflow/0.0.0").unwrap(),
            );
            headers.insert(
                reqwest::header::HeaderName::from_static("x-crabflow-task"),
                reqwest::header::HeaderValue::from_str(&task_name).unwrap(),
            );

            for (k, v) in &task_headers {
                let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap();
                let value = reqwest::header::HeaderValue::from_str(v).unwrap();
                headers.insert(name, value);
            }

            debug!("Request headers: {:?}", headers);
            let mut req = client
                .request(task.method.parse()?, &task.url)
                .headers(headers);
            trace!("Request URL: {}", task.url);
            trace!("Request method: {}", task.method);

            let mut task_body = task_body.clone();
            if let Some(mut body_val) = task_body {
                resolve_references(&mut body_val, &registry);
                task_body = Some(body_val);
            }

            if let Some(body_val) = task_body {
                let body_type = task_body_type.clone().unwrap_or(BodyType::Json);
                trace!("Request body type: {:?}", body_type);
                match body_type {
                    BodyType::Json => {
                        // Convert YAML to JSON string
                        let body_json =
                            serde_json::to_string(&serde_json::to_value(body_val).unwrap())?;
                        debug!("Request body (JSON): {}", body_json);
                        req = req
                            .header("Content-Type", "application/json")
                            .body(body_json);
                    }
                    BodyType::FormUrlencoded => {
                        let form_data = match body_val {
                            serde_yaml::Value::Mapping(map) => {
                                if task.method.to_uppercase() == "GET" {
                                    // For GET requests, append parameters to URL
                                    let query_params: Vec<String> = map
                                        .iter()
                                        .map(|(k, v)| {
                                            let key = k.as_str().unwrap_or_default();
                                            let value = v.as_str().unwrap_or_default();
                                            format!("{}={}", key, value)
                                        })
                                        .collect();
                                    let query_string = query_params.join("&");
                                    let url = if task.url.contains('?') {
                                        format!("{}&{}", task.url, query_string)
                                    } else {
                                        format!("{}?{}", task.url, query_string)
                                    };
                                    req = client.request(task.method.parse()?, &url);
                                    String::new() // No body for GET requests
                                } else {
                                    // For other methods, send as form body
                                    map.iter()
                                        .map(|(k, v)| {
                                            let key = k.as_str().unwrap_or_default();
                                            let value = v.as_str().unwrap_or_default();
                                            format!("{}={}", key, value)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("&")
                                }
                            }
                            _ => return Err("Form URL encoded body must be a key-value map".into()),
                        };

                        if !form_data.is_empty() {
                            debug!("Request body (form-urlencoded): {}", form_data);
                            req = req
                                .header("Content-Type", "application/x-www-form-urlencoded")
                                .body(form_data);
                        }
                    }
                    BodyType::Raw => {
                        let raw_body = body_val.as_str().unwrap_or_default();
                        debug!("Request body (raw): {}", raw_body);
                        req = req.body(raw_body.to_string());
                    }
                    BodyType::FormMultipart => {
                        let mut form = reqwest::multipart::Form::new();
                        match body_val {
                            serde_yaml::Value::Mapping(map) => {
                                for (k, v) in map {
                                    let key = k.as_str().unwrap_or_default().to_string();
                                    let value = v.as_str().unwrap_or_default().to_string();
                                    trace!("Adding form field: {}={}", key, value);
                                    form = form.text(key, value);
                                }
                            }
                            _ => return Err("Multipart form body must be a key-value map".into()),
                        }
                        debug!("Request body (multipart): {:?}", form);
                        req = req.multipart(form);
                    }
                }
            }

            let resp = req.send().await;
            match resp {
                Ok(r) => {
                    let status = r.status();
                    let headers = r.headers().clone();
                    let text = r.text().await?;
                    debug!("Response status: {}", status);
                    debug!("Response body: {}", text);
                    trace!("Response headers: {:?}", headers);

                    // Check all expectations
                    let mut all_expectations_met = true;
                    for expect in &task_expect {
                        trace!("Checking expectation: {:?}", expect);
                        match expect {
                            Expect::Status { code } => {
                                let status_code = status.as_u16();
                                debug!("Expected status: {}, got: {}", code, status_code);
                                if status_code != *code {
                                    error!(
                                        "Task `{}` failed: expected status {} but got {}",
                                        task_name, code, status
                                    );
                                    all_expectations_met = false;
                                    break;
                                }
                            }
                            Expect::JsonPath { path, value } => {
                                let json: Value = serde_json::from_str(&text)?;
                                let mut current = &json;
                                for part in path.split('.') {
                                    trace!("Traversing JSON path: {}", part);
                                    if part.contains('[') && part.contains(']') {
                                        // Handle array indexing
                                        let (key, index) = part.split_once('[').unwrap();
                                        let index =
                                            index.trim_end_matches(']').parse::<usize>().unwrap();
                                        current = &current[key][index];
                                    } else {
                                        current = &current[part];
                                    }
                                }
                                let current_str = match current {
                                    Value::Null => "null".to_string(),
                                    _ => current.to_string().trim_matches('"').to_string(),
                                };
                                debug!(
                                    "JSON path {}: expected {}, got {}",
                                    path, value, current_str
                                );
                                if current_str != *value {
                                    error!(
                                        "Task `{}` failed: expected {} = {} but got {}",
                                        task_name, path, value, current_str
                                    );
                                    all_expectations_met = false;
                                    break;
                                }
                            }
                            Expect::Raw { contains } => {
                                debug!("Checking for raw text: {}", contains);
                                if !text.contains(contains) {
                                    error!(
                                        "Task `{}` failed: response does not contain '{}'",
                                        task_name, contains
                                    );
                                    all_expectations_met = false;
                                    break;
                                }
                            }
                        }
                    }

                    if !all_expectations_met {
                        if attempt > task_retries {
                            break;
                        }
                        info!(
                            "Retrying `{}` in {} seconds...",
                            task_name, task_retry_delay
                        );
                        sleep(Duration::from_secs(task_retry_delay)).await;
                        continue;
                    }

                    // If we have a status expectation and it was met, consider it a success
                    let has_status_expectation = task_expect
                        .iter()
                        .any(|e| matches!(e, Expect::Status { .. }));
                    if has_status_expectation {
                        info!(
                            "Task `{}` succeeded with expected status {}",
                            task_name, status
                        );
                        let json = serde_json::json!({ "status": status.as_u16() });
                        results.insert(task_name.clone(), json.clone());

                        // Register the response if requested, even for status expectations
                        if let Some(register_name) = &task.register {
                            // Parse the response text as JSON if possible
                            let json = match serde_json::from_str(&text) {
                                Ok(json) => json,
                                Err(_) => serde_json::json!({ "text": text }),
                            };
                            registry.insert(
                                register_name.clone(),
                                RegisteredResponse {
                                    json,
                                    text: text.clone(),
                                },
                            );
                            info!("Registered response as '{}'", register_name);
                        }

                        break;
                    }

                    // For other cases, check if status is successful
                    if status.is_success() {
                        // Only try to parse as JSON if we're not using Raw expectation
                        let json: Value =
                            if task_expect.iter().any(|e| matches!(e, Expect::Raw { .. })) {
                                // For Raw expectations, create a simple JSON object with the text
                                serde_json::json!({ "text": text })
                            } else {
                                match serde_json::from_str(&text) {
                                    Ok(json) => json,
                                    Err(e) => {
                                        error!("Failed to parse response as JSON: {}", e);
                                        error!("Response text: {}", text);
                                        if attempt > task_retries {
                                            break;
                                        }
                                        info!(
                                            "Retrying `{}` in {} seconds...",
                                            task_name, task_retry_delay
                                        );
                                        sleep(Duration::from_secs(task_retry_delay)).await;
                                        continue;
                                    }
                                }
                            };
                        info!("Task `{}` succeeded", task_name);
                        results.insert(task_name.clone(), json.clone());
                        debug!("Response JSON: {}", json);

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

                        break;
                    } else {
                        error!("Task `{}` failed with status {}", task_name, status);
                        debug!("Error response: {}", text);
                        if attempt > task_retries {
                            break;
                        }
                        info!(
                            "Retrying `{}` in {} seconds...",
                            task_name, task_retry_delay
                        );
                        sleep(Duration::from_secs(task_retry_delay)).await;
                    }
                }
                Err(e) => {
                    error!("Task `{}` error: {}", task_name, e);
                    if attempt > task_retries {
                        break;
                    }
                    info!(
                        "Retrying `{}` in {} seconds...",
                        task_name, task_retry_delay
                    );
                    sleep(Duration::from_secs(task_retry_delay)).await;
                }
            }

            if attempt > task_retries {
                error!("Task `{}` exceeded retry limit", task_name);
                break;
            }
        }
    }

    info!("Workflow complete. Results: {:?}", results.keys());
    Ok(())
}
