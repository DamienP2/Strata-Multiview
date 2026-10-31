//! TSL UMD protocols, as switchers and tally controllers send them (Ross,
//! Grass Valley, Sony, Panasonic, vMix, Blackmagic through a converter…):
//!
//! - **3.1**: 18 bytes a display: `0x80 + address`, a control byte (tally 1
//!   to 4 in bits 0 to 3, brightness in bits 4-5), 16 characters of text.
//!   Several in one UDP datagram, or one after another on TCP.
//! - **5.0**: a packet (`PBC` its length, version, flags, screen) of display
//!   messages: index, control (right, text and left tally: 0 off, 1 red,
//!   2 green, 3 amber), the text (ASCII, or UTF-16LE when flagged). On TCP
//!   each packet comes after `DLE STX` (0xFE 0x02), a 0xFE in it doubled.

use super::{Lamp, Lamps};

/// What a display of the switcher says.
#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    /// TSL 3.1 address (0–126) or TSL 5 index.
    pub index: u32,
    pub lamps: Lamps,
    /// The UMD text, trimmed (None: the message carried no text).
    pub text: Option<String>,
}

/// The TSL 3.1 messages of a datagram (or of a TCP read, the bytes of an
/// incomplete one left in `rest`). `map` gives the lamp of tally 1 to 4.
pub fn parse_v31(bytes: &[u8], map: &[Lamp; 4], rest: &mut Vec<u8>) -> Vec<Display> {
    rest.extend_from_slice(bytes);
    let mut out = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        // A message starts with its address byte (high bit set).
        if rest[i] & 0x80 == 0 {
            i += 1;
            continue;
        }
        if rest.len() - i < 18 {
            break;
        }
        let m = &rest[i..i + 18];
        let control = m[1];
        let mut lamps = Lamps::default();
        for (bit, lamp) in map.iter().enumerate() {
            if control & (1 << bit) != 0 {
                lamps.light(*lamp);
            }
        }
        let text: String = m[2..]
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect();
        out.push(Display {
            index: (m[0] & 0x7f) as u32,
            lamps,
            text: Some(text.trim().to_string()),
        });
        i += 18;
    }
    rest.drain(..i.min(rest.len()));
    out
}

/// A TSL 5 tally colour as a lamp.
fn colour(v: u16) -> Option<Lamp> {
    match v & 3 {
        1 => Some(Lamp::Program),
        2 => Some(Lamp::Preview),
        3 => Some(Lamp::Aux),
        _ => None,
    }
}

/// The display messages of one TSL 5 packet (the screen's, when `screen`
/// is set; 0xFFFF reaches every screen).
pub fn parse_v5(p: &[u8], screen: Option<u16>) -> Vec<Display> {
    let u16le =
        |at: usize| -> Option<u16> { Some(u16::from_le_bytes([*p.get(at)?, *p.get(at + 1)?])) };
    let Some(pbc) = u16le(0) else {
        return Vec::new();
    };
    let end = (2 + pbc as usize).min(p.len());
    let (Some(&_ver), Some(&flags), Some(scr)) = (p.get(2), p.get(3), u16le(4)) else {
        return Vec::new();
    };
    // Screen control data: no display message.
    if flags & 0x02 != 0 {
        return Vec::new();
    }
    if let Some(want) = screen {
        if scr != want && scr != 0xffff {
            return Vec::new();
        }
    }
    let unicode = flags & 0x01 != 0;
    let mut out = Vec::new();
    let mut at = 6;
    while at + 4 <= end {
        let (Some(index), Some(control)) = (u16le(at), u16le(at + 2)) else {
            break;
        };
        at += 4;
        if control & 0x8000 != 0 {
            // Control data: its length and bytes, skipped.
            let Some(len) = u16le(at) else { break };
            at += 2 + len as usize;
            continue;
        }
        let Some(len) = u16le(at) else { break };
        at += 2;
        let raw = &p[at.min(end)..(at + len as usize).min(end)];
        at += len as usize;
        let text = if unicode {
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            raw.iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        ' '
                    }
                })
                .collect()
        };
        let mut lamps = Lamps::default();
        for v in [control, control >> 2, control >> 4] {
            if let Some(l) = colour(v) {
                lamps.light(l);
            }
        }
        out.push(Display {
            index: index as u32,
            lamps,
            text: Some(text.trim().to_string()),
        });
    }
    out
}

/// TSL 5 over TCP: the packets of the bytes read so far (`DLE STX`, a
/// doubled `DLE` standing for one), an incomplete one left in `rest`.
pub fn unwrap_v5_tcp(bytes: &[u8], rest: &mut Vec<u8>) -> Vec<Vec<u8>> {
    rest.extend_from_slice(bytes);
    let mut packets = Vec::new();
    loop {
        // The start of the next packet.
        let Some(start) = rest.windows(2).position(|w| w == [0xfe, 0x02]) else {
            // Keep a last 0xFE: the STX may come next.
            let keep = usize::from(rest.last() == Some(&0xfe));
            let n = rest.len() - keep;
            rest.drain(..n);
            break;
        };
        // Unstuffed until the packet's own length is reached.
        let mut p = Vec::new();
        let mut i = start + 2;
        let mut done = None;
        while i < rest.len() {
            let b = rest[i];
            if b == 0xfe {
                match rest.get(i + 1) {
                    Some(0xfe) => {
                        p.push(0xfe);
                        i += 2;
                    }
                    // Another packet starts: this one was cut short.
                    Some(0x02) => {
                        done = Some((i, false));
                        break;
                    }
                    Some(_) => {
                        p.push(0xfe);
                        i += 1;
                    }
                    None => break,
                }
            } else {
                p.push(b);
                i += 1;
            }
            if p.len() >= 2 {
                let pbc = u16::from_le_bytes([p[0], p[1]]) as usize;
                if p.len() >= 2 + pbc {
                    done = Some((i, true));
                    break;
                }
            }
        }
        match done {
            Some((next, whole)) => {
                if whole {
                    packets.push(p);
                }
                rest.drain(..next);
            }
            None => {
                rest.drain(..start);
                break;
            }
        }
    }
    packets
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const MAP: [Lamp; 4] = [Lamp::Program, Lamp::Preview, Lamp::Aux, Lamp::Aux];

    pub fn v31(address: u8, control: u8, text: &str) -> Vec<u8> {
        let mut m = vec![0x80 | address, control];
        let mut t = text.as_bytes().to_vec();
        t.resize(16, b' ');
        m.extend(t);
        m
    }

    /// A TSL 5 packet of display messages (index, control, text).
    pub fn v5(screen: u16, msgs: &[(u16, u16, &str)]) -> Vec<u8> {
        let mut body = vec![0u8, 0];
        body.extend(screen.to_le_bytes());
        for (index, control, text) in msgs {
            body.extend(index.to_le_bytes());
            body.extend(control.to_le_bytes());
            body.extend((text.len() as u16).to_le_bytes());
            body.extend(text.as_bytes());
        }
        let mut p = (body.len() as u16).to_le_bytes().to_vec();
        p.extend(body);
        p
    }

    #[test]
    fn tsl_31_messages_are_read() {
        let mut rest = Vec::new();
        let mut d = v31(3, 0b0001, "CAM 3");
        d.extend(v31(4, 0b0010, "CAM 4"));
        let got = parse_v31(&d, &MAP, &mut rest);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].index, 3);
        assert!(got[0].lamps.program && !got[0].lamps.preview);
        assert_eq!(got[0].text.as_deref(), Some("CAM 3"));
        assert!(got[1].lamps.preview);
        // On TCP, a message cut in two.
        let m = v31(5, 0b0101, "PLATEAU");
        assert!(parse_v31(&m[..7], &MAP, &mut rest).is_empty());
        let got = parse_v31(&m[7..], &MAP, &mut rest);
        assert_eq!(got.len(), 1);
        assert!(got[0].lamps.program && got[0].lamps.aux);
        assert_eq!(got[0].text.as_deref(), Some("PLATEAU"));
        assert!(rest.is_empty());
    }

    #[test]
    fn tsl_5_messages_are_read() {
        // RH red (1), LH green (2 << 4), text off.
        let p = v5(0, &[(12, 1 | (2 << 4), "INVITÉ"), (13, 3 << 2, "CAM 13")]);
        let got = parse_v5(&p, None);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].index, 12);
        assert!(got[0].lamps.program && got[0].lamps.preview);
        assert!(got[1].lamps.aux && !got[1].lamps.program);
        assert_eq!(got[1].text.as_deref(), Some("CAM 13"));
        // Another screen: ignored.
        assert!(parse_v5(&v5(2, &[(1, 1, "X")]), Some(1)).is_empty());
        assert_eq!(parse_v5(&v5(0xffff, &[(1, 1, "X")]), Some(1)).len(), 1);
    }

    #[test]
    fn tsl_5_unicode_text() {
        let text: Vec<u8> = "Invité"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let mut body = vec![0u8, 0x01];
        body.extend(0u16.to_le_bytes());
        body.extend(7u16.to_le_bytes());
        body.extend(1u16.to_le_bytes());
        body.extend((text.len() as u16).to_le_bytes());
        body.extend(&text);
        let mut p = (body.len() as u16).to_le_bytes().to_vec();
        p.extend(body);
        let got = parse_v5(&p, None);
        assert_eq!(got[0].text.as_deref(), Some("Invité"));
        assert!(got[0].lamps.program);
    }

    #[test]
    fn tsl_5_over_tcp_is_unwrapped() {
        // A packet with a 0xFE in it (index 0xFE), doubled on the wire.
        let p = v5(0, &[(0xfe, 1, "A")]);
        let mut wire = vec![0xfe, 0x02];
        for b in &p {
            wire.push(*b);
            if *b == 0xfe {
                wire.push(0xfe);
            }
        }
        wire.extend([0xfe, 0x02]);
        let second = v5(0, &[(2, 2, "B")]);
        wire.extend(&second[..5]);
        let mut rest = Vec::new();
        let got = unwrap_v5_tcp(&wire, &mut rest);
        assert_eq!(got, vec![p]);
        let got = unwrap_v5_tcp(&second[5..], &mut rest);
        assert_eq!(got, vec![second]);
        assert_eq!(parse_v5(&got[0], None)[0].index, 2);
    }
}
