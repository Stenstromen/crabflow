use serde::Deserialize;
use serde::de::Deserializer;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Expect {
    Status { code: u16 },
    JsonPath { path: String, value: String },
    Raw { contains: String },
}

pub fn deserialize_expect<'de, D>(deserializer: D) -> Result<Vec<Expect>, D::Error>
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
pub struct Workflow {
    pub name: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum BodyType {
    FormUrlencoded,
    Json,
    Raw,
    FormMultipart,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Task {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_yaml::Value>,
    #[serde(default)]
    pub body_type: Option<BodyType>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default = "default_retries")]
    pub retries: u32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,
    #[serde(default, deserialize_with = "deserialize_expect")]
    pub expect: Vec<Expect>,
    #[serde(default)]
    pub register: Option<String>,
    #[serde(default)]
    pub auth: Option<BasicAuth>,
    #[serde(default)]
    pub save_as: Option<String>,
    #[serde(default)]
    pub variables: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

pub fn default_retries() -> u32 {
    1
}

pub fn default_retry_delay() -> u64 {
    5
}

#[derive(Clone)]
pub struct RegisteredResponse {
    pub json: Value,
    #[allow(dead_code)]
    pub text: String,
}
