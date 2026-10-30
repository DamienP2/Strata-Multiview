//! Users, roles and sessions.
//!
//! Users live in `<data>/users.json` (Argon2 password hashes); the file is
//! re-read when it changes on disk so `strata-server user …` works while the
//! server runs. Sessions are random tokens kept in memory, sent as an
//! HttpOnly cookie to browsers or as `Authorization: Bearer` to automation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use serde::{Deserialize, Serialize};

pub const COOKIE: &str = "cg_session";
/// Languages of the interface.
pub const LANGS: [&str; 2] = ["en", "fr"];
/// Colour themes of the interface ("system" follows the computer's).
pub const THEMES: [&str; 3] = ["dark", "light", "system"];
const SESSION_TTL: Duration = Duration::from_secs(14 * 24 * 3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Everything, including users, channels and server settings.
    Admin,
    /// Creates and edits projects (templates); can operate channels.
    Designer,
    /// Runs channels: loads projects, takes graphics, edits rundown data.
    Operator,
    /// Read-only: sees the program and the projects.
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    pub role: Role,
    /// Projects the user may access; `None` = all.
    #[serde(default)]
    pub projects: Option<Vec<String>>,
    /// Channels the user may access; `None` = all.
    #[serde(default)]
    pub channels: Option<Vec<String>>,
    #[serde(default)]
    pub disabled: bool,
    /// Language of the interface ("en", "fr"); empty = the browser's.
    #[serde(default)]
    pub lang: String,
    /// Colour theme of the interface ("dark", "light", "system"); empty = dark.
    #[serde(default)]
    pub theme: String,
    password_hash: String,
}

/// A user as sent to clients.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub projects: Option<Vec<String>>,
    pub channels: Option<Vec<String>>,
    pub disabled: bool,
    pub lang: String,
    pub theme: String,
}

impl From<&User> for PublicUser {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.clone(),
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            role: u.role,
            projects: u.projects.clone(),
            channels: u.channels.clone(),
            disabled: u.disabled,
            lang: u.lang.clone(),
            theme: u.theme.clone(),
        }
    }
}

fn in_scope(scope: &Option<Vec<String>>, id: &str) -> bool {
    scope.as_ref().is_none_or(|ids| ids.iter().any(|i| i == id))
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
    pub fn sees_project(&self, id: &str) -> bool {
        self.is_admin() || in_scope(&self.projects, id)
    }
    pub fn sees_channel(&self, id: &str) -> bool {
        self.is_admin() || in_scope(&self.channels, id)
    }
    /// Create projects.
    pub fn can_create_projects(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Designer)
    }
    /// Edit templates and project settings, delete the project.
    pub fn can_design(&self, project: &str) -> bool {
        matches!(self.role, Role::Admin | Role::Designer) && self.sees_project(project)
    }
    /// Edit the rundown (playlist and its data) of a project.
    pub fn can_edit_rundown(&self, project: &str) -> bool {
        matches!(self.role, Role::Admin | Role::Designer | Role::Operator)
            && self.sees_project(project)
    }
    /// Take / continue / out / update and load projects on a channel.
    pub fn can_operate(&self, channel: &str) -> bool {
        matches!(self.role, Role::Admin | Role::Designer | Role::Operator)
            && self.sees_channel(channel)
    }
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    // UUID v4 comes from the OS random generator.
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes())
        .map_err(|e| anyhow::anyhow!("salt: {e}"))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("cannot hash password: {e}"))
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}

pub fn validate_password(password: &str) -> Result<(), String> {
    if password.chars().count() < 8 {
        return Err("passwords need at least 8 characters".into());
    }
    Ok(())
}

fn validate_username(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'));
    if ok {
        Ok(())
    } else {
        Err("user names use letters, digits and . - _ @".into())
    }
}

/// Fields an admin can set when creating or editing a user.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInput {
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    pub role: Role,
    #[serde(default)]
    pub projects: Option<Vec<String>>,
    #[serde(default)]
    pub channels: Option<Vec<String>>,
    #[serde(default)]
    pub disabled: bool,
    /// Required on creation; empty keeps the current password on edit.
    #[serde(default)]
    pub password: String,
}

struct Session {
    user_id: String,
    expires: Instant,
}

/// Permanent token for automation (Companion, MOS gateways, scripts). It
/// acts with the rights of its user. A SHA-256 of it checks requests; a
/// sealed copy lets an admin see it again (after typing their password).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub user_id: String,
    /// First characters, to recognise a key in lists.
    pub prefix: String,
    pub created_at: i64,
    hash: String,
    /// The secret sealed with the server's key (`keys.secret`), hex; "" for
    /// a key made before keys could be shown again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    sealed: String,
}

/// An API key as listed to clients (never the secret).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiKey {
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub prefix: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

pub const API_KEY_PREFIX: &str = "pcg_";

fn sha256_hex(s: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Keys {
    list: Vec<ApiKey>,
    mtime: Option<SystemTime>,
}

struct Users {
    list: Vec<User>,
    mtime: Option<SystemTime>,
}

pub struct Auth {
    path: PathBuf,
    users: RwLock<Users>,
    sessions: Mutex<HashMap<String, Session>>,
    keys_path: PathBuf,
    /// The key that seals the API keys' secrets.
    seal_path: PathBuf,
    keys: RwLock<Keys>,
    /// Key id -> last use (ms), kept in memory.
    key_use: Mutex<HashMap<String, i64>>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Auth {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("users.json");
        let list = Self::read(&path);
        let keys_path = data_dir.join("api_keys.json");
        let keys = Self::read(&keys_path);
        Self {
            users: RwLock::new(Users {
                list,
                mtime: mtime(&path),
            }),
            path,
            sessions: Mutex::new(HashMap::new()),
            keys: RwLock::new(Keys {
                list: keys,
                mtime: mtime(&keys_path),
            }),
            keys_path,
            seal_path: data_dir.join("keys.secret"),
            key_use: Mutex::new(HashMap::new()),
        }
    }

    /// The sealing key, made on first use (readable by the server only).
    fn seal_key(&self) -> anyhow::Result<ring::aead::LessSafeKey> {
        use ring::aead::{LessSafeKey, UnboundKey, CHACHA20_POLY1305};
        let bytes = match std::fs::read(&self.seal_path) {
            Ok(b) if b.len() == 32 => b,
            _ => {
                let mut b = vec![0u8; 32];
                ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b)
                    .map_err(|_| anyhow::anyhow!("no random source"))?;
                std::fs::write(&self.seal_path, &b)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &self.seal_path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                b
            }
        };
        let key = UnboundKey::new(&CHACHA20_POLY1305, &bytes)
            .map_err(|_| anyhow::anyhow!("invalid sealing key"))?;
        Ok(LessSafeKey::new(key))
    }

    fn seal(&self, secret: &str) -> anyhow::Result<String> {
        use ring::aead::{Aad, Nonce, NONCE_LEN};
        let mut nonce = [0u8; NONCE_LEN];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce)
            .map_err(|_| anyhow::anyhow!("no random source"))?;
        let mut data = secret.as_bytes().to_vec();
        self.seal_key()?
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut data)
            .map_err(|_| anyhow::anyhow!("cannot seal"))?;
        Ok(nonce
            .iter()
            .chain(&data)
            .map(|b| format!("{b:02x}"))
            .collect())
    }

    fn unseal(&self, sealed: &str) -> Option<String> {
        use ring::aead::{Aad, Nonce, NONCE_LEN};
        let bytes: Vec<u8> = (0..sealed.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(sealed.get(i..i + 2)?, 16).ok())
            .collect::<Option<_>>()?;
        if bytes.len() <= NONCE_LEN {
            return None;
        }
        let (n, ct) = bytes.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(n).ok()?;
        let mut data = ct.to_vec();
        let plain = self
            .seal_key()
            .ok()?
            .open_in_place(nonce, Aad::empty(), &mut data)
            .ok()?;
        String::from_utf8(plain.to_vec()).ok()
    }

    fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
        match std::fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                tracing::error!(?path, "invalid file: {e}");
                Vec::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            // E.g. owned by root, unreadable by the service account.
            Err(e) => {
                tracing::error!(?path, "cannot read: {e}");
                Vec::new()
            }
        }
    }

    /// Picks up edits made by the command line while the server runs.
    fn refresh(&self) {
        let current = mtime(&self.path);
        let stale = self
            .users
            .read()
            .map(|u| u.mtime != current)
            .unwrap_or(true);
        if stale {
            let list = Self::read(&self.path);
            if let Ok(mut u) = self.users.write() {
                *u = Users {
                    list,
                    mtime: current,
                };
            }
        }
    }

    fn write_private<T: Serialize>(path: &Path, list: &[T]) -> anyhow::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn save(&self, list: Vec<User>) -> anyhow::Result<()> {
        Self::write_private(&self.path, &list)?;
        let mut u = self
            .users
            .write()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        *u = Users {
            list,
            mtime: mtime(&self.path),
        };
        Ok(())
    }

    pub fn users(&self) -> Vec<User> {
        self.refresh();
        self.users
            .read()
            .map(|u| u.list.clone())
            .unwrap_or_default()
    }

    pub fn needs_setup(&self) -> bool {
        self.users().is_empty()
    }

    pub fn create(&self, input: UserInput) -> Result<User, String> {
        validate_username(&input.username)?;
        validate_password(&input.password)?;
        let mut list = self.users();
        if list
            .iter()
            .any(|u| u.username.eq_ignore_ascii_case(&input.username))
        {
            return Err(format!("user `{}` already exists", input.username));
        }
        let user = User {
            id: cg_core::new_id(),
            display_name: if input.display_name.trim().is_empty() {
                input.username.clone()
            } else {
                input.display_name.trim().to_string()
            },
            username: input.username,
            role: input.role,
            projects: input.projects,
            channels: input.channels,
            disabled: input.disabled,
            lang: String::new(),
            theme: String::new(),
            password_hash: hash_password(&input.password).map_err(|e| e.to_string())?,
        };
        list.push(user.clone());
        self.save(list).map_err(|e| e.to_string())?;
        Ok(user)
    }

    pub fn update(&self, id: &str, input: UserInput) -> Result<User, String> {
        validate_username(&input.username)?;
        let mut list = self.users();
        if list
            .iter()
            .any(|u| u.id != id && u.username.eq_ignore_ascii_case(&input.username))
        {
            return Err(format!("user `{}` already exists", input.username));
        }
        let idx = list.iter().position(|u| u.id == id).ok_or("no such user")?;
        let u = &mut list[idx];
        u.username = input.username;
        u.display_name = if input.display_name.trim().is_empty() {
            u.username.clone()
        } else {
            input.display_name.trim().to_string()
        };
        u.role = input.role;
        u.projects = input.projects;
        u.channels = input.channels;
        u.disabled = input.disabled;
        if !input.password.is_empty() {
            validate_password(&input.password)?;
            u.password_hash = hash_password(&input.password).map_err(|e| e.to_string())?;
        }
        let updated = u.clone();
        if !list.iter().any(|u| u.role == Role::Admin && !u.disabled) {
            return Err("at least one active admin is required".into());
        }
        self.save(list).map_err(|e| e.to_string())?;
        if updated.disabled {
            self.drop_sessions_of(&updated.id);
        }
        Ok(updated)
    }

    /// The language a user chose for the interface.
    pub fn set_lang(&self, id: &str, lang: &str) -> Result<User, String> {
        if !LANGS.contains(&lang) {
            return Err(format!("unknown language `{lang}`"));
        }
        let mut list = self.users();
        let u = list.iter_mut().find(|u| u.id == id).ok_or("no such user")?;
        u.lang = lang.to_string();
        let updated = u.clone();
        self.save(list).map_err(|e| e.to_string())?;
        Ok(updated)
    }

    /// The colour theme a user chose for the interface.
    pub fn set_theme(&self, id: &str, theme: &str) -> Result<User, String> {
        if !THEMES.contains(&theme) {
            return Err(format!("unknown theme `{theme}`"));
        }
        let mut list = self.users();
        let u = list.iter_mut().find(|u| u.id == id).ok_or("no such user")?;
        u.theme = theme.to_string();
        let updated = u.clone();
        self.save(list).map_err(|e| e.to_string())?;
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut list = self.users();
        let before = list.len();
        list.retain(|u| u.id != id);
        if list.len() == before {
            return Err("no such user".into());
        }
        if !list.iter().any(|u| u.role == Role::Admin && !u.disabled) {
            return Err("at least one active admin is required".into());
        }
        self.save(list).map_err(|e| e.to_string())?;
        self.drop_sessions_of(id);
        // Their API keys go with them.
        let keys: Vec<ApiKey> = self
            .api_keys_raw()
            .into_iter()
            .filter(|k| k.user_id != id)
            .collect();
        self.save_keys(keys).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn api_keys_raw(&self) -> Vec<ApiKey> {
        let current = mtime(&self.keys_path);
        let stale = self.keys.read().map(|k| k.mtime != current).unwrap_or(true);
        if stale {
            let list = Self::read(&self.keys_path);
            if let Ok(mut k) = self.keys.write() {
                *k = Keys {
                    list,
                    mtime: current,
                };
            }
        }
        self.keys.read().map(|k| k.list.clone()).unwrap_or_default()
    }

    fn save_keys(&self, list: Vec<ApiKey>) -> anyhow::Result<()> {
        Self::write_private(&self.keys_path, &list)?;
        let mut k = self
            .keys
            .write()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        *k = Keys {
            list,
            mtime: mtime(&self.keys_path),
        };
        Ok(())
    }

    pub fn api_keys(&self) -> Vec<PublicApiKey> {
        let uses = self.key_use.lock().map(|m| m.clone()).unwrap_or_default();
        self.api_keys_raw()
            .into_iter()
            .map(|k| PublicApiKey {
                last_used: uses.get(&k.id).copied(),
                id: k.id,
                name: k.name,
                user_id: k.user_id,
                prefix: k.prefix,
                created_at: k.created_at,
            })
            .collect()
    }

    /// Creates a key for `user_id`; the secret is returned once.
    pub fn create_api_key(
        &self,
        user_id: &str,
        name: &str,
    ) -> Result<(String, PublicApiKey), String> {
        if !self.users().iter().any(|u| u.id == user_id) {
            return Err("no such user".into());
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err("give the key a name (what uses it)".into());
        }
        let secret = format!(
            "{API_KEY_PREFIX}{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let key = ApiKey {
            id: cg_core::new_id(),
            name: name.into(),
            user_id: user_id.into(),
            prefix: secret[..API_KEY_PREFIX.len() + 6].into(),
            created_at: crate::state::now_ms(),
            hash: sha256_hex(&secret),
            sealed: self.seal(&secret).map_err(|e| e.to_string())?,
        };
        let mut list = self.api_keys_raw();
        list.push(key.clone());
        self.save_keys(list).map_err(|e| e.to_string())?;
        let public = PublicApiKey {
            id: key.id,
            name: key.name,
            user_id: key.user_id,
            prefix: key.prefix,
            created_at: key.created_at,
            last_used: None,
        };
        Ok((secret, public))
    }

    /// The secret of a key, again, for an admin who types their password.
    pub fn reveal_api_key(
        &self,
        id: &str,
        admin_id: &str,
        password: &str,
    ) -> Result<String, String> {
        let admin = self
            .users()
            .into_iter()
            .find(|u| u.id == admin_id && u.role == Role::Admin && !u.disabled)
            .ok_or("admins only")?;
        if !verify_password(password, &admin.password_hash) {
            return Err("wrong password".into());
        }
        let key = self
            .api_keys_raw()
            .into_iter()
            .find(|k| k.id == id)
            .ok_or("no such key")?;
        if key.sealed.is_empty() {
            return Err(
                "this key was made before keys could be shown again: create a new one".into(),
            );
        }
        self.unseal(&key.sealed)
            .filter(|s| sha256_hex(s) == key.hash)
            .ok_or_else(|| "the key cannot be read back (keys.secret changed)".into())
    }

    pub fn delete_api_key(&self, id: &str) -> Result<ApiKey, String> {
        let mut list = self.api_keys_raw();
        let idx = list.iter().position(|k| k.id == id).ok_or("no such key")?;
        let key = list.remove(idx);
        self.save_keys(list).map_err(|e| e.to_string())?;
        Ok(key)
    }

    /// Who acts on air with `token`: its user, or the API key by its name
    /// (a control surface, an automation).
    pub fn actor_for_token(&self, token: &str) -> Option<(User, crate::antenna::Actor)> {
        let user = self.user_for_token(token)?;
        if token.starts_with(API_KEY_PREFIX) {
            let hash = sha256_hex(token);
            let key = self.api_keys_raw().into_iter().find(|k| k.hash == hash)?;
            let actor = crate::antenna::Actor {
                kind: crate::antenna::ActorKind::Api,
                id: key.id,
                name: key.name,
            };
            return Some((user, actor));
        }
        let actor = crate::antenna::Actor::user(&user);
        Some((user, actor))
    }

    fn user_for_api_key(&self, secret: &str) -> Option<User> {
        let hash = sha256_hex(secret);
        let key = self.api_keys_raw().into_iter().find(|k| k.hash == hash)?;
        if let Ok(mut m) = self.key_use.lock() {
            m.insert(key.id.clone(), crate::state::now_ms());
        }
        self.users()
            .into_iter()
            .find(|u| u.id == key.user_id && !u.disabled)
    }

    /// Sets a password from the command line.
    pub fn set_password(&self, username: &str, password: &str) -> Result<(), String> {
        validate_password(password)?;
        let mut list = self.users();
        let u = list
            .iter_mut()
            .find(|u| u.username.eq_ignore_ascii_case(username))
            .ok_or("no such user")?;
        u.password_hash = hash_password(password).map_err(|e| e.to_string())?;
        self.save(list).map_err(|e| e.to_string())
    }

    fn drop_sessions_of(&self, user_id: &str) {
        if let Ok(mut s) = self.sessions.lock() {
            s.retain(|_, v| v.user_id != user_id);
        }
    }

    /// Checks credentials and opens a session.
    pub fn login(&self, username: &str, password: &str) -> Option<(String, User)> {
        let user = self
            .users()
            .into_iter()
            .find(|u| u.username.eq_ignore_ascii_case(username.trim()));
        // Hash even for unknown users so timing does not reveal them.
        let hash = user.as_ref().map(|u| u.password_hash.clone()).unwrap_or_else(|| "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$ZmFrZWZha2VmYWtlZmFrZWZha2VmYWtlZmFrZWZha2U".into());
        let ok = verify_password(password, &hash);
        let user = user.filter(|u| ok && !u.disabled)?;
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        if let Ok(mut s) = self.sessions.lock() {
            s.retain(|_, v| v.expires > Instant::now());
            s.insert(
                token.clone(),
                Session {
                    user_id: user.id.clone(),
                    expires: Instant::now() + SESSION_TTL,
                },
            );
        }
        Some((token, user))
    }

    pub fn logout(&self, token: &str) {
        if let Ok(mut s) = self.sessions.lock() {
            s.remove(token);
        }
    }

    pub fn user_for_token(&self, token: &str) -> Option<User> {
        if token.starts_with(API_KEY_PREFIX) {
            return self.user_for_api_key(token);
        }
        let user_id = {
            let mut s = self.sessions.lock().ok()?;
            let session = s.get_mut(token)?;
            if session.expires < Instant::now() {
                s.remove(token);
                return None;
            }
            session.expires = Instant::now() + SESSION_TTL;
            session.user_id.clone()
        };
        self.users()
            .into_iter()
            .find(|u| u.id == user_id && !u.disabled)
    }
}

/// Session token from the cookie or an `Authorization: Bearer` header.
pub fn token_from(parts: &Parts) -> Option<String> {
    // `?key=pcg_…`: pages embedded in other applications (newsroom MOS
    // plugin) cannot use our cookie; only API keys are accepted this way.
    if let Some(q) = parts.uri.query() {
        if let Some(k) = q.split('&').find_map(|kv| kv.strip_prefix("key=")) {
            if k.starts_with(API_KEY_PREFIX) {
                return Some(k.to_string());
            }
        }
    }
    if let Some(v) = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

/// The logged-in user (401 otherwise).
pub struct CurrentUser(pub User);

/// The logged-in user, if any.
pub struct MaybeUser(pub Option<User>);

impl FromRequestParts<crate::state::Shared> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        s: &crate::state::Shared,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(
            token_from(parts).and_then(|t| s.auth.user_for_token(&t)),
        ))
    }
}

/// The logged-in user and who they act as on air (themselves, or the API
/// key they came with).
pub struct Acting(pub User, pub crate::antenna::Actor);

impl FromRequestParts<crate::state::Shared> for Acting {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        s: &crate::state::Shared,
    ) -> Result<Self, Self::Rejection> {
        token_from(parts)
            .and_then(|t| s.auth.actor_for_token(&t))
            .map(|(u, a)| Acting(u, a))
            .ok_or((StatusCode::UNAUTHORIZED, "login required".into()))
    }
}

impl FromRequestParts<crate::state::Shared> for CurrentUser {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        s: &crate::state::Shared,
    ) -> Result<Self, Self::Rejection> {
        token_from(parts)
            .and_then(|t| s.auth.user_for_token(&t))
            .map(CurrentUser)
            .ok_or((StatusCode::UNAUTHORIZED, "login required".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, role: Role, pw: &str) -> UserInput {
        UserInput {
            username: name.into(),
            display_name: String::new(),
            role,
            projects: None,
            channels: None,
            disabled: false,
            password: pw.into(),
        }
    }

    #[test]
    fn users_sessions_and_rights() {
        let dir = std::env::temp_dir().join(format!("cg-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let auth = Auth::load(&dir);
        assert!(auth.needs_setup());
        let admin = auth
            .create(input("admin", Role::Admin, "correct horse"))
            .unwrap();
        assert!(auth.create(input("x", Role::Viewer, "short")).is_err());
        let mut op = input("op", Role::Operator, "operator pw");
        op.channels = Some(vec!["studio-a".into()]);
        op.projects = Some(vec!["news".into()]);
        let op = auth.create(op).unwrap();

        assert!(auth.login("admin", "wrong").is_none());
        let (token, u) = auth.login("ADMIN", "correct horse").unwrap();
        assert_eq!(u.id, admin.id);
        assert_eq!(auth.user_for_token(&token).unwrap().username, "admin");
        auth.logout(&token);
        assert!(auth.user_for_token(&token).is_none());

        assert!(op.can_operate("studio-a") && !op.can_operate("studio-b"));
        assert!(op.can_edit_rundown("news") && !op.can_design("news") && !op.sees_project("sport"));

        // API keys act as their user and die with it.
        let (secret, key) = auth.create_api_key(&op.id, "Companion").unwrap();
        assert!(secret.starts_with(API_KEY_PREFIX) && secret.starts_with(&key.prefix));
        assert_eq!(auth.user_for_token(&secret).unwrap().id, op.id);
        assert!(auth.user_for_token("pcg_wrong").is_none());
        assert!(!std::fs::read_to_string(dir.join("api_keys.json"))
            .unwrap()
            .contains(&secret));
        // An admin sees it again with their password, nobody else.
        assert_eq!(
            auth.reveal_api_key(&key.id, &admin.id, "correct horse")
                .unwrap(),
            secret
        );
        assert_eq!(
            auth.reveal_api_key(&key.id, &admin.id, "wrong")
                .unwrap_err(),
            "wrong password"
        );
        assert!(auth.reveal_api_key(&key.id, &op.id, "operator pw").is_err());
        auth.delete(&op.id).unwrap();
        assert!(auth.user_for_token(&secret).is_none() && auth.api_keys().is_empty());

        // Each user keeps the language of their interface.
        assert_eq!(admin.lang, "");
        assert_eq!(auth.set_lang(&admin.id, "fr").unwrap().lang, "fr");
        assert!(auth.set_lang(&admin.id, "de").is_err());
        // …and the colour theme of it.
        assert_eq!(admin.theme, "");
        assert_eq!(auth.set_theme(&admin.id, "light").unwrap().theme, "light");
        assert!(auth.set_theme(&admin.id, "pink").is_err());

        // The last admin cannot be removed; data survives a reload.
        assert!(auth.delete(&admin.id).is_err());
        assert_eq!(Auth::load(&dir).users().len(), 1);
        assert_eq!(Auth::load(&dir).users()[0].lang, "fr");
        assert_eq!(Auth::load(&dir).users()[0].theme, "light");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
