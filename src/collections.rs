use serde_json::{json, Value};

use crate::model::{ApiKeyIn, Auth, BodyMode, Kv, RequestDef};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Folder {
    pub name: String,
    pub requests: Vec<RequestDef>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<Kv>,
    pub requests: Vec<RequestDef>,
    pub folders: Vec<Folder>,
}

impl Collection {
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn total_requests(&self) -> usize {
        self.requests.len() + self.folders.iter().map(|f| f.requests.len()).sum::<usize>()
    }

    pub fn folder_names(&self) -> Vec<String> {
        let mut v = vec!["<root>".to_string()];
        v.extend(self.folders.iter().map(|f| f.name.clone()));
        v
    }

    pub fn add_request(&mut self, folder: &str, req: RequestDef) {
        if folder == "<root>" {
            self.requests.push(req);
        } else if let Some(f) = self.folders.iter_mut().find(|f| f.name == folder) {
            f.requests.push(req);
        } else {
            self.requests.push(req);
        }
    }
}

pub fn uuid() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut bytes = [0u8; 16];
    let mut seed = now as u64;
    for b in bytes.iter_mut() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (seed >> 33) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn kv_to_postman(k: &Kv) -> Value {
    json!({
        "key": k.key,
        "value": k.value,
        "disabled": !k.enabled,
        "description": k.description,
    })
}

fn request_to_postman(req: &RequestDef) -> Value {
    let mut url_obj = json!({ "raw": req.url });
    let (base, query_str) = match req.url.split_once('?') {
        Some((b, q)) => (b, q),
        None => (req.url.as_str(), ""),
    };
    if let Some(after_scheme) = base.split_once("//").map(|(_, r)| r) {
        let (host, path) = after_scheme.split_once('/').unwrap_or((after_scheme, ""));
        url_obj["host"] = json!(host.split('.').collect::<Vec<_>>());
        url_obj["path"] = json!(path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>());
    }
    let mut query: Vec<Kv> = req.params.clone();
    for seg in query_str.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
        if !query.iter().any(|q| q.key == k) {
            query.push(Kv::new(k, v));
        }
    }
    if !query.is_empty() {
        url_obj["query"] = json!(query.iter().map(kv_to_postman).collect::<Vec<_>>());
    }
    let body = match req.body_mode {
        BodyMode::None => json!(null),
        BodyMode::Raw => json!({ "mode": "raw", "raw": req.body, "options": { "raw": { "language": "json" } } }),
        BodyMode::UrlEncoded => json!({
            "mode": "urlencoded",
            "urlencoded": req.form.iter().map(kv_to_postman).collect::<Vec<_>>()
        }),
    };
    json!({
        "name": req.title(),
        "request": {
            "method": req.method,
            "header": req.headers.iter().map(kv_to_postman).collect::<Vec<_>>(),
            "body": body,
            "url": url_obj,
            "description": req.comment,
            "auth": auth_to_postman(&req.auth),
        }
    })
}

fn auth_to_postman(auth: &Auth) -> Value {
    match auth {
        Auth::NoAuth => json!({ "type": "noauth" }),
        Auth::Bearer { token } => json!({ "type": "bearer", "bearer": [{ "key": "token", "value": token, "type": "string" }] }),
        Auth::Basic { username, password } => json!({
            "type": "basic",
            "basic": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" }
            ]
        }),
        Auth::ApiKey { key, value, location } => json!({
            "type": "apikey",
            "apikey": [
                { "key": "key", "value": key, "type": "string" },
                { "key": "value", "value": value, "type": "string" },
                { "key": "in", "value": match location { ApiKeyIn::Header => "header", ApiKeyIn::Query => "query" }, "type": "string" }
            ]
        }),
    }
}

pub fn to_postman_json(col: &Collection) -> Value {
    let mut items = Vec::new();
    for req in &col.requests {
        items.push(request_to_postman(req));
    }
    for folder in &col.folders {
        items.push(json!({
            "name": folder.name,
            "item": folder.requests.iter().map(request_to_postman).collect::<Vec<_>>(),
        }));
    }
    json!({
        "info": {
            "_postman_id": col.id,
            "name": col.name,
            "description": col.description,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
        },
        "item": items,
        "variable": col.variables.iter().map(|v| json!({
            "key": v.key, "value": v.value, "description": v.description
        })).collect::<Vec<_>>(),
    })
}

fn str_field(obj: &Value, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn kvs_from_postman(value: Option<&Value>) -> Vec<Kv> {
    let mut out = Vec::new();
    if let Some(Value::Array(arr)) = value {
        for item in arr {
            out.push(Kv {
                key: str_field(item, "key"),
                value: str_field(item, "value"),
                enabled: !item.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
                description: str_field(item, "description"),
            });
        }
    }
    out
}

fn auth_from_postman(value: &Value) -> Auth {
    let ty = value.get("type").and_then(|v| v.as_str()).unwrap_or("noauth");
    let field = |name: &str| -> String {
        value
            .get(ty)
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find(|o| o.get("key").and_then(|k| k.as_str()) == Some(name))
            })
            .and_then(|o| o.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    match ty {
        "bearer" => Auth::Bearer { token: field("token") },
        "basic" => Auth::Basic {
            username: field("username"),
            password: field("password"),
        },
        "apikey" => Auth::ApiKey {
            key: field("key"),
            value: field("value"),
            location: if field("in") == "query" {
                ApiKeyIn::Query
            } else {
                ApiKeyIn::Header
            },
        },
        _ => Auth::NoAuth,
    }
}

pub fn request_from_postman(item: &Value) -> RequestDef {
    let name = str_field(item, "name");
    let req = item.get("request").cloned().unwrap_or(json!({}));
    let method = {
        let m = str_field(&req, "method").to_uppercase();
        if m.is_empty() { "GET".to_string() } else { m }
    };
    let url_value = req.get("url").cloned().unwrap_or(json!(""));
    let url = match &url_value {
        Value::String(s) => s.clone(),
        Value::Object(_) => str_field(&url_value, "raw"),
        _ => String::new(),
    };
    let params = match &url_value {
        Value::Object(_) => kvs_from_postman(url_value.get("query")),
        Value::String(s) => s
            .split_once('?')
            .map(|(_, q)| {
                q.split('&')
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        let (k, v) = p.split_once('=').unwrap_or((p, ""));
                        Kv::new(k, v)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let body = req.get("body").cloned().unwrap_or(json!({}));
    let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("raw");
    let (body_mode, body_text, form) = match mode {
        "urlencoded" => (BodyMode::UrlEncoded, String::new(), kvs_from_postman(body.get("urlencoded"))),
        "raw" => (BodyMode::Raw, str_field(&body, "raw"), Vec::new()),
        "file" => (BodyMode::None, String::new(), Vec::new()),
        _ => (BodyMode::None, String::new(), Vec::new()),
    };
    RequestDef {
        name,
        method,
        url,
        params,
        headers: kvs_from_postman(req.get("header")),
        body: body_text,
        body_mode,
        form,
        auth: auth_from_postman(&req.get("auth").cloned().unwrap_or(json!({}))),
        settings: Default::default(),
        comment: str_field(&req, "description"),
    }
}

pub fn from_postman_json(value: &Value) -> Collection {
    let info = value.get("info").cloned().unwrap_or(json!({}));
    let mut col = Collection {
        id: str_field(&info, "_postman_id"),
        name: str_field(&info, "name"),
        description: str_field(&info, "description"),
        variables: kvs_from_postman(value.get("variable")),
        requests: Vec::new(),
        folders: Vec::new(),
    };
    if col.id.is_empty() {
        col.id = uuid();
    }
    if col.name.is_empty() {
        col.name = "Imported collection".to_string();
    }
    if let Some(Value::Array(items)) = value.get("item") {
        walk_items(items, &mut col.requests, &mut col.folders);
    }
    col
}

fn walk_items(items: &[Value], requests: &mut Vec<RequestDef>, folders: &mut Vec<Folder>) {
    for item in items {
        if let Some(Value::Array(children)) = item.get("item") {
            let mut folder = Folder {
                name: str_field(item, "name"),
                requests: Vec::new(),
            };
            let mut nested_requests = Vec::new();
            let mut nested_folders = Vec::new();
            walk_items(children, &mut nested_requests, &mut nested_folders);
            folder.requests.extend(nested_requests);
            folders.push(folder);
            folders.extend(nested_folders);
        } else if item.get("request").is_some() {
            requests.push(request_from_postman(item));
        }
    }
}
