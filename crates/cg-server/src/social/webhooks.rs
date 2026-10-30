//! Platforms that call the server: TikTok Live (through the bridge of
//! tools/tiktok-bridge), WhatsApp (Cloud API) and SMS (Twilio). They post to
//! `/api/social/hook/<connection id>?token=<settings.hookToken>`, an address
//! reachable from the internet (the customer's own infrastructure).

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use super::http::{form, get_bytes, get_json};
use super::run::{connected, message, setting};
use super::{Author, Hub, Message, Platform, SocialSource};
use crate::state::{now_ms, Shared};

/// The bridge is live when it called in the last minute.
const BRIDGE_ALIVE: i64 = 60_000;

/// Keeps the status of a webhook connection: it does not connect anywhere.
pub async fn watch(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    loop {
        if src.platform == Platform::Tiktok {
            let seen = hub.status(&src.id).hook_at;
            let live = seen.is_some_and(|t| now_ms() - t < BRIDGE_ALIVE);
            connected(hub, src, !live);
        } else {
            connected(hub, src, false);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// A phone number as shown: "+33 6 •• •• 41 07".
pub fn mask_phone(number: &str) -> String {
    let d: String = number.chars().filter(char::is_ascii_digit).collect();
    if d.len() < 8 {
        return number.to_string();
    }
    // Country code: 1 digit (+1, +7), 3 digits (most of Africa, …), else 2.
    const THREE: &[&str] = &[
        "21", "22", "23", "24", "25", "26", "29", "35", "37", "38", "42", "50", "59", "67", "68",
        "69", "85", "88", "96", "97", "99",
    ];
    let cc = match &d[..2] {
        p if p.starts_with('1') || p.starts_with('7') => 1,
        p if THREE.contains(&p) => 3,
        _ => 2,
    };
    let (code, national) = d.split_at(cc);
    let (first, rest) = national.split_at(1);
    let (middle, last) = rest.split_at(rest.len().saturating_sub(4));
    let dots = vec!["••"; middle.len().div_ceil(2)].join(" ");
    format!("+{code} {first} {dots} {} {}", &last[..2], &last[2..]).replace("  ", " ")
}

fn author_by_phone(number: &str, name: Option<&str>) -> Author {
    let number = if number.starts_with('+') {
        number.to_string()
    } else {
        format!("+{number}")
    };
    Author {
        name: name
            .filter(|n| !n.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| mask_phone(&number)),
        id: number,
        ..Default::default()
    }
}

/// The messages of a TikTok bridge call: one or a list of
/// `{id, author: {id, name, avatar}, text, amount}`; `{heartbeat: true}` only
/// says the bridge is alive.
pub fn parse_tiktok(src: &SocialSource, body: &Value) -> Vec<Message> {
    let list = match body {
        Value::Array(a) => a.iter().collect(),
        v => vec![v],
    };
    let mut out = Vec::new();
    for v in list {
        let text = v["text"].as_str().unwrap_or("");
        if text.trim().is_empty() && v["amount"].as_str().unwrap_or("").is_empty() {
            continue;
        }
        let a = &v["author"];
        let handle = a["handle"].as_str().unwrap_or("");
        let author = Author {
            id: a["id"].as_str().unwrap_or(handle).to_string(),
            name: a["name"].as_str().unwrap_or(handle).to_string(),
            handle: handle.to_string(),
            avatar: a["avatar"].as_str().unwrap_or("").to_string(),
            ..Default::default()
        };
        let id = match &v["id"] {
            Value::String(s) if !s.is_empty() => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => uuid::Uuid::new_v4().simple().to_string(),
        };
        let mut m = message(src, &id, author, text);
        m.amount = v["amount"].as_str().unwrap_or("").to_string();
        out.push(m);
    }
    out
}

/// A message of a WhatsApp webhook, with the id of its picture to download.
pub struct WhatsappMessage {
    pub message: Message,
    pub media: Option<String>,
}

/// The messages of a WhatsApp Cloud API webhook (texts and pictures).
pub fn parse_whatsapp(src: &SocialSource, body: &Value) -> Vec<WhatsappMessage> {
    let only = setting(src, "phoneNumberId");
    let mut out = Vec::new();
    for change in body["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| e["changes"].as_array().into_iter().flatten())
    {
        let v = &change["value"];
        if !only.is_empty() && v["metadata"]["phone_number_id"].as_str() != Some(only) {
            continue;
        }
        let names: HashMap<&str, &str> = v["contacts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| Some((c["wa_id"].as_str()?, c["profile"]["name"].as_str()?)))
            .collect();
        for m in v["messages"].as_array().into_iter().flatten() {
            let (Some(id), Some(from)) = (m["id"].as_str(), m["from"].as_str()) else {
                continue;
            };
            let (text, media) = match m["type"].as_str() {
                Some("text") => (m["text"]["body"].as_str().unwrap_or(""), None),
                Some("image") => (
                    m["image"]["caption"].as_str().unwrap_or(""),
                    m["image"]["id"].as_str().map(String::from),
                ),
                _ => continue,
            };
            let author = author_by_phone(from, names.get(from).copied());
            out.push(WhatsappMessage {
                message: message(src, id, author, text),
                media,
            });
        }
    }
    out
}

/// An SMS / MMS of a Twilio webhook (form fields), with the addresses of its
/// pictures. `settings.keyword`: only the messages starting with it, without it.
pub fn parse_twilio(
    src: &SocialSource,
    fields: &[(String, String)],
) -> Option<(Message, Vec<String>)> {
    let get = |k: &str| {
        fields
            .iter()
            .find(|(n, _)| n == k)
            .map_or("", |(_, v)| v.as_str())
    };
    let from = get("From");
    if from.is_empty() {
        return None;
    }
    let mut text = get("Body").trim();
    let keyword = setting(src, "keyword");
    if !keyword.is_empty() {
        let head = text.get(..keyword.len())?;
        if !head.eq_ignore_ascii_case(keyword) {
            return None;
        }
        text = text[keyword.len()..]
            .trim_start_matches([' ', ':', ',', '-'])
            .trim();
    }
    let id = match get("MessageSid") {
        "" => uuid::Uuid::new_v4().simple().to_string(),
        s => s.to_string(),
    };
    let mut author = author_by_phone(from, None);
    author.city = get("FromCity").to_string();
    let n: usize = get("NumMedia").parse().unwrap_or(0);
    let photos = (0..n.min(10))
        .filter(|i| {
            get(&format!("MediaContentType{i}")).starts_with("image/")
                || get(&format!("MediaContentType{i}")).is_empty()
        })
        .map(|i| get(&format!("MediaUrl{i}")).to_string())
        .filter(|u| !u.is_empty())
        .collect();
    Some((message(src, &id, author, text), photos))
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `X-Hub-Signature-256: sha256=<hex>`: HMAC-SHA256 of the body with the app secret.
pub fn signed(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(sig) = signature.strip_prefix("sha256=").and_then(hex) else {
        return false;
    };
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    ring::hmac::verify(&key, body, &sig).is_ok()
}

/// WhatsApp's check of the webhook address: answers `hub.challenge` when
/// `hub.verify_token` is the connection's.
pub fn verify(src: &SocialSource, q: &HashMap<String, String>) -> Result<String, StatusCode> {
    let want = setting(src, "verifyToken");
    let ok = q.get("hub.mode").map(String::as_str) == Some("subscribe")
        && !want.is_empty()
        && q.get("hub.verify_token").map(|t| t.as_str()) == Some(want);
    match (ok, q.get("hub.challenge")) {
        (true, Some(c)) => Ok(c.clone()),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

pub async fn hook_get(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(src) = s.social.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match verify(&src, &q) {
        Ok(c) => {
            s.social.set_status(&id, |st| st.hook_at = Some(now_ms()));
            c.into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// The token of the address, in `?token=` or the `X-Strata-Token` header.
fn token_ok(src: &SocialSource, q: &HashMap<String, String>, headers: &HeaderMap) -> bool {
    let want = setting(src, "hookToken");
    let given = q
        .get("token")
        .map(String::as_str)
        .or_else(|| headers.get("x-strata-token").and_then(|v| v.to_str().ok()));
    !want.is_empty() && given == Some(want)
}

pub async fn hook_post(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(src) = s.social.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let secret = setting(&src, "appSecret");
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let allowed = if src.platform == Platform::Whatsapp && !secret.is_empty() {
        signed(secret, &body, signature)
    } else {
        token_ok(&src, &q, &headers)
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, "wrong token or signature").into_response();
    }
    s.social.set_status(&id, |st| st.hook_at = Some(now_ms()));
    let twiml = || {
        (
            [(header::CONTENT_TYPE, "text/xml")],
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response></Response>",
        )
            .into_response()
    };
    // Turned off: taken, and dropped.
    if !src.enabled {
        return match src.platform {
            Platform::Sms => twiml(),
            _ => StatusCode::OK.into_response(),
        };
    }
    match src.platform {
        Platform::Tiktok => {
            let Ok(v) = serde_json::from_slice::<Value>(&body) else {
                return (StatusCode::BAD_REQUEST, "not JSON").into_response();
            };
            let n = parse_tiktok(&src, &v)
                .into_iter()
                .filter(|m| s.social.publish(m.clone()))
                .count();
            axum::Json(serde_json::json!({ "received": n })).into_response()
        }
        Platform::Whatsapp => {
            let Ok(v) = serde_json::from_slice::<Value>(&body) else {
                return (StatusCode::BAD_REQUEST, "not JSON").into_response();
            };
            for wm in parse_whatsapp(&src, &v) {
                match wm.media {
                    None => {
                        s.social.publish(wm.message);
                    }
                    // The picture first (WhatsApp's need the token to be read).
                    Some(media) => {
                        let (s, src) = (s.clone(), src.clone());
                        tokio::spawn(async move {
                            let mut m = wm.message;
                            match whatsapp_media(&s, &src, &media).await {
                                Ok(url) => m.photos.push(url),
                                Err(e) => tracing::warn!(source = %src.id, "WhatsApp picture: {e}"),
                            }
                            s.social.publish(m);
                        });
                    }
                }
            }
            StatusCode::OK.into_response()
        }
        Platform::Sms => {
            let fields = form(&String::from_utf8_lossy(&body));
            if let Some((mut m, photos)) = parse_twilio(&src, &fields) {
                let (s, src) = (s.clone(), src.clone());
                tokio::spawn(async move {
                    for url in photos {
                        m.photos
                            .push(twilio_media(&s, &src, &url).await.unwrap_or(url));
                    }
                    s.social.publish(m);
                });
            }
            twiml()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            "this connection reads its platform itself",
        )
            .into_response(),
    }
}

/// Saves a picture to the server's media (`/assets/social/…`, outside the
/// shared library's list) and gives its address for the renderers.
async fn save(s: &Shared, name: &str, bytes: &[u8], kind: &str) -> Result<String, String> {
    let ext = match kind.split(';').next().unwrap_or("").trim() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let dir = s.assets_dir().join("social");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let file = format!("{safe}.{ext}");
    tokio::fs::write(dir.join(&file), bytes)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("/assets/social/{file}"))
}

async fn whatsapp_media(s: &Shared, src: &SocialSource, media: &str) -> Result<String, String> {
    let token = setting(src, "accessToken");
    if token.is_empty() {
        return Err("no access token to read the picture".into());
    }
    let a = get_json(
        format!("{}/{}", super::meta::GRAPH, super::http::enc(media)),
        Some(token.to_string()),
    )
    .await?;
    if !a.ok() {
        return Err(a.error());
    }
    let url = a.body["url"].as_str().ok_or("no picture address")?;
    let (bytes, kind) = get_bytes(url.to_string(), Some(token.to_string()), None).await?;
    save(s, &format!("{}-{media}", src.id), &bytes, &kind).await
}

async fn twilio_media(s: &Shared, src: &SocialSource, url: &str) -> Result<String, String> {
    let (sid, token) = (setting(src, "accountSid"), setting(src, "authToken"));
    let basic =
        (!sid.is_empty() && !token.is_empty()).then(|| (sid.to_string(), token.to_string()));
    let (bytes, kind) = get_bytes(url.to_string(), None, basic).await?;
    let name = url.rsplit('/').next().unwrap_or("mms");
    save(s, &format!("{}-{name}", src.id), &bytes, &kind).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn src(platform: Platform, settings: &[(&str, &str)]) -> SocialSource {
        SocialSource {
            id: "s".into(),
            name: "S".into(),
            platform,
            enabled: true,
            settings: settings
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn phone_numbers_are_masked() {
        assert_eq!(mask_phone("+33612344107"), "+33 6 •• •• 41 07");
        assert_eq!(mask_phone("33612344107"), "+33 6 •• •• 41 07");
        assert_eq!(mask_phone("+14155550123"), "+1 4 •• •• •• 01 23");
        assert_eq!(mask_phone("+212612345678"), "+212 6 •• •• 56 78");
        assert_eq!(mask_phone("3615"), "3615");
    }

    #[test]
    fn whatsapp_texts_and_pictures() {
        let body: Value = serde_json::from_str(
            r#"{"object":"whatsapp_business_account","entry":[{"id":"102290129340398","changes":[{"field":"messages","value":{
              "messaging_product":"whatsapp",
              "metadata":{"display_phone_number":"33180000000","phone_number_id":"106540352242922"},
              "contacts":[{"profile":{"name":"Sophie"},"wa_id":"33611112222"}],
              "messages":[
                {"from":"33612344107","id":"wamid.HBgLMzM2MTIzNDQxMDcVAgASGBQzQUY","timestamp":"1792865659","type":"image",
                 "image":{"caption":"Photo des inondations à Quimperlé ce soir 🌊","mime_type":"image/jpeg","sha256":"abc","id":"1003383421387256"}},
                {"from":"33611112222","id":"wamid.2","timestamp":"1792865660","type":"text","text":{"body":"Bravo pour le JT"}},
                {"from":"33611112222","id":"wamid.3","timestamp":"1792865661","type":"reaction","reaction":{"emoji":"👍"}}
              ]}}]}]}"#,
        )
        .unwrap();
        let ms = parse_whatsapp(&src(Platform::Whatsapp, &[]), &body);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].message.author.id, "+33612344107");
        assert_eq!(ms[0].message.author.name, "+33 6 •• •• 41 07");
        assert_eq!(
            ms[0].message.text,
            "Photo des inondations à Quimperlé ce soir 🌊"
        );
        assert_eq!(ms[0].media.as_deref(), Some("1003383421387256"));
        assert_eq!(ms[1].message.author.name, "Sophie");
        assert!(ms[1].media.is_none());
        // Another number of the account.
        let other = src(Platform::Whatsapp, &[("phoneNumberId", "1")]);
        assert!(parse_whatsapp(&other, &body).is_empty());
    }

    #[test]
    fn whatsapp_checks_the_address_and_the_signature() {
        let s = src(Platform::Whatsapp, &[("verifyToken", "strata-42")]);
        let q = |token: &str| {
            HashMap::from([
                ("hub.mode".to_string(), "subscribe".to_string()),
                ("hub.verify_token".to_string(), token.to_string()),
                ("hub.challenge".to_string(), "1158201444".to_string()),
            ])
        };
        assert_eq!(verify(&s, &q("strata-42")).unwrap(), "1158201444");
        assert_eq!(verify(&s, &q("nope")), Err(StatusCode::FORBIDDEN));
        assert!(verify(&src(Platform::Whatsapp, &[]), &q("")).is_err());

        let body = br#"{"object":"whatsapp_business_account"}"#;
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, b"app-secret");
        let tag = ring::hmac::sign(&key, body);
        let sig: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        assert!(signed("app-secret", body, &format!("sha256={sig}")));
        assert!(!signed("other", body, &format!("sha256={sig}")));
        assert!(!signed("app-secret", body, "sha256=zz"));
    }

    #[test]
    fn twilio_sms_with_a_keyword_and_a_picture() {
        let body = "ToCountry=FR&MediaContentType0=image%2Fjpeg&ToState=&SmsMessageSid=MM1&NumMedia=1&FromCity=Lyon\
                    &Body=JT+%3A+Que+fait+le+gouvernement+%3F&FromCountry=FR&To=%2B33644600000&MessageSid=MM8f2&AccountSid=AC1\
                    &From=%2B33612344107&MediaUrl0=https%3A%2F%2Fapi.twilio.com%2F2010-04-01%2FAccounts%2FAC1%2FMessages%2FMM8f2%2FMedia%2FME1&ApiVersion=2010-04-01";
        let fields = form(body);
        let s = src(Platform::Sms, &[("keyword", "jt")]);
        let (m, photos) = parse_twilio(&s, &fields).unwrap();
        assert_eq!(m.id, "s:MM8f2");
        assert_eq!(m.text, "Que fait le gouvernement ?");
        assert_eq!(m.author.id, "+33612344107");
        assert_eq!(m.author.name, "+33 6 •• •• 41 07");
        assert_eq!(m.author.city, "Lyon");
        assert_eq!(
            photos,
            vec!["https://api.twilio.com/2010-04-01/Accounts/AC1/Messages/MM8f2/Media/ME1"]
        );
        // Without the keyword: not for this connection.
        let other = form("From=%2B33600000000&Body=Bonjour&NumMedia=0");
        assert!(parse_twilio(&s, &other).is_none());
        assert_eq!(
            parse_twilio(&src(Platform::Sms, &[]), &other)
                .unwrap()
                .0
                .text,
            "Bonjour"
        );
    }

    #[test]
    fn tiktok_bridge_calls() {
        let s = src(Platform::Tiktok, &[]);
        let body: Value = serde_json::from_str(
            r#"[{"id":"7429","author":{"id":"6812","name":"Lina ✨","handle":"lina.tt","avatar":"https://p16-sign.tiktokcdn.com/a.jpeg"},"text":"On vous regarde depuis Brest"},
                {"author":{"id":"77","name":"Marc"},"text":"","amount":"Rose × 5"},
                {"heartbeat":true}]"#,
        )
        .unwrap();
        let ms = parse_tiktok(&s, &body);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].id, "s:7429");
        assert_eq!(ms[0].author.name, "Lina ✨");
        assert_eq!(ms[0].author.handle, "lina.tt");
        assert_eq!(ms[1].amount, "Rose × 5");
        assert!(parse_tiktok(&s, &serde_json::json!({"heartbeat": true})).is_empty());
    }
}
