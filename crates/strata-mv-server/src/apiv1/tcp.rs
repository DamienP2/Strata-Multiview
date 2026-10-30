//! TCP text control: one command a line, the words of the HTTP paths
//! (`main take 2`, `main out lower`, `main value lower name "Marie Curie"`),
//! answered `OK …` or `ERR …`. `status [channel]` answers one JSON line,
//! `watch [channel]` sends a JSON line each time the status changes (and
//! commands still work), `unwatch` stops it. No login: only when enabled in
//! the settings, from the allowed machines.

use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::{allowed, channel_id, run_words, surface_actor, Connected, Extra, Watcher, ACTIONS};
use crate::state::Shared;

/// Longest line read.
const LINE_MAX: u64 = 64 * 1024;
/// Connections at the same time.
const CONNECTIONS_MAX: usize = 64;

/// The words of a line: spaces between words, "double" or 'single' quotes
/// around words with spaces; `\` keeps the next character as it is.
pub fn split(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => match chars.next() {
                Some(n) => word.push(n),
                None => return Err("a line ends with \\".into()),
            },
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c == '\\' => match chars.next() {
                Some(n) => {
                    word.push(n);
                    in_word = true;
                }
                None => return Err("a line ends with \\".into()),
            },
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err("a quote is not closed".into());
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// A line written as a path (`/main/take/2`, `/strata/main/next`): its
/// words, then the other words of the line.
fn path_words(words: Vec<String>) -> Vec<String> {
    match words.split_first() {
        Some((first, rest)) if first.starts_with('/') => {
            let path = first.strip_prefix("/strata").unwrap_or(first);
            path.split('/')
                .filter(|s| !s.is_empty())
                .map(super::unescape)
                .chain(rest.iter().cloned())
                .collect()
        }
        _ => words,
    }
}

pub async fn serve(s: Shared) {
    let cfg = s.config.control.clone();
    if !cfg.tcp_enabled {
        return;
    }
    let listener = match TcpListener::bind(("0.0.0.0", cfg.tcp_port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(port = cfg.tcp_port, "TCP control: cannot listen: {e}");
            return;
        }
    };
    tracing::info!(port = cfg.tcp_port, "TCP control listening");
    let open = Arc::new(AtomicUsize::new(0));
    loop {
        let Ok((sock, peer)) = listener.accept().await else {
            continue;
        };
        let ip = peer.ip().to_canonical();
        if !allowed(ip, &cfg.allow) || open.load(Ordering::Relaxed) >= CONNECTIONS_MAX {
            tracing::debug!(%ip, "TCP control: refused");
            continue;
        }
        open.fetch_add(1, Ordering::Relaxed);
        let (s, open) = (s.clone(), open.clone());
        tokio::spawn(async move {
            let closing = crate::state::until_true(s.closing.subscribe());
            tokio::select! {
                _ = session(&s, sock, ip) => {}
                _ = closing => {}
            }
            open.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

/// What a line asks.
enum Reply {
    Line(String),
    Watch(Option<String>),
    Unwatch,
    Quit,
}

async fn answer(s: &Shared, line: &str, ip: IpAddr) -> Reply {
    let words = match split(line) {
        Ok(w) => path_words(w),
        Err(e) => return Reply::Line(format!("ERR {e}")),
    };
    let Some(first) = words.first().map(|w| w.to_ascii_lowercase()) else {
        return Reply::Line(String::new());
    };
    let channel = |w: Option<&String>| match w {
        Some(c) => channel_id(s, c, |_| true).map(Some).map_err(|e| e.1),
        None => Ok(None),
    };
    match first.as_str() {
        "ping" => Reply::Line("OK pong".into()),
        "quit" | "exit" | "bye" => Reply::Quit,
        "help" => Reply::Line(format!(
            "OK <channel> <{}> … | status [channel] | watch [channel] | unwatch | ping | quit",
            ACTIONS.replace(", ", "|")
        )),
        "status" => match channel(words.get(1)) {
            Ok(only) => {
                let st = super::status::status(s, None, only.as_deref()).await;
                Reply::Line(serde_json::to_string(&st).unwrap_or_default())
            }
            Err(e) => Reply::Line(format!("ERR {e}")),
        },
        "watch" => match channel(words.get(1)) {
            Ok(only) => Reply::Watch(only),
            Err(e) => Reply::Line(format!("ERR {e}")),
        },
        "unwatch" => Reply::Unwatch,
        _ => {
            let actor = surface_actor("TCP", ip);
            match run_words(s, &words, Extra::default(), &actor).await {
                Ok(done) => Reply::Line(format!("OK {done}")),
                Err(e) => Reply::Line(format!("ERR {}", e.replace(['\r', '\n'], " "))),
            }
        }
    }
}

async fn session(s: &Shared, sock: TcpStream, ip: IpAddr) {
    let _connected = Connected::new();
    let (rd, mut wr) = sock.into_split();
    // Lines are read on their own (reading a line cannot be cut short by
    // a status to send).
    let (tx, mut lines) = tokio::sync::mpsc::channel::<Option<String>>(16);
    let reader = tokio::spawn(async move {
        let mut rd = BufReader::new(rd);
        loop {
            let mut line = String::new();
            let read = (&mut rd).take(LINE_MAX).read_line(&mut line).await;
            let msg = match read {
                Ok(0) | Err(_) => break,
                Ok(_) if !line.ends_with('\n') && line.len() as u64 >= LINE_MAX => None,
                Ok(_) => Some(line),
            };
            let last = msg.is_none();
            if tx.send(msg).await.is_err() || last {
                break;
            }
        }
    });
    let mut watcher: Option<Watcher> = None;
    loop {
        let reply = tokio::select! {
            line = lines.recv() => match line {
                Some(Some(line)) => answer(s, line.trim(), ip).await,
                Some(None) => {
                    let _ = wr.write_all(b"ERR line too long\n").await;
                    break;
                }
                None => break,
            },
            json = async { watcher.as_mut().expect("watching").next().await }, if watcher.is_some() => match json {
                Some(json) => Reply::Line(json),
                None => break,
            },
        };
        let out = match reply {
            Reply::Line(l) if l.is_empty() => continue,
            Reply::Line(l) => l,
            Reply::Watch(only) => {
                watcher = Some(Watcher::new(s, None, only));
                "OK watching".into()
            }
            Reply::Unwatch => {
                watcher = None;
                "OK".into()
            }
            Reply::Quit => {
                let _ = wr.write_all(b"OK bye\n").await;
                break;
            }
        };
        if wr.write_all(format!("{out}\n").as_bytes()).await.is_err() {
            break;
        }
    }
    reader.abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apiv1_tcp_lines() {
        assert_eq!(split("main take 2").unwrap(), ["main", "take", "2"]);
        assert_eq!(
            split(r#"  main value lower name "Marie Curie"  "#).unwrap(),
            ["main", "value", "lower", "name", "Marie Curie"]
        );
        assert_eq!(
            split(r#"main take 1 name="Ada \"the\" first" title='L\x'"#).unwrap(),
            ["main", "take", "1", r#"name=Ada "the" first"#, r"title=L\x"]
        );
        assert_eq!(
            split(r#"main value lower name """#).unwrap(),
            ["main", "value", "lower", "name", ""]
        );
        assert!(split(r#"main value lower name "Marie"#).is_err());
        assert!(split("").unwrap().is_empty());
        assert_eq!(split(r"Channel\ 1 next").unwrap(), ["Channel 1", "next"]);
        assert_eq!(
            path_words(split("/strata/main/value/lower/name 'Marie Curie'").unwrap()),
            ["main", "value", "lower", "name", "Marie Curie"]
        );
        assert_eq!(path_words(split("/main/next").unwrap()), ["main", "next"]);
    }
}
