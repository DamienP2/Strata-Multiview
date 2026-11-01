//! The security of the server: HTTPS for the web interface (a certificate
//! given, or one made for the machine on first use), signing in with the
//! company's directory (LDAP / Active Directory, or an OpenID Connect
//! provider: Azure AD / Entra ID, Okta, Keycloak, Google…), and the audit
//! journal of what was changed, by whom.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::auth::Role;

// ------------------------------------------------------------------ HTTPS --

/// HTTPS of the web interface.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TlsConfig {
    pub enabled: bool,
    /// PEM certificate (chain) and private key; both empty: a certificate
    /// made for this machine (`<data>/tls/`), which browsers ask to trust.
    pub cert: String,
    pub key: String,
}

/// The certificate and key to serve: the ones given, else this machine's
/// own (made once).
pub fn tls_files(data_dir: &Path, c: &TlsConfig) -> anyhow::Result<(PathBuf, PathBuf)> {
    if !c.cert.trim().is_empty() && !c.key.trim().is_empty() {
        return Ok((PathBuf::from(c.cert.trim()), PathBuf::from(c.key.trim())));
    }
    let dir = data_dir.join("tls");
    let (cert, key) = (dir.join("cert.pem"), dir.join("key.pem"));
    if cert.exists() && key.exists() {
        return Ok((cert, key));
    }
    std::fs::create_dir_all(&dir)?;
    let mut names = vec!["localhost".to_string()];
    if let Ok(h) = gethostname::gethostname().into_string() {
        names.push(h.clone());
        names.push(format!("{h}.local"));
    }
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        names.extend(ifs.iter().map(|i| i.ip().to_string()));
    }
    names.sort();
    names.dedup();
    let rcgen::CertifiedKey {
        cert: c,
        signing_key,
    } = rcgen::generate_simple_self_signed(names)?;
    std::fs::write(&cert, c.pem())?;
    std::fs::write(&key, signing_key.serialize_pem())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!(
        "HTTPS: made a certificate for this machine ({})",
        cert.display()
    );
    Ok((cert, key))
}

/// The rustls settings of the HTTPS listener.
pub fn rustls_config(data_dir: &Path, c: &TlsConfig) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    // One crypto provider for the process (ring: no C toolchain needed).
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (cert, key) = tls_files(data_dir, c)?;
    let chain = CertificateDer::pem_file_iter(&cert)
        .with_context(|| format!("{}", cert.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("{}", cert.display()))?;
    let key = PrivateKeyDer::from_pem_file(&key).with_context(|| format!("{}", key.display()))?;
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Serves `app` on `listener`, HTTPS and HTTP on the same port: a TLS handshake
/// is served over HTTPS; plain HTTP from the machine itself (the menu bar app,
/// scripts on the server) is served as is, and from elsewhere is sent to the
/// same address over HTTPS. Ends when `shutdown` does, giving the open
/// connections 3 s to finish.
pub async fn serve_https(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    tls: Arc<rustls::ServerConfig>,
    shutdown: impl std::future::Future<Output = ()>,
) {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder;
    use hyper_util::service::TowerToHyperService;
    use tower::ServiceExt;

    let acceptor = tokio_rustls::TlsAcceptor::from(tls);
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();
    tokio::pin!(shutdown);
    loop {
        let (tcp, peer) = tokio::select! {
            r = listener.accept() => match r {
                Ok(c) => c,
                Err(e) => {
                    // Out of file descriptors and the like: wait, do not spin.
                    tracing::warn!("accept: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            _ = &mut shutdown => break,
        };
        let _ = tcp.set_nodelay(true);
        let acceptor = acceptor.clone();
        let app = app.clone();
        let watcher = graceful.watcher();
        tokio::spawn(async move {
            let mut first = [0u8; 1];
            let tls =
                match tokio::time::timeout(Duration::from_secs(10), tcp.peek(&mut first)).await {
                    Ok(Ok(1)) => first[0] == 0x16, // a TLS handshake record
                    _ => return,
                };
            let local = peer.ip().is_loopback()
                || matches!(peer.ip(), std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()));
            let app = app.map_request(move |mut req: axum::extract::Request| {
                req.extensions_mut()
                    .insert(axum::extract::ConnectInfo(peer));
                req
            });
            let builder = Builder::new(TokioExecutor::new());
            if tls {
                let Ok(Ok(stream)) =
                    tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await
                else {
                    return;
                };
                let svc = TowerToHyperService::new(app.map_request(
                    |r: hyper::Request<hyper::body::Incoming>| r.map(axum::body::Body::new),
                ));
                let _ = watcher
                    .watch(
                        builder
                            .serve_connection_with_upgrades(TokioIo::new(stream), svc)
                            .into_owned(),
                    )
                    .await;
            } else if local {
                let svc = TowerToHyperService::new(app.map_request(
                    |r: hyper::Request<hyper::body::Incoming>| r.map(axum::body::Body::new),
                ));
                let _ = watcher
                    .watch(
                        builder
                            .serve_connection_with_upgrades(TokioIo::new(tcp), svc)
                            .into_owned(),
                    )
                    .await;
            } else {
                let svc = hyper::service::service_fn(
                    |req: hyper::Request<hyper::body::Incoming>| async move {
                        let host = req
                            .headers()
                            .get(hyper::header::HOST)
                            .and_then(|h| h.to_str().ok())
                            .unwrap_or("localhost")
                            .to_string();
                        let path = req
                            .uri()
                            .path_and_query()
                            .map(|p| p.as_str())
                            .unwrap_or("/")
                            .to_string();
                        hyper::Response::builder()
                            .status(hyper::StatusCode::PERMANENT_REDIRECT)
                            .header(hyper::header::LOCATION, format!("https://{host}{path}"))
                            .body(axum::body::Body::empty())
                    },
                );
                let _ = watcher
                    .watch(
                        builder
                            .serve_connection(TokioIo::new(tcp), svc)
                            .into_owned(),
                    )
                    .await;
            }
        });
    }
    drop(listener);
    if tokio::time::timeout(Duration::from_secs(3), graceful.shutdown())
        .await
        .is_err()
    {
        tracing::debug!("HTTPS: connections still open after 3 s, closed");
    }
}

// ------------------------------------------------------------------- LDAP --

/// Signing in with LDAP / Active Directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LdapConfig {
    pub enabled: bool,
    /// ldap://dc.example.com, ldaps://dc.example.com:636.
    pub url: String,
    /// StartTLS on ldap://.
    pub starttls: bool,
    /// Do not check the server's certificate (a lab, an internal CA).
    pub insecure: bool,
    /// The account searching the directory ("" : anonymous).
    pub bind_dn: String,
    /// Sent back empty to the browsers; an empty one keeps the saved one.
    pub bind_password: String,
    /// Where the users are: "dc=example,dc=com".
    pub base_dn: String,
    /// `{user}` is the name typed: "(sAMAccountName={user})" (AD),
    /// "(uid={user})" (OpenLDAP).
    pub user_filter: String,
    /// The attribute of the name shown.
    pub name_attribute: String,
    /// Members of these groups (DN or CN) get the role; the first matching
    /// from admin down.
    pub admin_groups: Vec<String>,
    pub designer_groups: Vec<String>,
    pub operator_groups: Vec<String>,
    /// The others ("" : refused).
    pub default_role: String,
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            starttls: false,
            insecure: false,
            bind_dn: String::new(),
            bind_password: String::new(),
            base_dn: String::new(),
            user_filter: "(sAMAccountName={user})".into(),
            name_attribute: "displayName".into(),
            admin_groups: Vec::new(),
            designer_groups: Vec::new(),
            operator_groups: Vec::new(),
            default_role: "viewer".into(),
        }
    }
}

/// The role of someone in these groups (DN or CN of each), by the lists.
pub fn role_of_groups(
    groups: &[String],
    admin: &[String],
    designer: &[String],
    operator: &[String],
    default_role: &str,
) -> Option<Role> {
    let cn = |g: &str| -> String {
        g.split(',')
            .next()
            .and_then(|p| p.split_once('='))
            .map_or(g, |(_, v)| v)
            .trim()
            .to_lowercase()
    };
    let has = |list: &[String]| {
        list.iter().filter(|x| !x.trim().is_empty()).any(|want| {
            groups.iter().any(|g| {
                g.eq_ignore_ascii_case(want.trim())
                    || cn(g) == cn(want)
                    || g.trim().eq_ignore_ascii_case(&cn(want))
            })
        })
    };
    if has(admin) {
        Some(Role::Admin)
    } else if has(designer) {
        Some(Role::Designer)
    } else if has(operator) {
        Some(Role::Operator)
    } else {
        parse_role(default_role)
    }
}

pub fn parse_role(s: &str) -> Option<Role> {
    serde_json::from_value(serde_json::Value::String(s.trim().to_lowercase())).ok()
}

/// Checks a name and password against the directory: their name shown and
/// role when they may sign in.
pub async fn ldap_login(
    c: &LdapConfig,
    user: &str,
    password: &str,
) -> anyhow::Result<Option<(String, Role)>> {
    use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
    // An empty password is an anonymous bind: never a sign-in.
    if !c.enabled || user.trim().is_empty() || password.is_empty() {
        return Ok(None);
    }
    let settings = LdapConnSettings::new()
        .set_conn_timeout(Duration::from_secs(5))
        .set_starttls(c.starttls)
        .set_no_tls_verify(c.insecure);
    let (conn, mut ldap) = LdapConnAsync::with_settings(settings, c.url.trim()).await?;
    ldap3::drive!(conn);
    ldap.with_timeout(Duration::from_secs(5));
    if !c.bind_dn.trim().is_empty() {
        ldap.simple_bind(c.bind_dn.trim(), &c.bind_password)
            .await?
            .success()?;
    }
    let filter = c
        .user_filter
        .replace("{user}", &ldap3::ldap_escape(user.trim()));
    let name_attr = if c.name_attribute.trim().is_empty() {
        "displayName"
    } else {
        c.name_attribute.trim()
    };
    let (entries, _) = ldap
        .search(
            c.base_dn.trim(),
            Scope::Subtree,
            &filter,
            vec![name_attr, "memberOf", "cn"],
        )
        .await?
        .success()?;
    let Some(entry) = entries.into_iter().next().map(SearchEntry::construct) else {
        let _ = ldap.unbind().await;
        return Ok(None);
    };
    // Their own password, on their own entry.
    let ok = ldap
        .simple_bind(&entry.dn, password)
        .await?
        .success()
        .is_ok();
    let _ = ldap.unbind().await;
    if !ok {
        return Ok(None);
    }
    let groups = entry.attrs.get("memberOf").cloned().unwrap_or_default();
    let role = role_of_groups(
        &groups,
        &c.admin_groups,
        &c.designer_groups,
        &c.operator_groups,
        &c.default_role,
    );
    let name = entry
        .attrs
        .get(name_attr)
        .or_else(|| entry.attrs.get("cn"))
        .and_then(|v| v.first().cloned())
        .unwrap_or_else(|| user.to_string());
    Ok(role.map(|r| (name, r)))
}

// ------------------------------------------------------------------- OIDC --

/// Signing in with an OpenID Connect provider (single sign-on).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OidcConfig {
    pub enabled: bool,
    /// The words of the sign-in button ("Microsoft", "Okta"…).
    pub label: String,
    /// https://login.microsoftonline.com/<tenant>/v2.0, https://accounts.google.com…
    pub issuer: String,
    pub client_id: String,
    /// Sent back empty to the browsers; an empty one keeps the saved one.
    pub client_secret: String,
    /// The claim of the user name and of the groups.
    pub username_claim: String,
    pub groups_claim: String,
    pub admin_groups: Vec<String>,
    pub designer_groups: Vec<String>,
    pub operator_groups: Vec<String>,
    pub default_role: String,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            label: "SSO".into(),
            issuer: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            username_claim: "preferred_username".into(),
            groups_claim: "groups".into(),
            admin_groups: Vec::new(),
            designer_groups: Vec::new(),
            operator_groups: Vec::new(),
            default_role: "viewer".into(),
        }
    }
}

/// Sign-ins under way: state → (nonce, where to come back, since when).
#[derive(Default)]
pub struct OidcPending {
    map: Mutex<BTreeMap<String, (String, String, Instant)>>,
}

impl OidcPending {
    pub fn put(&self, state: String, nonce: String, redirect: String) {
        let mut m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        m.retain(|_, (_, _, at)| at.elapsed() < Duration::from_secs(600));
        m.insert(state, (nonce, redirect, Instant::now()));
    }
    pub fn take(&self, state: &str) -> Option<(String, String)> {
        let mut m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        m.remove(state)
            .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(600))
            .map(|(n, r, _)| (n, r))
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn get_json(url: &str) -> anyhow::Result<serde_json::Value> {
    let mut r = agent().get(url).call()?;
    Ok(serde_json::from_slice(&r.body_mut().read_to_vec()?)?)
}

/// The provider's endpoints (OpenID discovery).
pub struct Discovery {
    pub authorization: String,
    pub token: String,
    pub jwks: String,
    pub issuer: String,
}

pub fn discover(issuer: &str) -> anyhow::Result<Discovery> {
    let base = issuer.trim().trim_end_matches('/');
    let d = get_json(&format!("{base}/.well-known/openid-configuration"))?;
    let s = |k: &str| {
        d.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let out = Discovery {
        authorization: s("authorization_endpoint"),
        token: s("token_endpoint"),
        jwks: s("jwks_uri"),
        issuer: s("issuer"),
    };
    anyhow::ensure!(
        !out.authorization.is_empty() && !out.token.is_empty(),
        "not an OpenID provider: {base}"
    );
    Ok(out)
}

fn b64(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('='))?)
}

/// Exchanges the code for the ID token, checks it (signature RS256 against
/// the provider's keys, issuer, audience, expiry, nonce): its claims.
pub fn finish(
    c: &OidcConfig,
    d: &Discovery,
    code: &str,
    redirect: &str,
    nonce: &str,
) -> anyhow::Result<serde_json::Value> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", c.client_id.trim()),
        ("client_secret", c.client_secret.as_str()),
    ];
    let body: String = form
        .iter()
        .map(|(k, v)| format!("{k}={}", urlenc(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut r = agent()
        .post(&d.token)
        .header("content-type", "application/x-www-form-urlencoded")
        .send(body)?;
    let tok: serde_json::Value = serde_json::from_slice(&r.body_mut().read_to_vec()?)?;
    let id = tok
        .get("id_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no ID token: {}",
                tok.get("error_description")
                    .or(tok.get("error"))
                    .unwrap_or(&serde_json::Value::Null)
            )
        })?;
    let parts: Vec<&str> = id.split('.').collect();
    anyhow::ensure!(parts.len() == 3, "malformed ID token");
    let header: serde_json::Value = serde_json::from_slice(&b64(parts[0])?)?;
    let claims: serde_json::Value = serde_json::from_slice(&b64(parts[1])?)?;
    let sig = b64(parts[2])?;
    anyhow::ensure!(
        header.get("alg").and_then(|v| v.as_str()) == Some("RS256"),
        "unsupported token signature (RS256 only)"
    );
    let kid = header
        .get("kid")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let keys = get_json(&d.jwks)?;
    let key = keys
        .get("keys")
        .and_then(|k| k.as_array())
        .and_then(|list| {
            list.iter()
                .find(|k| kid.is_empty() || k.get("kid").and_then(|v| v.as_str()) == Some(kid))
        })
        .ok_or_else(|| anyhow::anyhow!("signing key not found"))?;
    let n = b64(key.get("n").and_then(|v| v.as_str()).unwrap_or_default())?;
    let e = b64(key.get("e").and_then(|v| v.as_str()).unwrap_or_default())?;
    let public = ring::signature::RsaPublicKeyComponents { n: &n, e: &e };
    let signed = format!("{}.{}", parts[0], parts[1]);
    public
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            signed.as_bytes(),
            &sig,
        )
        .map_err(|_| anyhow::anyhow!("ID token signature does not verify"))?;
    let iss = claims
        .get("iss")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    anyhow::ensure!(
        d.issuer.is_empty() || iss == d.issuer,
        "ID token from another issuer"
    );
    let aud_ok = match claims.get("aud") {
        Some(serde_json::Value::String(a)) => a == c.client_id.trim(),
        Some(serde_json::Value::Array(a)) => {
            a.iter().any(|x| x.as_str() == Some(c.client_id.trim()))
        }
        _ => false,
    };
    anyhow::ensure!(aud_ok, "ID token for another application");
    let now = jiff::Timestamp::now().as_second();
    let exp = claims.get("exp").and_then(|v| v.as_i64()).unwrap_or(0);
    anyhow::ensure!(exp + 60 > now, "ID token expired");
    anyhow::ensure!(
        claims.get("nonce").and_then(|v| v.as_str()) == Some(nonce),
        "ID token replayed"
    );
    Ok(claims)
}

/// The name, shown name and role of the claims, by the settings.
pub fn identity(c: &OidcConfig, claims: &serde_json::Value) -> Option<(String, String, Role)> {
    let s = |k: &str| claims.get(k).and_then(|v| v.as_str()).map(String::from);
    let user = s(c.username_claim.trim())
        .or_else(|| s("email"))
        .or_else(|| s("sub"))?;
    let name = s("name").unwrap_or_else(|| user.clone());
    let groups: Vec<String> = match claims.get(c.groups_claim.trim()) {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        Some(serde_json::Value::String(g)) => vec![g.clone()],
        _ => Vec::new(),
    };
    let role = role_of_groups(
        &groups,
        &c.admin_groups,
        &c.designer_groups,
        &c.operator_groups,
        &c.default_role,
    )?;
    // Names of this server: letters, digits, . _ - @.
    let user: String = user
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || "._-@".contains(ch) {
                ch
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    Some((user, name, role))
}

pub fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ------------------------------------------------------------------ audit --

/// A change made through the API: when, who, from where, what.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// ms since 1970.
    pub at: i64,
    pub user: String,
    pub ip: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    /// What it is, in words ("Signed in", "Multiview saved"…).
    pub action: String,
}

/// The audit journal: one JSON line per change in `<data>/audit.log`
/// (rotated at 20 MB to `audit.1.log`).
pub struct Audit {
    file: PathBuf,
    lock: Mutex<()>,
}

const AUDIT_MAX: u64 = 20 * 1024 * 1024;

impl Audit {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            file: data_dir.join("audit.log"),
            lock: Mutex::new(()),
        }
    }

    pub fn write(&self, e: &AuditEntry) {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if std::fs::metadata(&self.file).is_ok_and(|m| m.len() > AUDIT_MAX) {
            let _ = std::fs::rename(&self.file, self.file.with_file_name("audit.1.log"));
        }
        let line = match serde_json::to_string(e) {
            Ok(l) => l,
            Err(_) => return,
        };
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// The newest `limit` lines (of the current file and the one before).
    pub fn read(&self, limit: usize) -> Vec<AuditEntry> {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: Vec<AuditEntry> = Vec::new();
        for f in [self.file.clone(), self.file.with_file_name("audit.1.log")] {
            if let Ok(s) = std::fs::read_to_string(&f) {
                let mut lines: Vec<AuditEntry> = s
                    .lines()
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect();
                lines.reverse();
                out.extend(lines);
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        out
    }
}

/// What a request does, in words.
pub fn action_of(method: &str, path: &str) -> String {
    let p = path.trim_end_matches('/');
    let seg: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    let has = |s: &str| seg.contains(&s);
    let what = match () {
        _ if p.starts_with("/x-nmos/connection") => "NMOS · connexion IS-05",
        _ if p.starts_with("/x-nmos") => "NMOS",
        _ if p == "/api/auth/login" => "Connexion",
        _ if p == "/api/auth/logout" => "Déconnexion",
        _ if p.starts_with("/api/auth/oidc") => "Connexion SSO",
        _ if p == "/api/auth/setup" => "Premier administrateur créé",
        _ if p.starts_with("/api/system/config") => "Réglages du serveur",
        _ if p.starts_with("/api/users") => "Utilisateurs",
        _ if p.starts_with("/api/keys") => "Clés d’API",
        _ if p.starts_with("/api/alarms/ack") => "Alarme acquittée",
        _ if p.starts_with("/api/alarms") => "Réglages des alarmes",
        _ if p.starts_with("/api/tally") => "Tally",
        _ if has("generator") => "Générateur de test",
        _ if p.starts_with("/api/channels") => "Canal (multiview à l’antenne)",
        _ if has("publish") => "Multiview publié",
        _ if p.starts_with("/api/projects") => "Multiview",
        _ if p.starts_with("/api/media") || p.starts_with("/api/assets") => "Bibliothèque",
        _ if p.starts_with("/api/presets") => "Préréglages",
        _ if p.starts_with("/api/inputs") => "Entrées",
        _ if p.starts_with("/api/hardware") => "Matériel",
        _ => "",
    };
    let verb = match method {
        "POST" => "création / action",
        "PUT" | "PATCH" => "modification",
        "DELETE" => "suppression",
        _ => "",
    };
    match (what, verb) {
        ("", v) => format!("{v} · {p}"),
        (w, _)
            if w.starts_with("Connexion")
                || w.starts_with("Déconnexion")
                || w.contains("acquittée")
                || w.contains("publié")
                || w.contains("créé") =>
        {
            w.to_string()
        }
        (w, v) => format!("{w} · {v}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_give_roles() {
        let admin = vec!["CN=Régie-Admins,OU=Groups,DC=ex,DC=com".to_string()];
        let ops = vec!["Operateurs".to_string()];
        let g = vec!["CN=Operateurs,OU=Groups,DC=ex,DC=com".to_string()];
        assert_eq!(
            role_of_groups(&g, &admin, &[], &ops, ""),
            Some(Role::Operator)
        );
        let g = vec!["cn=régie-admins,ou=groups,dc=ex,dc=com".to_string()];
        assert_eq!(role_of_groups(&g, &admin, &[], &ops, ""), Some(Role::Admin));
        assert_eq!(role_of_groups(&[], &admin, &[], &ops, ""), None);
        assert_eq!(
            role_of_groups(&[], &admin, &[], &ops, "viewer"),
            Some(Role::Viewer)
        );
    }

    #[test]
    fn a_certificate_for_this_machine() {
        let dir = std::env::temp_dir().join(format!("smv-tls-{}", std::process::id()));
        let (c, k) = tls_files(&dir, &TlsConfig::default()).unwrap();
        assert!(std::fs::read_to_string(&c)
            .unwrap()
            .contains("BEGIN CERTIFICATE"));
        assert!(std::fs::read_to_string(&k).unwrap().contains("PRIVATE KEY"));
        // Kept: the same one the next time.
        let before = std::fs::read(&c).unwrap();
        tls_files(&dir, &TlsConfig::default()).unwrap();
        assert_eq!(std::fs::read(&c).unwrap(), before);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_audit_journal_keeps_the_newest_first() {
        let dir = std::env::temp_dir().join(format!("smv-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = Audit::new(&dir);
        for i in 0..3 {
            a.write(&AuditEntry {
                at: i,
                user: "admin".into(),
                ip: "10.0.0.1".into(),
                method: "PUT".into(),
                path: "/api/alarms/settings".into(),
                status: 204,
                action: action_of("PUT", "/api/alarms/settings"),
            });
        }
        let r = a.read(2);
        assert_eq!(r.iter().map(|e| e.at).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(r[0].action, "Réglages des alarmes · modification");
        assert_eq!(action_of("POST", "/api/auth/login"), "Connexion");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn oidc_claims_to_an_account() {
        let c = OidcConfig {
            admin_groups: vec!["mv-admins".into()],
            default_role: "operator".into(),
            ..Default::default()
        };
        let claims = serde_json::json!({ "preferred_username": "jean.dupont@ex.com", "name": "Jean Dupont", "groups": ["mv-admins"] });
        assert_eq!(
            identity(&c, &claims),
            Some((
                "jean.dupont@ex.com".into(),
                "Jean Dupont".into(),
                Role::Admin
            ))
        );
        let claims = serde_json::json!({ "email": "a b@ex.com" });
        assert_eq!(
            identity(&c, &claims).map(|x| (x.0, x.2)),
            Some(("a_b@ex.com".into(), Role::Operator))
        );
    }
}
