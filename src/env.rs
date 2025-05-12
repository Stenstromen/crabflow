use log::debug;
use crate::types::Task;

pub trait EnvResolver {
    fn resolve_env_vars(&mut self);
}

impl EnvResolver for Task {
    fn resolve_env_vars(&mut self) {
        // Resolve environment variables in URL
        let mut url = self.url.clone();
        let mut start = 0;
        while let Some(pos) = url[start..].find("{{env.") {
            let start_pos = start + pos;
            if let Some(end_pos) = url[start_pos..].find("}}") {
                let end_pos = start_pos + end_pos + 2; // +2 for the closing }}
                let var = url[start_pos + 6..end_pos - 2].to_string(); // 6 is length of "{{env."
                let value = std::env::var(&var).unwrap_or_else(|_| {
                    debug!("Environment variable {} not found", var);
                    "".to_string()
                });
                url = url[..start_pos].to_string() + &value + &url[end_pos..];
                start = start_pos + value.len();
            } else {
                break;
            }
        }
        self.url = url;

        // Resolve environment variables in auth
        if let Some(auth) = &mut self.auth {
            if auth.username.starts_with("{{env.") && auth.username.ends_with("}}") {
                let var = auth
                    .username
                    .trim_matches(|c| c == '{' || c == '}')
                    .strip_prefix("env.")
                    .unwrap();
                auth.username = std::env::var(var).unwrap_or_else(|_| auth.username.clone());
            }
            if auth.password.starts_with("{{env.") && auth.password.ends_with("}}") {
                let var = auth
                    .password
                    .trim_matches(|c| c == '{' || c == '}')
                    .strip_prefix("env.")
                    .unwrap();
                auth.password = std::env::var(var).unwrap_or_else(|_| auth.password.clone());
            }
        }

        // Resolve environment variables in headers
        for (_, value) in self.headers.iter_mut() {
            if value.starts_with("{{env.") && value.ends_with("}}") {
                let var = value
                    .trim_matches(|c| c == '{' || c == '}')
                    .strip_prefix("env.")
                    .unwrap();
                *value = std::env::var(var).unwrap_or_else(|_| value.clone());
            }
        }

        // Resolve environment variables in expect conditions
        for expect in &mut self.expect {
            match expect {
                crate::types::Expect::Raw { contains } => {
                    if contains.starts_with("{{env.") && contains.ends_with("}}") {
                        let var = contains
                            .trim_matches(|c| c == '{' || c == '}')
                            .strip_prefix("env.")
                            .unwrap();
                        *contains = std::env::var(var).unwrap_or_else(|_| contains.clone());
                    }
                }
                crate::types::Expect::JsonPath { value, .. } => {
                    if value.starts_with("{{env.") && value.ends_with("}}") {
                        let var = value
                            .trim_matches(|c| c == '{' || c == '}')
                            .strip_prefix("env.")
                            .unwrap();
                        *value = std::env::var(var).unwrap_or_else(|_| value.clone());
                    }
                }
                crate::types::Expect::Status { .. } => {} // No environment variables in status codes
            }
        }
    }
} 