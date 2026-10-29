use std::path::PathBuf;

use crate::collections::Collection;
use crate::model::{Kv, RequestDef};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Environment {
    pub name: String,
    pub variables: Vec<Kv>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HistoryRecord {
    pub at: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub elapsed_ms: u128,
    pub body_bytes: usize,
    pub request: RequestDef,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    pub proxy_url: String,
    pub default_timeout_secs: u64,
    #[serde(default)]
    pub theme: crate::theme::Theme,
    pub pretty_by_default: bool,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            proxy_url: String::new(),
            default_timeout_secs: 30,
            theme: crate::theme::Theme::default(),
            pretty_by_default: true,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub collections: Vec<Collection>,
    pub environments: Vec<Environment>,
    pub active_environment: Option<String>,
    pub history: Vec<HistoryRecord>,
    pub settings: GlobalSettings,
}

impl Default for Workspace {
    fn default() -> Self {
        let sample = Collection {
            id: crate::collections::uuid(),
            name: "Starter".to_string(),
            description: "Built-in sample requests".to_string(),
            variables: vec![Kv::new("base_url", "https://httpbin.org")],
            requests: vec![RequestDef {
                name: "GET echo".to_string(),
                method: "GET".to_string(),
                url: "{{base_url}}/get".to_string(),
                headers: vec![Kv::new("Accept", "application/json")],
                ..Default::default()
            }],
            folders: vec![crate::collections::Folder {
                name: "Write".to_string(),
                requests: vec![RequestDef {
                    name: "POST json".to_string(),
                    method: "POST".to_string(),
                    url: "{{base_url}}/post".to_string(),
                    body_mode: crate::model::BodyMode::Raw,
                    body: "{\n  \"hello\": \"world\"\n}".to_string(),
                    headers: vec![Kv::new("Content-Type", "application/json")],
                    ..Default::default()
                }],
            }],
        };
        Self {
            collections: vec![sample],
            environments: vec![Environment {
                name: "dev".to_string(),
                variables: vec![Kv::new("base_url", "https://httpbin.org")],
            }],
            active_environment: Some("dev".to_string()),
            history: Vec::new(),
            settings: GlobalSettings::default(),
        }
    }
}

impl Workspace {
    pub fn vars(&self) -> std::collections::BTreeMap<String, String> {
        let mut map = std::collections::BTreeMap::new();
        for col in &self.collections {
            for v in &col.variables {
                if v.enabled && !v.key.trim().is_empty() {
                    map.insert(v.key.trim().to_string(), v.value.clone());
                }
            }
        }
        if let Some(name) = &self.active_environment {
            if let Some(env) = self.environments.iter().find(|e| &e.name == name) {
                for v in &env.variables {
                    if v.enabled && !v.key.trim().is_empty() {
                        map.insert(v.key.trim().to_string(), v.value.clone());
                    }
                }
            }
        }
        map
    }

    pub fn unresolved(&self, text: &str) -> Vec<String> {
        let vars = self.vars();
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find("{{") {
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else { break };
            let name = after[..close].trim();
            if !vars.contains_key(name) {
                out.push(name.to_string());
            }
            rest = &after[close + 2..];
        }
        out
    }
}

pub fn config_path() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".config"))
            .unwrap_or_else(|_| PathBuf::from("."))
    };
    base.join("TechvisionApiChecker")
}

fn legacy_state_file() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".config"))
            .unwrap_or_else(|_| PathBuf::from("."))
    };
    base.join("ApiChecker").join("workspace.json")
}

pub fn state_file() -> PathBuf {
    config_path().join("workspace.json")
}

pub fn load() -> Workspace {
    for path in [state_file(), legacy_state_file()] {
        if let Ok(text) = std::fs::read_to_string(&path) {
            return serde_json::from_str::<Workspace>(&text).unwrap_or_default();
        }
    }
    Workspace::default()
}

pub fn save(ws: &Workspace) -> Result<usize, String> {
    let path = state_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(ws).map_err(|e| e.to_string())?;
    std::fs::write(&path, &text).map_err(|e| e.to_string())?;
    Ok(text.len())
}
