//! Twitch chat, read anonymously over IRC (no account needed).

use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use super::run::{connected, message, setting};
use super::{Author, Hub, Message, SocialSource};

const SERVER: &str = "irc.chat.twitch.tv:6667";
/// Twitch pings every ~5 min: silent longer than this, the link is dead.
const SILENCE: Duration = Duration::from_secs(400);

/// The channel's login from a name, `#name` or a twitch.tv link.
pub fn channel_of(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    let s = s.rsplit('/').next().unwrap_or(s);
    s.trim_start_matches(['#', '@']).to_ascii_lowercase()
}

pub async fn run(hub: &Hub, src: &SocialSource) -> Result<(), String> {
    let channel = channel_of(setting(src, "channel"));
    if channel.is_empty() {
        return Err("no channel".into());
    }
    let stream = tokio::time::timeout(Duration::from_secs(15), TcpStream::connect(SERVER))
        .await
        .map_err(|_| "cannot reach Twitch".to_string())?
        .map_err(|e| format!("cannot reach Twitch: {e}"))?;
    let (read, mut write) = stream.into_split();
    let nick = format!("justinfan{}", 10_000 + rand_u32() % 89_999);
    let hello =
        format!("CAP REQ :twitch.tv/tags twitch.tv/commands\r\nNICK {nick}\r\nJOIN #{channel}\r\n");
    write
        .write_all(hello.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(read).lines();
    loop {
        let line = match tokio::time::timeout(SILENCE, lines.next_line()).await {
            Err(_) => return Err("Twitch stopped answering".into()),
            Ok(Err(e)) => return Err(e.to_string()),
            Ok(Ok(None)) => return Err("Twitch closed the connection".into()),
            Ok(Ok(Some(l))) => l,
        };
        if let Some(rest) = line.strip_prefix("PING") {
            write
                .write_all(format!("PONG{rest}\r\n").as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            continue;
        }
        let cmd = command(&line);
        match cmd {
            // Joined: the channel's state comes first.
            "ROOMSTATE" | "366" => connected(hub, src, false),
            "RECONNECT" => return Ok(()),
            "NOTICE" if line.contains("msg_channel_suspended") => {
                return Err(format!("the channel #{channel} is suspended"))
            }
            "PRIVMSG" => {
                if let Some(m) = parse(src, &line) {
                    hub.publish(m);
                }
            }
            _ => {}
        }
    }
}

fn rand_u32() -> u32 {
    let b = uuid::Uuid::new_v4().into_bytes();
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// The command of an IRC line (after its tags and prefix).
fn command(line: &str) -> &str {
    let mut rest = line;
    if rest.starts_with('@') {
        rest = rest.split_once(' ').map_or("", |(_, r)| r);
    }
    if rest.starts_with(':') {
        rest = rest.split_once(' ').map_or("", |(_, r)| r);
    }
    rest.split(' ').next().unwrap_or("")
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => out.push(' '),
            Some(':') => out.push(';'),
            Some('\\') => out.push('\\'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// A chat message: `@tags :login!login@login.tmi.twitch.tv PRIVMSG #chan :text`.
pub fn parse(src: &SocialSource, line: &str) -> Option<Message> {
    let (tags, rest) = match line.strip_prefix('@') {
        Some(l) => l.split_once(' ')?,
        None => ("", line),
    };
    let tags: HashMap<&str, String> = tags
        .split(';')
        .filter_map(|t| t.split_once('='))
        .map(|(k, v)| (k, unescape(v)))
        .collect();
    let rest = rest.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let login = prefix.split('!').next().unwrap_or(prefix);
    let rest = rest.strip_prefix("PRIVMSG ")?;
    let (_, text) = rest.split_once(" :")?;
    // "/me" messages.
    let text = text
        .strip_prefix("\u{1}ACTION ")
        .map(|t| t.trim_end_matches('\u{1}'))
        .unwrap_or(text);
    let tag = |k: &str| tags.get(k).map(String::as_str).unwrap_or("");
    let name = match tag("display-name") {
        "" => login,
        n => n,
    };
    let id = match tag("id") {
        "" => format!("{login}-{}", tag("tmi-sent-ts")),
        id => id.to_string(),
    };
    let badges = tag("badges");
    let author = Author {
        id: match tag("user-id") {
            "" => login.to_string(),
            u => u.to_string(),
        },
        name: name.to_string(),
        handle: login.to_string(),
        verified: badges.contains("partner/") || badges.contains("verified/"),
        ..Default::default()
    };
    let mut m = message(src, &id, author, text);
    if let Ok(bits) = tag("bits").parse::<u32>() {
        m.amount = format!("{bits} bits");
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::Platform;

    fn src() -> SocialSource {
        SocialSource {
            id: "tw".into(),
            name: "Twitch · lefigaro_live".into(),
            platform: Platform::Twitch,
            enabled: true,
            settings: Default::default(),
        }
    }

    #[test]
    fn chat_lines_with_tags_and_bits() {
        let line = "@badge-info=subscriber/8;badges=subscriber/6,partner/1;bits=100;color=#1E90FF;display-name=Kevin_BZH;emotes=;first-msg=0;flags=;id=b34ccfc7-4977-403a-8a94-33c6bac34fb8;mod=0;room-id=1337;subscriber=1;tmi-sent-ts=1729800000000;turbo=0;user-id=123456;user-type= :kevin_bzh!kevin_bzh@kevin_bzh.tmi.twitch.tv PRIVMSG #lefigaro_live :Cheer100 Bravo\\sla régie ; top";
        assert_eq!(command(line), "PRIVMSG");
        let m = parse(&src(), line).unwrap();
        assert_eq!(m.id, "tw:b34ccfc7-4977-403a-8a94-33c6bac34fb8");
        assert_eq!(m.author.name, "Kevin_BZH");
        assert_eq!(m.author.handle, "kevin_bzh");
        assert_eq!(m.author.id, "123456");
        assert!(m.author.verified);
        assert_eq!(m.amount, "100 bits");
        // The text is not unescaped (only tags are).
        assert_eq!(m.text, "Cheer100 Bravo\\sla régie ; top");

        let me = "@display-name=Yanis\\sB;id=x1;user-id=9 :yanis!yanis@yanis.tmi.twitch.tv PRIVMSG #lefigaro_live :\u{1}ACTION applaudit\u{1}";
        let m = parse(&src(), me).unwrap();
        assert_eq!(m.author.name, "Yanis B");
        assert_eq!(m.text, "applaudit");
        assert!(m.amount.is_empty());

        assert_eq!(
            command(":tmi.twitch.tv 001 justinfan123 :Welcome, GLHF!"),
            "001"
        );
        assert_eq!(
            command("@emote-only=0;room-id=1 :tmi.twitch.tv ROOMSTATE #x"),
            "ROOMSTATE"
        );
        assert!(parse(&src(), ":tmi.twitch.tv 001 justinfan123 :Welcome").is_none());
        assert_eq!(
            channel_of("https://www.twitch.tv/LeFigaro_Live/"),
            "lefigaro_live"
        );
        assert_eq!(channel_of("#zerator"), "zerator");
    }
}
