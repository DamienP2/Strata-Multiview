//! YouTube, read with the Data API v3 and an API key: the live chat of a
//! given video or of the channel's live of the moment; a given video that
//! is not live gives its comments instead (the newest first, then the new
//! ones as they come).

use std::time::Duration;

use serde_json::Value;

use super::http::{enc, get_json};
use super::run::{connected, message, setting};
use super::{Author, Hub, Message, SocialSource};

const API: &str = "https://www.googleapis.com/youtube/v3";
/// Looking for a channel's live costs 100 of the 10 000 units of a day.
const SEARCH_EVERY: Duration = Duration::from_secs(15 * 60);
/// The comments of a video are read again (1 unit).
const COMMENTS_EVERY: Duration = Duration::from_secs(45);
/// Comments read at the first pass of a video (the newest).
const FIRST_COMMENTS: usize = 20;

/// The video id from an id or a link (watch?v=, youtu.be/, /live/).
pub fn video_of(s: &str) -> String {
    let s = s.trim();
    if let Some(v) = s.split(['?', '&']).find_map(|p| p.strip_prefix("v=")) {
        return v.split(['&', '#']).next().unwrap_or(v).to_string();
    }
    for mark in ["youtu.be/", "/live/", "/shorts/", "/embed/"] {
        if let Some((_, rest)) = s.split_once(mark) {
            return rest.split(['?', '/', '#']).next().unwrap_or("").to_string();
        }
    }
    s.to_string()
}

async fn call(path: &str, key: &str) -> Result<Value, String> {
    let a = get_json(format!("{API}/{path}&key={}", enc(key)), None).await?;
    if a.ok() {
        Ok(a.body)
    } else {
        Err(a.error())
    }
}

/// The live chat of a video, and its channel's name.
async fn chat_of(video: &str, key: &str) -> Result<(Option<String>, String), String> {
    let v = call(
        &format!("videos?part=liveStreamingDetails,snippet&id={}", enc(video)),
        key,
    )
    .await?;
    let item = &v["items"][0];
    if item.is_null() {
        return Err(format!("no video {video}"));
    }
    let chat = item["liveStreamingDetails"]["activeLiveChatId"]
        .as_str()
        .map(String::from);
    let account = item["snippet"]["channelTitle"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok((chat, account))
}

/// The channel id of `UC…`, `@handle` or a channel link.
async fn channel_id(channel: &str, key: &str) -> Result<String, String> {
    let c = channel.trim().trim_end_matches('/');
    let c = c.rsplit('/').next().unwrap_or(c);
    if c.starts_with("UC") && c.len() == 24 {
        return Ok(c.to_string());
    }
    let handle = format!("@{}", c.trim_start_matches('@'));
    let v = call(&format!("channels?part=id&forHandle={}", enc(&handle)), key).await?;
    v["items"][0]["id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("no channel {handle}"))
}

pub async fn run(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    let key = setting(src, "apiKey");
    if key.is_empty() {
        return Err("no API key".into());
    }
    let by_video = setting(src, "mode") == "video";
    let mut channel = None;
    loop {
        // The video whose chat to read.
        let video = if by_video {
            let v = video_of(setting(src, "video"));
            if v.is_empty() {
                return Err("no video".into());
            }
            v
        } else {
            if channel.is_none() {
                let c = setting(src, "channel");
                if c.is_empty() {
                    return Err("no channel".into());
                }
                channel = Some(channel_id(c, key).await?);
            }
            let id = channel.as_deref().unwrap_or_default();
            let found = call(
                &format!(
                    "search?part=id&channelId={}&eventType=live&type=video&maxResults=1",
                    enc(id)
                ),
                key,
            )
            .await?;
            match found["items"][0]["id"]["videoId"].as_str() {
                Some(v) => v.to_string(),
                None => {
                    connected(hub, src, true);
                    tokio::time::sleep(SEARCH_EVERY).await;
                    continue;
                }
            }
        };
        let (chat, account) = chat_of(&video, key).await?;
        hub.set_status(&src.id, |st| {
            st.account = Some(account).filter(|a| !a.is_empty())
        });
        let Some(chat) = chat else {
            if by_video {
                // Not a live: its comments, until a live chat opens on it.
                comments(hub, src, key, &video).await?;
            } else {
                connected(hub, src, true);
                tokio::time::sleep(SEARCH_EVERY).await;
            }
            continue;
        };
        connected(hub, src, false);
        poll(hub, src, key, &chat, &video).await?;
        // The live ended: look for the next one.
        connected(hub, src, true);
    }
}

/// Reads a live chat until it ends.
async fn poll(
    hub: &Hub,
    src: &SocialSource,
    key: &str,
    chat: &str,
    video: &str,
) -> Result<(), String> {
    let read = Read::of(setting(src, "read"));
    let mut page = String::new();
    loop {
        let mut path = format!(
            "liveChat/messages?part=snippet,authorDetails&maxResults=2000&liveChatId={}",
            enc(chat)
        );
        if !page.is_empty() {
            path.push_str(&format!("&pageToken={}", enc(&page)));
        }
        let a = get_json(format!("{API}/{path}&key={}", enc(key)), None).await?;
        if !a.ok() {
            let reason = a.body["error"]["errors"][0]["reason"]
                .as_str()
                .unwrap_or("");
            if matches!(
                reason,
                "liveChatEnded" | "liveChatNotFound" | "liveChatDisabled"
            ) {
                return Ok(());
            }
            return Err(a.error());
        }
        connected(hub, src, false);
        for m in parse(src, &a.body, video, &read) {
            hub.publish(m);
        }
        if a.body["offlineAt"].is_string() {
            return Ok(());
        }
        page = a.body["nextPageToken"].as_str().unwrap_or("").to_string();
        let wait = a.body["pollingIntervalMillis"].as_u64().unwrap_or(5000);
        tokio::time::sleep(Duration::from_millis(wait.clamp(2000, 60_000))).await;
    }
}

/// Reads the comments of a video (not live) as they come, until a live chat
/// opens on it: the newest at the first pass, then the ones newer than the
/// last read.
async fn comments(hub: &Hub, src: &SocialSource, key: &str, video: &str) -> Result<(), String> {
    let mut seen: Option<String> = None;
    loop {
        let v = call(
            &format!(
                "commentThreads?part=snippet&order=time&maxResults=100&textFormat=plainText&videoId={}",
                enc(video)
            ),
            key,
        )
        .await?;
        connected(hub, src, false);
        let (list, newest) = parse_comments(src, &v, video, seen.as_deref());
        let list = if seen.is_none() {
            list.into_iter().take(FIRST_COMMENTS).collect()
        } else {
            list
        };
        // Oldest first: the wall shows them in the order they came.
        for m in list.into_iter().rev() {
            hub.publish(m);
        }
        if newest.is_some() {
            seen = newest;
        } else if seen.is_none() {
            seen = Some(String::new());
        }
        tokio::time::sleep(COMMENTS_EVERY).await;
        // A live started on the video: its chat takes over.
        if chat_of(video, key).await?.0.is_some() {
            return Ok(());
        }
    }
}

/// The comments of a `commentThreads` page newer than `after` (a
/// `publishedAt`), newest first, and the newest `publishedAt` of the page.
pub fn parse_comments(
    src: &SocialSource,
    page: &Value,
    video: &str,
    after: Option<&str>,
) -> (Vec<Message>, Option<String>) {
    let mut out = Vec::new();
    let mut newest: Option<String> = None;
    for item in page["items"].as_array().into_iter().flatten() {
        let sn = &item["snippet"]["topLevelComment"]["snippet"];
        let at = sn["publishedAt"].as_str().unwrap_or("");
        if newest.as_deref().is_none_or(|n| at > n) {
            newest = Some(at.to_string());
        }
        if after.is_some_and(|a| at <= a) {
            continue;
        }
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        let text = sn["textOriginal"]
            .as_str()
            .or_else(|| sn["textDisplay"].as_str())
            .unwrap_or("");
        let author = Author {
            id: sn["authorChannelId"]["value"].as_str().unwrap_or("").into(),
            name: sn["authorDisplayName"]
                .as_str()
                .unwrap_or("")
                .trim_start_matches('@')
                .into(),
            avatar: sn["authorProfileImageUrl"].as_str().unwrap_or("").into(),
            ..Default::default()
        };
        let mut m = message(src, id, author, text);
        m.url = format!("https://www.youtube.com/watch?v={video}&lc={id}");
        out.push(m);
    }
    (out, newest)
}

/// What is read (`settings.read`: chat, superChats, members, polls).
pub struct Read {
    chat: bool,
    super_chats: bool,
    members: bool,
    polls: bool,
}

impl Read {
    pub fn of(s: &str) -> Self {
        if s.trim().is_empty() {
            return Self {
                chat: true,
                super_chats: true,
                members: false,
                polls: false,
            };
        }
        let has = |k| s.split(',').any(|x| x.trim() == k);
        Self {
            chat: has("chat"),
            super_chats: has("superChats"),
            members: has("members"),
            polls: has("polls"),
        }
    }
}

/// The messages of a `liveChat/messages` page.
pub fn parse(src: &SocialSource, page: &Value, video: &str, read: &Read) -> Vec<Message> {
    let mut out = Vec::new();
    for item in page["items"].as_array().into_iter().flatten() {
        let sn = &item["snippet"];
        let kind = sn["type"].as_str().unwrap_or("");
        let (wanted, amount, text) = match kind {
            "textMessageEvent" => (
                read.chat,
                "",
                sn["textMessageDetails"]["messageText"].as_str(),
            ),
            "superChatEvent" => (
                read.super_chats,
                sn["superChatDetails"]["amountDisplayString"]
                    .as_str()
                    .unwrap_or(""),
                sn["superChatDetails"]["userComment"].as_str(),
            ),
            "superStickerEvent" => (
                read.super_chats,
                sn["superStickerDetails"]["amountDisplayString"]
                    .as_str()
                    .unwrap_or(""),
                None,
            ),
            "newSponsorEvent"
            | "memberMilestoneChatEvent"
            | "membershipGiftingEvent"
            | "giftMembershipReceivedEvent" => (read.members, "", None),
            "pollEvent" => (
                read.polls,
                "",
                sn["pollDetails"]["metadata"]["questionText"].as_str(),
            ),
            _ => (false, "", None),
        };
        if !wanted {
            continue;
        }
        let text = text
            .filter(|t| !t.is_empty())
            .or_else(|| sn["displayMessage"].as_str())
            .unwrap_or("");
        let a = &item["authorDetails"];
        let author = Author {
            id: a["channelId"].as_str().unwrap_or("").into(),
            name: a["displayName"].as_str().unwrap_or("").into(),
            avatar: a["profileImageUrl"].as_str().unwrap_or("").into(),
            verified: a["isVerified"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        let mut m = message(src, id, author, text);
        m.amount = amount.to_string();
        m.url = format!("https://www.youtube.com/watch?v={video}");
        out.push(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::Platform;

    const PAGE: &str = r#"{
      "kind": "youtube#liveChatMessageListResponse",
      "etag": "x",
      "pollingIntervalMillis": 3200,
      "nextPageToken": "GO3ruKyR2IkDIOHZ7Y",
      "pageInfo": {"totalResults": 3, "resultsPerPage": 3},
      "items": [
        {"kind": "youtube#liveChatMessage", "id": "LCC.Cg8KDQoLeDhLcTAwMDAwMDAa",
         "snippet": {"type": "textMessageEvent", "liveChatId": "Cg0K", "authorChannelId": "UCa1",
           "publishedAt": "2026-10-24T18:14:22.115Z", "hasDisplayContent": true,
           "displayMessage": "Pourquoi la réforme ne concerne pas les indépendants ?",
           "textMessageDetails": {"messageText": "Pourquoi la réforme ne concerne pas les indépendants ?"}},
         "authorDetails": {"channelId": "UCa1", "channelUrl": "http://www.youtube.com/channel/UCa1",
           "displayName": "Sophie L.", "profileImageUrl": "https://yt3.ggpht.com/a.jpg",
           "isVerified": false, "isChatOwner": false, "isChatSponsor": false, "isChatModerator": false}},
        {"kind": "youtube#liveChatMessage", "id": "LCC.super1",
         "snippet": {"type": "superChatEvent", "displayMessage": "5,00 € from Lina: \"Bravo à toute l'équipe\"",
           "superChatDetails": {"amountMicros": "5000000", "currency": "EUR", "amountDisplayString": "5,00 €",
             "userComment": "Bravo à toute l'équipe", "tier": 2}},
         "authorDetails": {"channelId": "UCb2", "displayName": "Lina", "profileImageUrl": "https://yt3.ggpht.com/b.jpg", "isVerified": true}},
        {"kind": "youtube#liveChatMessage", "id": "LCC.member1",
         "snippet": {"type": "newSponsorEvent", "displayMessage": "Bienvenue Yanis !"},
         "authorDetails": {"channelId": "UCc3", "displayName": "Yanis"}}
      ]
    }"#;

    #[test]
    fn a_live_chat_page_with_a_super_chat() {
        let src = SocialSource {
            id: "yt".into(),
            name: "YouTube · JT 20H".into(),
            platform: Platform::Youtube,
            enabled: true,
            settings: Default::default(),
        };
        let page: Value = serde_json::from_str(PAGE).unwrap();
        let ms = parse(&src, &page, "x8Kq", &Read::of(""));
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].id, "yt:LCC.Cg8KDQoLeDhLcTAwMDAwMDAa");
        assert_eq!(ms[0].author.name, "Sophie L.");
        assert_eq!(ms[0].author.avatar, "https://yt3.ggpht.com/a.jpg");
        assert_eq!(
            ms[0].text,
            "Pourquoi la réforme ne concerne pas les indépendants ?"
        );
        assert_eq!(ms[1].amount, "5,00 €");
        assert_eq!(ms[1].text, "Bravo à toute l'équipe");
        assert!(ms[1].author.verified);
        assert_eq!(ms[1].url, "https://www.youtube.com/watch?v=x8Kq");
        let all = parse(&src, &page, "x8Kq", &Read::of("chat,superChats,members"));
        assert_eq!(all[2].text, "Bienvenue Yanis !");
        let supers = parse(&src, &page, "x8Kq", &Read::of("superChats"));
        assert_eq!(supers.len(), 1);
    }

    #[test]
    fn comments_of_a_video_newer_than_the_last_read() {
        let src = SocialSource {
            id: "yt".into(),
            name: "YouTube".into(),
            platform: Platform::Youtube,
            enabled: true,
            settings: Default::default(),
        };
        let page: Value = serde_json::json!({
          "items": [
            {"id": "c2", "snippet": {"topLevelComment": {"snippet": {
               "textOriginal": "Et pour les retraités ?", "authorDisplayName": "@martine",
               "authorProfileImageUrl": "https://yt3.ggpht.com/m.jpg",
               "authorChannelId": {"value": "UCm"}, "publishedAt": "2026-10-25T10:02:00Z"}}}},
            {"id": "c1", "snippet": {"topLevelComment": {"snippet": {
               "textDisplay": "Merci pour ce JT", "authorDisplayName": "Marc B.",
               "authorChannelId": {"value": "UCb"}, "publishedAt": "2026-10-25T10:01:00Z"}}}}
          ]
        });
        let (all, newest) = parse_comments(&src, &page, "x8Kq", None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "yt:c2");
        assert_eq!(all[0].author.name, "martine");
        assert_eq!(all[0].text, "Et pour les retraités ?");
        assert_eq!(all[0].url, "https://www.youtube.com/watch?v=x8Kq&lc=c2");
        assert_eq!(all[1].text, "Merci pour ce JT");
        assert_eq!(newest.as_deref(), Some("2026-10-25T10:02:00Z"));
        let (new, _) = parse_comments(&src, &page, "x8Kq", Some("2026-10-25T10:01:00Z"));
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].id, "yt:c2");
    }

    #[test]
    fn video_ids_from_links() {
        assert_eq!(
            video_of("https://www.youtube.com/watch?v=x8Kq123&t=4"),
            "x8Kq123"
        );
        assert_eq!(video_of("https://youtu.be/x8Kq123?si=a"), "x8Kq123");
        assert_eq!(
            video_of("https://www.youtube.com/live/x8Kq123?feature=share"),
            "x8Kq123"
        );
        assert_eq!(video_of("x8Kq123"), "x8Kq123");
    }
}
