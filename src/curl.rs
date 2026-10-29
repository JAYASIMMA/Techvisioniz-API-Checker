use std::collections::BTreeMap;

use crate::model::{compose_url, substitute, ApiKeyIn, Auth, BodyMode, RequestDef};

fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn dq(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Flattens auth + headers + params into the concrete wire form, after `{{var}}`
/// substitution, so generated snippets match what is actually sent.
fn resolved(req: &RequestDef, vars: &BTreeMap<String, String>) -> (String, Vec<(String, String)>, Option<String>) {
    let mut url = compose_url(&substitute(&req.url, vars), &req.params);
    let mut headers = Vec::new();
    for h in &req.headers {
        if h.enabled && !h.key.trim().is_empty() {
            headers.push((h.key.trim().to_string(), substitute(&h.value, vars)));
        }
    }
    match &req.auth {
        Auth::NoAuth => {}
        Auth::Bearer { token } => {
            let t = substitute(token, vars);
            if !t.trim().is_empty() {
                headers.push(("Authorization".into(), format!("Bearer {}", t.trim())));
            }
        }
        Auth::Basic { username, password } => {
            let u = substitute(username, vars);
            let p = substitute(password, vars);
            headers.push(("Authorization".into(), format!("Basic {u}:{p} (base64)")));
        }
        Auth::ApiKey { key, value, location } => {
            let k = substitute(key, vars);
            let v = substitute(value, vars);
            if !k.trim().is_empty() && !v.trim().is_empty() {
                match location {
                    ApiKeyIn::Header => headers.push((k.trim().to_string(), v.trim().to_string())),
                    ApiKeyIn::Query => {
                        url = if url.contains('?') {
                            format!("{url}&{k}={v}")
                        } else {
                            format!("{url}?{k}={v}")
                        };
                    }
                }
            }
        }
    }

    let body = match req.body_mode {
        BodyMode::None => None,
        BodyMode::Raw if !req.is_bodyless() && !req.body.trim().is_empty() => {
            if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                headers.push(("Content-Type".into(), "application/json".into()));
            }
            Some(substitute(&req.body, vars))
        }
        BodyMode::UrlEncoded if !req.is_bodyless() => {
            let pairs: Vec<String> = req
                .form
                .iter()
                .filter(|f| f.enabled && !f.key.trim().is_empty())
                .map(|f| format!("{}={}", f.key.trim(), substitute(&f.value, vars)))
                .collect();
            if pairs.is_empty() {
                None
            } else {
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                    headers.push(("Content-Type".into(), "application/x-www-form-urlencoded".into()));
                }
                Some(pairs.join("&"))
            }
        }
        _ => None,
    };
    (url, headers, body)
}

pub fn curl_bash(req: &RequestDef, vars: &BTreeMap<String, String>) -> String {
    let (url, headers, body) = resolved(req, vars);
    let mut out = String::from("curl");
    if req.method != "GET" {
        out.push_str(&format!(" -X {}", req.method));
    }
    for (k, v) in &headers {
        out.push_str(&format!(" \\\n  -H {}:{}\n", sq(k), sq(v)));
    }
    if let Some(b) = &body {
        out.push_str(&format!(" \\\n  --data-raw {}", sq(b)));
    }
    out.push_str(&format!(" \\\n  {}", sq(&url)));
    out
}

pub fn curl_windows(req: &RequestDef, vars: &BTreeMap<String, String>) -> String {
    let (url, headers, body) = resolved(req, vars);
    let mut out = String::from("curl");
    if req.method != "GET" {
        out.push_str(&format!(" -X {}", req.method));
    }
    for (k, v) in &headers {
        out.push_str(&format!(" \\\n  -H {}:{}", dq(k), dq(v)));
    }
    if let Some(b) = &body {
        out.push_str(&format!(" \\\n  --data-raw {}", dq(b)));
    }
    out.push_str(&format!(" \\\n  {}", dq(&url)));
    out
}

pub fn powershell(req: &RequestDef, vars: &BTreeMap<String, String>) -> String {
    let (url, headers, body) = resolved(req, vars);
    let mut out = String::from("$headers = @{\n");
    for (k, v) in &headers {
        out.push_str(&format!("  {} = {}\n", dq(k), dq(v)));
    }
    out.push_str("}\n\n");
    out.push_str(&format!(
        "Invoke-RestMethod -Method {} -Uri {} -Headers $headers",
        req.method,
        dq(&url)
    ));
    if let Some(b) = &body {
        out.push_str(&format!(" -Body {}", dq(b)));
    }
    out
}

pub fn python(req: &RequestDef, vars: &BTreeMap<String, String>) -> String {
    let (url, headers, body) = resolved(req, vars);
    let mut out = String::from("import requests\n\nurl = ");
    out.push_str(&dq(&url));
    out.push_str("\nheaders = {\n");
    for (k, v) in &headers {
        out.push_str(&format!("    {}: {},\n", dq(k), dq(v)));
    }
    out.push_str("}\n");
    if let Some(b) = &body {
        out.push_str(&format!("\npayload = {}\n", dq(b)));
        out.push_str(&format!(
            "\nres = requests.{}(url, headers=headers, data=payload, timeout={})\nprint(res.status_code, res.text)",
            req.method.to_lowercase(),
            req.settings.timeout_secs
        ));
    } else {
        out.push_str(&format!(
            "\nres = requests.{}(url, headers=headers, timeout={})\nprint(res.status_code, res.text)",
            req.method.to_lowercase(),
            req.settings.timeout_secs
        ));
    }
    out
}

pub fn js_fetch(req: &RequestDef, vars: &BTreeMap<String, String>) -> String {
    let (url, headers, body) = resolved(req, vars);
    let mut out = String::from("const res = await fetch(");
    out.push_str(&format!("\"{url}\", {{\n  method: \"{}\",\n", req.method));
    out.push_str("  headers: {\n");
    for (k, v) in &headers {
        out.push_str(&format!("    \"{k}\": \"{v}\",\n"));
    }
    out.push_str("  },\n");
    if let Some(b) = &body {
        out.push_str(&format!("  body: {:?},\n", b));
    }
    out.push_str("});\nconsole.log(res.status, await res.text());");
    out
}
