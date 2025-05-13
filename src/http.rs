use base64;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use log::{debug, error, info, trace};
use reqwest;
use serde_json::Value;
use std::collections::HashMap;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::time::{Duration, sleep};

use crate::resolve::resolve_references;
use crate::types::{Expect, RegisteredResponse, Task};

pub async fn execute_task(
    task: &mut Task,
    client: &reqwest::Client,
    registry: &HashMap<String, RegisteredResponse>,
) -> Result<(Value, String), Box<dyn std::error::Error>> {
    let task_name = task.name.clone();
    let task_headers = task.headers.clone();
    let task_body = task.body.clone();
    let task_body_type = task.body_type.clone();
    let task_retries = task.retries;
    let task_retry_delay = task.retry_delay;
    let task_expect = task.expect.clone();
    let task_auth = task.auth.clone();

    let mut attempt = 0;
    loop {
        attempt += 1;
        info!("Executing task `{}` (attempt {})...", task_name, attempt);

        let mut headers = reqwest::header::HeaderMap::new();

        headers.insert(
            reqwest::header::HeaderName::from_static("user-agent"),
            reqwest::header::HeaderValue::from_str(&format!(
                "crabflow/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .unwrap(),
        );

        if log::log_enabled!(log::Level::Debug) || log::log_enabled!(log::Level::Trace) {
            headers.insert(
                reqwest::header::HeaderName::from_static("x-crabflow-task"),
                reqwest::header::HeaderValue::from_str(&task_name).unwrap(),
            );
        }

        // Add basic auth if provided
        if let Some(auth) = &task_auth {
            let credentials = format!("{}:{}", auth.username, auth.password);
            let encoded = BASE64.encode(credentials);
            let auth_header = format!("Basic {}", encoded);
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_header).unwrap(),
            );
        }

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
            resolve_references(&mut body_val, registry);
            task_body = Some(body_val);
        }

        if let Some(body_val) = task_body {
            let body_type = task_body_type
                .clone()
                .unwrap_or(crate::types::BodyType::Json);
            trace!("Request body type: {:?}", body_type);
            match body_type {
                crate::types::BodyType::Json => {
                    // Convert YAML to JSON string
                    let body_json =
                        serde_json::to_string(&serde_json::to_value(body_val).unwrap())?;
                    debug!("Request body (JSON): {}", body_json);
                    req = req
                        .header("Content-Type", "application/json")
                        .body(body_json);
                }
                crate::types::BodyType::FormUrlencoded => {
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
                        _ => {
                            return Err("Form URL encoded body must be a key-value map".into());
                        }
                    };

                    if !form_data.is_empty() {
                        debug!("Request body (form-urlencoded): {}", form_data);
                        req = req
                            .header("Content-Type", "application/x-www-form-urlencoded")
                            .body(form_data);
                    }
                }
                crate::types::BodyType::Raw => {
                    let raw_body = body_val.as_str().unwrap_or_default();
                    debug!("Request body (raw): {}", raw_body);
                    req = req.body(raw_body.to_string());
                }
                crate::types::BodyType::FormMultipart => {
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
                        _ => {
                            return Err("Multipart form body must be a key-value map".into());
                        }
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

                // Check if we should save as file
                if let Some(save_path) = &task.save_as {
                    // Get content type to check if it's a stream
                    let content_type = headers
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    // If it's a stream, binary content, or an image, save as file
                    if content_type.contains("stream")
                        || content_type.contains("octet-stream")
                        || content_type.starts_with("image/")
                    {
                        let mut file = File::create(save_path).await?;
                        let bytes = r.bytes().await?;
                        file.write_all(&bytes).await?;

                        // Create a JSON response with file info
                        let json = serde_json::json!({
                            "status": status.as_u16(),
                            "saved_as": save_path,
                            "content_type": content_type,
                            "size": bytes.len()
                        });

                        info!(
                            "Task `{}` succeeded and saved response to {}",
                            task_name, save_path
                        );
                        return Ok((json, format!("Response saved to {}", save_path)));
                    }
                }

                // Handle regular responses as before
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
                    // Parse the response JSON instead of creating a new status-only object
                    let json: Value = if task_expect.iter().any(|e| matches!(e, Expect::Raw { .. }))
                    {
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
                    return Ok((json, text));
                }

                // For other cases, check if status is successful
                if status.is_success() {
                    // Only try to parse as JSON if we're not using Raw expectation
                    let json: Value = if task_expect.iter().any(|e| matches!(e, Expect::Raw { .. }))
                    {
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
                    return Ok((json, text));
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

    Err("Task failed after all retries".into())
}
