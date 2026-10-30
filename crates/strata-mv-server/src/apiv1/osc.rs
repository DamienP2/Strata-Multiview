//! OSC 1.0 over UDP: `/strata/{channel}/{action}/…` with the same words as
//! the HTTP paths; arguments are the values (`/strata/main/value/lower/name
//! "Marie Curie"`, or pairs `name "Marie" title "…"` for take and update).
//! No login: only when enabled in the settings, from the allowed machines.

use std::net::IpAddr;

use tokio::net::UdpSocket;

use super::{allowed, heard_osc, run_words, surface_actor, Extra};
use crate::state::Shared;

/// An OSC argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    Str(String),
    Blob(Vec<u8>),
    Bool(bool),
    Long(i64),
    Double(f64),
    Nil,
}

impl Arg {
    /// As a word of the command (`None`: no value).
    pub fn text(&self) -> Option<String> {
        Some(match self {
            Arg::Int(i) => i.to_string(),
            Arg::Long(i) => i.to_string(),
            Arg::Float(f) => f.to_string(),
            Arg::Double(f) => f.to_string(),
            Arg::Str(s) => s.clone(),
            Arg::Blob(b) => String::from_utf8_lossy(b).into_owned(),
            Arg::Bool(b) => b.to_string(),
            Arg::Nil => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub addr: String,
    pub args: Vec<Arg>,
}

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.buf.len())
            .ok_or("truncated packet")?;
        let out = &self.buf[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }
    /// A string: its bytes, a NUL, padded to 4 bytes.
    fn string(&mut self) -> Result<String, String> {
        let rest = &self.buf[self.at..];
        let len = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or("unterminated string")?;
        let s = std::str::from_utf8(&rest[..len])
            .map_err(|_| "string not UTF-8")?
            .to_string();
        self.take((len + 4) & !3)?;
        Ok(s)
    }
    fn blob(&mut self) -> Result<Vec<u8>, String> {
        let n = self.u32()? as usize;
        let b = self.take(n)?.to_vec();
        self.take((4 - n % 4) % 4)?;
        Ok(b)
    }
}

fn message(buf: &[u8]) -> Result<Message, String> {
    let mut r = Reader { buf, at: 0 };
    let addr = r.string()?;
    if !addr.starts_with('/') {
        return Err("an OSC address starts with /".into());
    }
    // A message may come without type tags (old senders): no arguments.
    if r.at >= buf.len() {
        return Ok(Message { addr, args: vec![] });
    }
    let tags = r.string()?;
    let tags = tags.strip_prefix(',').ok_or("type tags start with ,")?;
    let mut args = Vec::new();
    for tag in tags.chars() {
        args.push(match tag {
            'i' => Arg::Int(r.u32()? as i32),
            'f' => Arg::Float(f32::from_bits(r.u32()?)),
            's' | 'S' => Arg::Str(r.string()?),
            'b' => Arg::Blob(r.blob()?),
            'h' => Arg::Long(r.u64()? as i64),
            'd' => Arg::Double(f64::from_bits(r.u64()?)),
            't' => Arg::Long(r.u64()? as i64),
            'c' => Arg::Str(char::from_u32(r.u32()?).unwrap_or('?').to_string()),
            'r' | 'm' => Arg::Int(r.u32()? as i32),
            'T' => Arg::Bool(true),
            'F' => Arg::Bool(false),
            'N' | 'I' => Arg::Nil,
            // Arrays: their arguments, flat.
            '[' | ']' => continue,
            other => return Err(format!("OSC type `{other}` not supported")),
        });
    }
    Ok(Message { addr, args })
}

/// The messages of a packet: one message, or a bundle (and the bundles it
/// holds), run at once whatever their time tag.
pub fn parse(buf: &[u8]) -> Result<Vec<Message>, String> {
    let mut out = Vec::new();
    parse_into(buf, &mut out, 0)?;
    Ok(out)
}

fn parse_into(buf: &[u8], out: &mut Vec<Message>, depth: u32) -> Result<(), String> {
    if depth > 8 {
        return Err("bundles nested too deep".into());
    }
    if buf.starts_with(b"#bundle\0") {
        let mut r = Reader { buf, at: 8 };
        r.u64()?; // time tag
        while r.at < buf.len() {
            let n = r.u32()? as usize;
            let element = r.take(n)?;
            parse_into(element, out, depth + 1)?;
        }
        return Ok(());
    }
    out.push(message(buf)?);
    Ok(())
}

/// The words of a command from an OSC address under `/strata`.
pub fn words(addr: &str) -> Option<Vec<String>> {
    let rest = addr.strip_prefix("/strata/")?;
    Some(
        rest.split('/')
            .filter(|s| !s.is_empty())
            .map(super::unescape)
            .collect(),
    )
}

pub async fn serve(s: Shared) {
    let cfg = s.config.control.clone();
    if !cfg.osc_enabled {
        return;
    }
    let sock = match UdpSocket::bind(("0.0.0.0", cfg.osc_port)).await {
        Ok(sock) => sock,
        Err(e) => {
            tracing::error!(port = cfg.osc_port, "OSC: cannot listen: {e}");
            return;
        }
    };
    tracing::info!(port = cfg.osc_port, "OSC control listening");
    let mut buf = vec![0u8; 65_536];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf).await else {
            continue;
        };
        let ip: IpAddr = from.ip().to_canonical();
        if !allowed(ip, &cfg.allow) {
            tracing::debug!(%ip, "OSC: not an allowed address");
            continue;
        }
        let msgs = match parse(&buf[..n]) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(%ip, "OSC: {e}");
                continue;
            }
        };
        heard_osc(ip);
        let actor = surface_actor("OSC", ip);
        for m in msgs {
            let Some(w) = words(&m.addr) else {
                tracing::debug!(%ip, addr = m.addr, "OSC: not under /strata");
                continue;
            };
            let extra = Extra {
                positional: m.args.iter().filter_map(Arg::text).collect(),
                ..Default::default()
            };
            match run_words(&s, &w, extra, &actor).await {
                Ok(done) => tracing::debug!(%ip, "OSC: {done}"),
                Err(e) => tracing::warn!(%ip, addr = m.addr, "OSC: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    fn encode(addr: &str, args: &[Arg]) -> Vec<u8> {
        let mut out = Vec::new();
        pad(&mut out, addr);
        let mut tags = String::from(",");
        let mut data = Vec::new();
        for a in args {
            match a {
                Arg::Int(i) => {
                    tags.push('i');
                    data.extend_from_slice(&i.to_be_bytes());
                }
                Arg::Float(f) => {
                    tags.push('f');
                    data.extend_from_slice(&f.to_bits().to_be_bytes());
                }
                Arg::Str(s) => {
                    tags.push('s');
                    pad(&mut data, s);
                }
                Arg::Bool(b) => tags.push(if *b { 'T' } else { 'F' }),
                _ => unreachable!(),
            }
        }
        pad(&mut out, &tags);
        out.extend(data);
        out
    }

    #[test]
    fn apiv1_osc_messages() {
        let p = encode(
            "/strata/main/value/lower/name",
            &[Arg::Str("Marie Curie".into())],
        );
        let m = parse(&p).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].addr, "/strata/main/value/lower/name");
        assert_eq!(m[0].args, vec![Arg::Str("Marie Curie".into())]);
        assert_eq!(
            words(&m[0].addr).unwrap(),
            ["main", "value", "lower", "name"]
        );

        let p = encode(
            "/strata/main/next",
            &[
                Arg::Int(-3),
                Arg::Float(0.5),
                Arg::Bool(true),
                Arg::Str("abcd".into()),
            ],
        );
        let m = parse(&p).unwrap();
        assert_eq!(
            m[0].args,
            vec![
                Arg::Int(-3),
                Arg::Float(0.5),
                Arg::Bool(true),
                Arg::Str("abcd".into())
            ]
        );
        let text: Vec<_> = m[0].args.iter().filter_map(Arg::text).collect();
        assert_eq!(text, ["-3", "0.5", "true", "abcd"]);
        assert_eq!(Arg::Float(3.0).text().unwrap(), "3");

        // No type tags at all.
        let mut bare = Vec::new();
        pad(&mut bare, "/strata/main/undo");
        assert_eq!(parse(&bare).unwrap()[0].args, vec![]);
        assert!(words("/other/main/next").is_none());
    }

    #[test]
    fn apiv1_osc_bundles_and_bad_packets() {
        let a = encode("/strata/main/take/1", &[]);
        let b = encode("/strata/main/out/lower", &[Arg::Int(1)]);
        let mut bundle = b"#bundle\0".to_vec();
        bundle.extend_from_slice(&1u64.to_be_bytes());
        for e in [&a, &b] {
            bundle.extend_from_slice(&(e.len() as u32).to_be_bytes());
            bundle.extend_from_slice(e);
        }
        let m = parse(&bundle).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].addr, "/strata/main/take/1");
        assert_eq!(m[1].args, vec![Arg::Int(1)]);

        // Truncated or wrong: an error, never a panic.
        let good = encode("/strata/main/value/lower/name", &[Arg::Str("x".into())]);
        for n in 0..good.len() {
            let _ = parse(&good[..n]);
        }
        assert!(parse(b"nope\0\0\0\0").is_err());
        assert!(parse(&good[..good.len() - 2]).is_err());
        let mut lying = b"#bundle\0".to_vec();
        lying.extend_from_slice(&0u64.to_be_bytes());
        lying.extend_from_slice(&1000u32.to_be_bytes());
        assert!(parse(&lying).is_err());
    }
}
