//! X (Twitter) posts matching a search, read with the API v2 recent search
//! and a bearer token.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use super::http::{enc, get_json};
use super::run::{connected, message, setting};
use super::{Author, Hub, Message, SocialSource};

const SEARCH: &str = "https://api.x.com/2/tweets/search/recent";
/// The Basic plan allows 60 searches every 15 min.
const EVERY: Duration = Duration::from_secs(15);

pub async fn run(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    let bearer = setting(src, "bearer");
    if bearer.is_empty() {
        return Err("no bearer token".into());
    }
    let query = setting(src, "query");
    if query.is_empty() {
        return Err("no search".into());
    }
    let mut since = String::new();
    loop {
        let mut url = format!(
            "{SEARCH}?query={}&max_results={}&expansions=author_id,attachments.media_keys\
             &tweet.fields=created_at,attachments&user.fields=profile_image_url,verified,username,name\
             &media.fields=url,preview_image_url,type",
            enc(query),
            if since.is_empty() { 10 } else { 100 }
        );
        if !since.is_empty() {
            url.push_str(&format!("&since_id={since}"));
        }
        let a = get_json(url, Some(bearer.to_string())).await?;
        let until_reset = a
            .reset
            .map(|r| Duration::from_secs((r - crate::state::now_ms() / 1000).clamp(1, 900) as u64));
        if a.status == 429 {
            let wait = until_reset.unwrap_or(Duration::from_secs(60));
            hub.set_status(&src.id, |st| {
                st.warning = Some(format!("rate limited, {} s", wait.as_secs()))
            });
            tokio::time::sleep(wait).await;
            continue;
        }
        if !a.ok() {
            return Err(a.error());
        }
        hub.set_status(&src.id, |st| st.warning = None);
        connected(hub, src, false);
        if let Some(n) = a.body["meta"]["newest_id"].as_str() {
            since = n.to_string();
        }
        for m in parse(src, &a.body) {
            hub.publish(m);
        }
        // The last search of the window: wait for the next one.
        let wait = match (a.remaining, until_reset) {
            (Some(0), Some(r)) => r,
            _ => EVERY,
        };
        tokio::time::sleep(wait).await;
    }
}

/// The posts of a search page, oldest first.
pub fn parse(src: &SocialSource, page: &Value) -> Vec<Message> {
    let users: HashMap<&str, &Value> = page["includes"]["users"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| Some((u["id"].as_str()?, u)))
        .collect();
    let media: HashMap<&str, &Value> = page["includes"]["media"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["media_key"].as_str()?, m)))
        .collect();
    let mut out = Vec::new();
    for t in page["data"].as_array().into_iter().flatten() {
        let Some(id) = t["id"].as_str() else {
            continue;
        };
        let uid = t["author_id"].as_str().unwrap_or("");
        let u = users.get(uid).copied().unwrap_or(&Value::Null);
        let handle = u["username"].as_str().unwrap_or("");
        let author = Author {
            id: uid.to_string(),
            name: u["name"].as_str().unwrap_or(handle).to_string(),
            handle: handle.to_string(),
            avatar: u["profile_image_url"].as_str().unwrap_or("").to_string(),
            verified: u["verified"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        let mut m = message(src, id, author, t["text"].as_str().unwrap_or(""));
        for k in t["attachments"]["media_keys"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let pic = k
                .as_str()
                .and_then(|k| media.get(k))
                .and_then(|m| m["url"].as_str().or(m["preview_image_url"].as_str()));
            if let Some(p) = pic {
                m.photos.push(p.to_string());
            }
        }
        m.url = format!(
            "https://x.com/{}/status/{id}",
            if handle.is_empty() { "i" } else { handle }
        );
        out.push(m);
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::Platform;

    #[test]
    fn a_search_page_with_its_users() {
        let src = SocialSource {
            id: "x".into(),
            name: "X · #JT20H".into(),
            platform: Platform::X,
            enabled: true,
            settings: Default::default(),
        };
        let page: Value = serde_json::from_str(
            r#"{
              "data": [
                {"id": "1849500000000000002", "author_id": "2244994945", "text": "Le plateau du #JT20H ce soir 👇", "created_at": "2026-10-24T18:14:15.000Z",
                 "attachments": {"media_keys": ["3_1849"]}, "edit_history_tweet_ids": ["1849500000000000002"]},
                {"id": "1849500000000000001", "author_id": "99", "text": "Question pour le #JT20H : et les retraites ?", "created_at": "2026-10-24T18:14:01.000Z"}
              ],
              "includes": {
                "users": [
                  {"id": "2244994945", "name": "Le Figaro", "username": "Le_Figaro", "verified": true, "profile_image_url": "https://pbs.twimg.com/a_normal.jpg"},
                  {"id": "99", "name": "Anon", "username": "anon_8812"}
                ],
                "media": [{"media_key": "3_1849", "type": "photo", "url": "https://pbs.twimg.com/media/p.jpg"}]
              },
              "meta": {"newest_id": "1849500000000000002", "oldest_id": "1849500000000000001", "result_count": 2}
            }"#,
        )
        .unwrap();
        let ms = parse(&src, &page);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].author.handle, "anon_8812");
        assert_eq!(ms[1].id, "x:1849500000000000002");
        assert_eq!(ms[1].author.name, "Le Figaro");
        assert!(ms[1].author.verified);
        assert_eq!(ms[1].author.avatar, "https://pbs.twimg.com/a_normal.jpg");
        assert_eq!(ms[1].photos, vec!["https://pbs.twimg.com/media/p.jpg"]);
        assert_eq!(
            ms[1].url,
            "https://x.com/Le_Figaro/status/1849500000000000002"
        );
    }
}
