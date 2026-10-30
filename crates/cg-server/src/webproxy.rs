//! Web pages for the Iframe widget, fetched by the server.
//!
//! Most sites forbid being shown in another site's frame (`X-Frame-Options`,
//! CSP `frame-ancestors`), so an iframe pointing at them stays blank. The
//! page is fetched here and sent without those headers, with a `<base>` so
//! its pictures, styles and scripts still load from the site itself.
//!
//! The answer is sandboxed (CSP `sandbox`): the page runs in an origin of
//! its own and never with the rights of the Strata session, even opened on
//! its own. Only public addresses are fetched (no loopback, private or
//! link-local network), redirects included.

use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use axum::extract::Query;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Deserialize)]
pub struct ProxyQuery {
    url: String,
    /// The page is made transparent (its root and body backgrounds cleared).
    #[serde(default)]
    transparent: Option<String>,
}

/// `GET /api/web/proxy?url=…[&transparent=1]`
pub async fn proxy(Query(q): Query<ProxyQuery>) -> Response {
    let clear = q.transparent.as_deref().is_some_and(|v| v != "0");
    match tokio::task::spawn_blocking(move || fetch(&q.url, clear)).await {
        Ok(Ok(page)) => page.into_response(),
        Ok(Err(e)) => error_page(&e),
        Err(_) => error_page("the page could not be loaded"),
    }
}

struct Page {
    content_type: String,
    body: Vec<u8>,
}

impl IntoResponse for Page {
    fn into_response(self) -> Response {
        (
            [
                (header::CONTENT_TYPE, self.content_type),
                (header::CACHE_CONTROL, "no-store".to_string()),
                (header::CONTENT_SECURITY_POLICY, SANDBOX.to_string()),
            ],
            self.body,
        )
            .into_response()
    }
}

const SANDBOX: &str = "sandbox allow-scripts allow-forms allow-popups";

/// A small page in the frame saying why the site is not shown; it tells the
/// Iframe widget too (its Operator row shows the page did not load).
fn error_page(msg: &str) -> Response {
    let esc = msg
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let js = serde_json::to_string(msg)
        .unwrap_or_else(|_| "\"\"".into())
        .replace('<', "\\u003c");
    (
        StatusCode::BAD_GATEWAY,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, SANDBOX),
        ],
        format!(
            "<!doctype html><meta charset=utf-8><body style=\"margin:0;display:grid;place-items:center;height:100vh;\
             font:600 22px system-ui,sans-serif;color:#fff;background:rgba(20,22,34,.85)\"><p style=\"max-width:80%;text-align:center\">{esc}</p>\
             <script>parent.postMessage({{strataWeb:'error',msg:{js}}},'*')</script>"
        ),
    )
        .into_response()
}

/// True for an address on the public internet.
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // Carrier-grade NAT, 100.64.0.0/10.
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                || o[0] == 0)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique local fc00::/7, link-local fe80::/10.
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// Checks an address before it is fetched: http(s), a public host.
fn check(url: &str) -> Result<Uri, String> {
    let uri: Uri = url
        .trim()
        .parse()
        .map_err(|_| "this is not a web address".to_string())?;
    let https = match uri.scheme_str() {
        Some("https") => true,
        Some("http") => false,
        _ => return Err("the address must start with http:// or https://".into()),
    };
    let host = uri
        .host()
        .ok_or("the address has no host")?
        .trim_matches(['[', ']']);
    let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
    let addrs: Vec<IpAddr> = match (host, port).to_socket_addrs() {
        Ok(a) => a.map(|a| a.ip()).collect(),
        // Behind a web proxy the server may not resolve names itself: the
        // proxy does (an IP address or a local name is still refused).
        Err(_) if behind_proxy() && host.parse::<IpAddr>().is_err() && !local_name(host) => {
            return Ok(uri)
        }
        Err(_) => return Err(format!("{host} cannot be found")),
    };
    if addrs.is_empty() || !addrs.iter().all(|ip| public(*ip)) {
        return Err(format!(
            "{host} is on the local network: only public sites go through the server"
        ));
    }
    Ok(uri)
}

fn behind_proxy() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
}

fn local_name(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".internal")
        || !h.contains('.')
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(15)))
        // Each hop is checked here.
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0 Safari/537.36 Strata",
        )
        .build()
        .into()
}

fn fetch(url: &str, clear: bool) -> Result<Page, String> {
    let agent = agent();
    let mut uri = check(url)?;
    for _ in 0..=MAX_REDIRECTS {
        let res = agent
            .get(uri.to_string())
            .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
            .header("Accept-Language", "fr,en;q=0.8")
            .call()
            .map_err(|e| format!("{} cannot be reached: {e}", uri.host().unwrap_or("")))?;
        let status = res.status();
        if status.is_redirection() {
            let to = res
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("the site redirects nowhere")?;
            uri = check(&join(&uri, to))?;
            continue;
        }
        if !status.is_success() {
            return Err(format!("the site answered {status}"));
        }
        let content_type = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();
        let mut body = Vec::new();
        res.into_body()
            .into_reader()
            .take(MAX_BYTES)
            .read_to_end(&mut body)
            .map_err(|e| format!("the page could not be read: {e}"))?;
        if content_type.contains("html") {
            body = with_base(&body, &uri.to_string(), clear);
        }
        return Ok(Page { content_type, body });
    }
    Err("the site redirects too many times".into())
}

/// A redirect target, absolute or relative to `base`.
fn join(base: &Uri, to: &str) -> String {
    if to.starts_with("http://") || to.starts_with("https://") {
        return to.to_string();
    }
    let origin = format!(
        "{}://{}",
        base.scheme_str().unwrap_or("https"),
        base.authority().map(|a| a.as_str()).unwrap_or("")
    );
    if let Some(rest) = to.strip_prefix("//") {
        return format!("{}://{rest}", base.scheme_str().unwrap_or("https"));
    }
    if to.starts_with('/') {
        return format!("{origin}{to}");
    }
    let path = base.path();
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    format!("{origin}{}{to}", if dir.is_empty() { "/" } else { dir })
}

/// The page with `<base href>` at the start of its head, so that its
/// relative addresses point at the site, not at Strata; `clear`: a style
/// making the page transparent (the Iframe's « Fond » set to transparent).
fn with_base(html: &[u8], url: &str, clear: bool) -> Vec<u8> {
    let mut tag = format!(
        "<base href=\"{}\">",
        url.replace('&', "&amp;").replace('"', "&quot;")
    );
    if clear {
        tag.push_str("<style>html,body{background:transparent!important}</style>");
    }
    let lower: Vec<u8> = html
        .iter()
        .take(64 * 1024)
        .map(u8::to_ascii_lowercase)
        .collect();
    let find = |needle: &[u8]| lower.windows(needle.len()).position(|w| w == needle);
    // After `<head …>`, else after `<html …>`, else first.
    let at = find(b"<head")
        .or_else(|| find(b"<html"))
        .and_then(|i| {
            lower[i..]
                .iter()
                .position(|&c| c == b'>')
                .map(|j| i + j + 1)
        })
        .unwrap_or(0);
    let mut out = Vec::with_capacity(html.len() + tag.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&html[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_are_refused() {
        for url in [
            "http://127.0.0.1:8080/api/projects",
            "http://localhost/",
            "http://10.1.2.3/",
            "http://192.168.1.10/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(check(url).is_err(), "{url} should be refused");
        }
        assert!(public("93.184.215.14".parse().unwrap()));
        assert!(!public("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!public("100.100.1.1".parse().unwrap()));
    }

    #[test]
    fn the_page_gets_a_base() {
        let out = with_base(
            b"<!doctype html><HTML><Head lang=x><title>a</title>",
            "https://e.com/a/b?x=1&y=2",
            false,
        );
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<!doctype html><HTML><Head lang=x><base href=\"https://e.com/a/b?x=1&amp;y=2\"><title>a</title>"
        );
        assert!(
            String::from_utf8(with_base(b"<p>hi", "https://e.com/", false))
                .unwrap()
                .starts_with("<base href")
        );
        // Transparent: the page's backgrounds are cleared, after the base.
        let out = String::from_utf8(with_base(b"<head><title>a</title>", "https://e.com/", true))
            .unwrap();
        assert!(out.starts_with("<head><base href=\"https://e.com/\"><style>html,body{background:transparent!important}</style><title>"));
    }

    #[test]
    fn redirects_are_resolved() {
        let base: Uri = "https://e.com/news/today.html".parse().unwrap();
        assert_eq!(join(&base, "https://x.org/"), "https://x.org/");
        assert_eq!(join(&base, "//cdn.e.com/p"), "https://cdn.e.com/p");
        assert_eq!(join(&base, "/fr/"), "https://e.com/fr/");
        assert_eq!(
            join(&base, "tomorrow.html"),
            "https://e.com/news/tomorrow.html"
        );
    }
}
