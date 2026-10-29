use serde::{Deserialize, Serialize};

pub const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kv {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
}

fn default_true() -> bool {
    true
}

impl Kv {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
            description: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiKeyIn {
    Header,
    Query,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Auth {
    NoAuth,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyIn,
    },
}

impl Default for Auth {
    fn default() -> Self {
        Auth::NoAuth
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyMode {
    None,
    Raw,
    UrlEncoded,
}

impl Default for BodyMode {
    fn default() -> Self {
        BodyMode::None
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReqSettings {
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    #[serde(default = "default_true")]
    pub cookie_jar: bool,
}

fn default_timeout() -> u64 {
    30
}
fn default_max_redirects() -> usize {
    10
}

impl Default for ReqSettings {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout(),
            follow_redirects: default_true(),
            max_redirects: default_max_redirects(),
            verify_tls: default_true(),
            cookie_jar: default_true(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestDef {
    pub name: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub params: Vec<Kv>,
    #[serde(default)]
    pub headers: Vec<Kv>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub body_mode: BodyMode,
    #[serde(default)]
    pub form: Vec<Kv>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub settings: ReqSettings,
    #[serde(default)]
    pub comment: String,
}

impl RequestDef {
    pub fn method_index(&self) -> usize {
        METHODS
            .iter()
            .position(|m| *m == self.method)
            .unwrap_or(0)
    }

    pub fn set_method(&mut self, idx: usize) {
        self.method = METHODS[idx.min(METHODS.len() - 1)].to_string();
    }

    pub fn is_bodyless(&self) -> bool {
        matches!(self.method.as_str(), "GET" | "HEAD" | "TRACE" | "")
    }

    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            let host = self
                .url
                .split('/')
                .nth(2)
                .unwrap_or(self.url.as_str())
                .to_string();
            if host.is_empty() {
                "Untitled request".to_string()
            } else {
                format!("{} · {}", self.method, host)
            }
        } else {
            self.name.clone()
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResponseBundle {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<(String, String)>,
    pub body: String,
    pub body_bytes: usize,
    pub content_type: Option<String>,
    pub elapsed_ms: u128,
    pub final_url: String,
    pub error: Option<String>,
}

impl ResponseBundle {
    pub fn failure(err: String, elapsed_ms: u128) -> Self {
        Self {
            status: 0,
            status_text: "Error".to_string(),
            headers: Vec::new(),
            cookies: Vec::new(),
            body: String::new(),
            body_bytes: 0,
            content_type: None,
            elapsed_ms,
            final_url: String::new(),
            error: Some(err),
        }
    }
}

/// Joins the URL's own query string with the enabled params grid, keeping one
/// source of truth at send time instead of syncing the two editors.
pub fn compose_url(url: &str, params: &[Kv]) -> String {
    let trimmed = url.trim();
    let (base, existing) = match trimmed.split_once('?') {
        Some((b, q)) => (b, q),
        None => (trimmed, ""),
    };
    let mut pairs: Vec<(String, String)> = Vec::new();
    for seg in existing.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
        pairs.push((k.to_string(), v.to_string()));
    }
    for p in params.iter().filter(|p| p.enabled && !p.key.trim().is_empty()) {
        if pairs.iter().any(|(k, _)| k == p.key.trim()) {
            continue;
        }
        pairs.push((p.key.trim().to_string(), p.value.clone()));
    }
    if pairs.is_empty() {
        return base.to_string();
    }
    let qs = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{qs}")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn pretty_body(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string()),
        Err(_) => body.to_string(),
    }
}

pub fn substitute(input: &str, vars: &std::collections::BTreeMap<String, String>) -> String {
    if vars.is_empty() || !input.contains("{{") {
        return input.to_string();
    }
    let mut out = String::new();
    let mut rest = input;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        match after.find("}}") {
            Some(close) => {
                let name = after[..close].trim();
                match vars.get(name) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&format!("{{{{{name}}}}}")),
                }
                rest = &after[close + 2..];
            }
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}
