//! Calls to the platforms' web APIs (blocking ureq, run off the async
//! threads).

use std::time::Duration;

use serde_json::Value;

/// A platform's answer.
pub struct Answer {
    pub status: u16,
    pub body: Value,
    /// `x-rate-limit-reset` (X): when the quota comes back (unix s).
    pub reset: Option<i64>,
    /// `x-rate-limit-remaining` (X).
    pub remaining: Option<i64>,
}

impl Answer {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The platform's own error message, else the status.
    pub fn error(&self) -> String {
        let b = &self.body;
        let msg = b["error"]["message"]
            .as_str()
            .or_else(|| b["detail"].as_str())
            .or_else(|| b["title"].as_str())
            .or_else(|| b["errors"][0]["message"].as_str());
        match msg {
            Some(m) => format!("{m} ({})", self.status),
            None => format!("the platform answered {}", self.status),
        }
    }
}

pub(super) fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(5)
        .http_status_as_error(false)
        .user_agent(concat!("strata-graphics/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn header_i64(res: &ureq::http::Response<ureq::Body>, name: &str) -> Option<i64> {
    res.headers().get(name)?.to_str().ok()?.trim().parse().ok()
}

/// GET a JSON document, with a bearer token when given.
pub async fn get_json(url: String, bearer: Option<String>) -> Result<Answer, String> {
    tokio::task::spawn_blocking(move || {
        let mut req = agent().get(&url);
        if let Some(b) = &bearer {
            req = req.header("Authorization", &format!("Bearer {b}"));
        }
        let mut res = req
            .call()
            .map_err(|e| format!("cannot reach the platform: {e}"))?;
        let status = res.status().as_u16();
        let reset = header_i64(&res, "x-rate-limit-reset");
        let remaining = header_i64(&res, "x-rate-limit-remaining");
        let bytes = res
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| format!("cannot read the answer: {e}"))?;
        Ok(Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            reset,
            remaining,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// GET a file (a picture), with a bearer token or a user and password.
pub async fn get_bytes(
    url: String,
    bearer: Option<String>,
    basic: Option<(String, String)>,
) -> Result<(Vec<u8>, String), String> {
    tokio::task::spawn_blocking(move || {
        let mut req = agent().get(&url);
        if let Some(b) = &bearer {
            req = req.header("Authorization", &format!("Bearer {b}"));
        }
        if let Some((u, p)) = &basic {
            use base64::Engine;
            let auth = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
            req = req.header("Authorization", &format!("Basic {auth}"));
        }
        let mut res = req.call().map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("the platform answered {}", res.status()));
        }
        let kind = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = res
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        Ok((bytes, kind))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Percent-encodes a query parameter.
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Decodes `application/x-www-form-urlencoded` (Twilio's webhooks).
pub fn form(body: &str) -> Vec<(String, String)> {
    let dec = |s: &str| {
        let s = s.replace('+', " ");
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(b[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    body.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (dec(k), dec(v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_bodies_are_decoded() {
        let f = form("From=%2B33612344107&Body=Bonjour+%C3%A0+tous+%21&X=%");
        assert_eq!(f[0], ("From".into(), "+33612344107".into()));
        assert_eq!(f[1].1, "Bonjour à tous !");
        assert_eq!(f[2].1, "%");
        assert_eq!(enc("#JT20H -is:retweet"), "%23JT20H%20-is%3Aretweet");
    }
}
