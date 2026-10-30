//! Facebook and Instagram comments, read with the Meta Graph API and a page
//! access token.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::http::{enc, get_json};
use super::run::{connected, message, setting};
use super::{Author, Hub, Message, SocialSource};

pub const GRAPH: &str = "https://graph.facebook.com/v21.0";
const EVERY: Duration = Duration::from_secs(4);
/// Looking for the page's live again when there is none.
const LIVE_EVERY: Duration = Duration::from_secs(30);
const TOKEN_EVERY: Duration = Duration::from_secs(3600);

async fn call(path: &str, token: &str) -> Result<Value, String> {
    let sep = if path.contains('?') { '&' } else { '?' };
    let a = get_json(
        format!("{GRAPH}/{path}{sep}access_token={}", enc(token)),
        None,
    )
    .await?;
    if a.ok() {
        Ok(a.body)
    } else {
        Err(a.error())
    }
}

/// When the token expires (`debug_token`), shown as a warning on the Data page.
async fn check_token(hub: &Hub, src: &SocialSource, token: &str) -> Result<(), String> {
    let v = call(&format!("debug_token?input_token={}", enc(token)), token).await?;
    if v["data"]["is_valid"] == Value::Bool(false) {
        return Err("the access token is no longer valid".into());
    }
    let at = v["data"]["expires_at"].as_i64().filter(|t| *t > 0);
    hub.set_status(&src.id, |st| st.expires_at = at.map(|t| t * 1000));
    Ok(())
}

/// Reads the comments of an object (live video, post, media) newer than the
/// last ones read.
struct Comments {
    since: HashMap<String, i64>,
}

impl Comments {
    async fn read(
        &mut self,
        hub: &Hub,
        src: &SocialSource,
        token: &str,
        object: &str,
        fields: &str,
    ) -> Result<(), String> {
        let mut path = format!("{object}/comments?fields={fields}&limit=50");
        if src.platform == super::Platform::Facebook {
            path.push_str("&order=reverse_chronological&filter=stream&live_filter=no_filter");
        }
        if let Some(t) = self.since.get(object) {
            path.push_str(&format!("&since={t}"));
        }
        let page = call(&path, token).await?;
        let ms = parse_comments(src, &page);
        if let Some(newest) = page["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["created_time"].as_str().or(c["timestamp"].as_str()))
            .filter_map(unix)
            .max()
        {
            // Same second again: the ids already read are dropped by the hub.
            self.since.insert(object.to_string(), newest);
        }
        for m in ms {
            hub.publish(m);
        }
        Ok(())
    }
}

/// `2026-10-24T18:14:22+0000` as unix seconds.
fn unix(s: &str) -> Option<i64> {
    let fixed = if s.len() > 5 && (s.ends_with("+0000") || s.ends_with("-0000")) {
        format!("{}+00:00", &s[..s.len() - 5])
    } else {
        s.to_string()
    };
    fixed.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

pub async fn facebook(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    let token = setting(src, "accessToken");
    if token.is_empty() {
        return Err("no access token".into());
    }
    let page = setting(src, "page");
    let post = setting(src, "post");
    let by_post = setting(src, "mode") == "post";
    if by_post && post.is_empty() {
        return Err("no post".into());
    }
    if !by_post && page.is_empty() {
        return Err("no page".into());
    }
    let fields = "id,message,created_time,from{id,name,picture},permalink_url,attachment";
    let mut comments = Comments {
        since: HashMap::new(),
    };
    let mut token_at: Option<Instant> = None;
    let mut live: Option<(String, Instant)> = None;
    loop {
        if token_at.is_none_or(|t| t.elapsed() > TOKEN_EVERY) {
            check_token(hub, src, token).await?;
            token_at = Some(Instant::now());
        }
        let object = if by_post {
            post.to_string()
        } else {
            if live
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() > LIVE_EVERY)
            {
                let v = call(
                    &format!(
                        "{}/live_videos?broadcast_status=[\"LIVE\"]&fields=id,title&limit=1",
                        enc(page)
                    ),
                    token,
                )
                .await?;
                live = v["data"][0]["id"]
                    .as_str()
                    .map(|id| (id.to_string(), Instant::now()));
                if live.is_none() {
                    connected(hub, src, true);
                    tokio::time::sleep(LIVE_EVERY).await;
                    continue;
                }
            }
            live.as_ref().map(|(id, _)| id.clone()).unwrap_or_default()
        };
        comments.read(hub, src, token, &object, fields).await?;
        connected(hub, src, false);
        tokio::time::sleep(EVERY).await;
    }
}

pub async fn instagram(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    let token = setting(src, "accessToken");
    if token.is_empty() {
        return Err("no access token".into());
    }
    let user = setting(src, "igUser");
    let media = setting(src, "media");
    let one = setting(src, "mode") == "media";
    if one && media.is_empty() {
        return Err("no post".into());
    }
    if !one && user.is_empty() {
        return Err("no Instagram account".into());
    }
    let fields = "id,text,timestamp,username,from{id,username}";
    let mut comments = Comments {
        since: HashMap::new(),
    };
    let mut token_at: Option<Instant> = None;
    let mut recent: Option<(Vec<String>, Instant)> = None;
    loop {
        if token_at.is_none_or(|t| t.elapsed() > TOKEN_EVERY) {
            check_token(hub, src, token).await?;
            let me = call(&format!("{}?fields=username", enc(user)), token).await;
            if let Some(name) = me
                .ok()
                .and_then(|v| v["username"].as_str().map(String::from))
            {
                hub.set_status(&src.id, |st| st.account = Some(format!("@{name}")));
            }
            token_at = Some(Instant::now());
        }
        let objects = if one {
            vec![media.to_string()]
        } else {
            if recent
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() > Duration::from_secs(60))
            {
                // The last posts of the account.
                let v = call(
                    &format!("{}/media?fields=id,timestamp&limit=3", enc(user)),
                    token,
                )
                .await?;
                let ids = v["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect();
                recent = Some((ids, Instant::now()));
            }
            recent
                .as_ref()
                .map(|(ids, _)| ids.clone())
                .unwrap_or_default()
        };
        for o in &objects {
            comments.read(hub, src, token, o, fields).await?;
        }
        connected(hub, src, objects.is_empty());
        // Instagram's quota is per hour: three posts every 12 s.
        tokio::time::sleep(EVERY * objects.len().max(1) as u32).await;
    }
}

/// The comments of a Graph `…/comments` page (Facebook or Instagram).
pub fn parse_comments(src: &SocialSource, page: &Value) -> Vec<Message> {
    let mut out = Vec::new();
    for c in page["data"].as_array().into_iter().flatten() {
        let Some(id) = c["id"].as_str() else {
            continue;
        };
        let from = &c["from"];
        let handle = c["username"]
            .as_str()
            .or(from["username"].as_str())
            .unwrap_or("");
        let author = Author {
            id: from["id"].as_str().unwrap_or(handle).to_string(),
            name: from["name"].as_str().unwrap_or(handle).to_string(),
            handle: handle.to_string(),
            avatar: from["picture"]["data"]["url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            ..Default::default()
        };
        let text = c["message"].as_str().or(c["text"].as_str()).unwrap_or("");
        let mut m = message(src, id, author, text);
        if let Some(photo) = c["attachment"]["media"]["image"]["src"].as_str() {
            m.photos.push(photo.to_string());
        }
        m.url = c["permalink_url"].as_str().unwrap_or("").to_string();
        out.push(m);
    }
    // Oldest first, as they came.
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::Platform;

    #[test]
    fn graph_comment_pages() {
        let fb = SocialSource {
            id: "fb".into(),
            name: "Facebook · Le Figaro".into(),
            platform: Platform::Facebook,
            enabled: true,
            settings: Default::default(),
        };
        let page: Value = serde_json::from_str(
            r#"{"data":[
              {"id":"1092_2201","message":"On peut revoir le reportage sur Brest ?","created_time":"2026-10-24T18:14:16+0000",
               "from":{"id":"5521","name":"Pierre Durand","picture":{"data":{"height":50,"is_silhouette":false,"url":"https://scontent.xx.fbcdn.net/p.jpg","width":50}}},
               "permalink_url":"https://www.facebook.com/lefigaro/videos/1092/?comment_id=2201"},
              {"id":"1092_2200","message":"Bravo pour ce direct","created_time":"2026-10-24T18:14:02+0000",
               "attachment":{"type":"photo","media":{"image":{"height":720,"src":"https://scontent.xx.fbcdn.net/i.jpg","width":1280}}}}
             ],
             "paging":{"cursors":{"before":"QVFI","after":"QVFJ"}}}"#,
        )
        .unwrap();
        let ms = parse_comments(&fb, &page);
        assert_eq!(ms.len(), 2);
        // Oldest first.
        assert_eq!(ms[0].id, "fb:1092_2200");
        assert_eq!(ms[0].photos, vec!["https://scontent.xx.fbcdn.net/i.jpg"]);
        assert_eq!(ms[1].author.name, "Pierre Durand");
        assert_eq!(ms[1].author.avatar, "https://scontent.xx.fbcdn.net/p.jpg");
        assert_eq!(ms[1].text, "On peut revoir le reportage sur Brest ?");
        assert!(ms[1].url.contains("comment_id=2201"));
        assert_eq!(unix("2026-10-24T18:14:16+0000"), Some(1_792_865_656));

        let ig = SocialSource {
            platform: Platform::Instagram,
            id: "ig".into(),
            ..fb
        };
        let page: Value = serde_json::from_str(
            r#"{"data":[{"id":"17890","text":"Superbe photo 😍","timestamp":"2026-10-24T18:10:00+0000","username":"julie.mrt","from":{"id":"178414","username":"julie.mrt"}}]}"#,
        )
        .unwrap();
        let ms = parse_comments(&ig, &page);
        assert_eq!(ms[0].author.name, "julie.mrt");
        assert_eq!(ms[0].author.handle, "julie.mrt");
        assert_eq!(ms[0].author.id, "178414");
        assert_eq!(ms[0].platform, Platform::Instagram);
    }
}
