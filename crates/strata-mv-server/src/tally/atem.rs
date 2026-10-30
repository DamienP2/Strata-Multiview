//! Blackmagic ATEM switchers (UDP 9910): the tally of every input and
//! their names, as the switcher tells its control panels.
//!
//! A packet: a 12-byte header (5 bits of flags and the length, the session,
//! the packet acknowledged, the packet's own number), then commands (their
//! length, 2 bytes, 4 letters, their data). The client says hello, the
//! switcher answers with the session and sends its whole state, then each
//! change; every packet asking for it is acknowledged, or the switcher
//! drops the client after a few seconds.

/// Header flags (the top 5 bits of the first word).
const ACK_REQUEST: u16 = 0x01;
const NEW_SESSION: u16 = 0x02;
const ACK_REPLY: u16 = 0x10;

pub const PORT: u16 = 9910;

/// What the switcher said.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The tally of the inputs: (input number, on air, next).
    Tally(Vec<(u16, bool, bool)>),
    /// An input's long name.
    Name(u16, String),
}

/// One connection to a switcher.
pub struct Session {
    id: u16,
    /// The last packet of the switcher handled (repeats are only acknowledged).
    last: Option<u16>,
    /// The switcher sends the tally by input number (`TlSr`): the one by
    /// position (`TlIn`) is then not needed.
    by_source: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: 0x53ab,
            last: None,
            by_source: false,
        }
    }
}

fn header(flags: u16, len: usize, session: u16, ack: u16) -> Vec<u8> {
    let mut h = vec![0u8; 12];
    h[0..2].copy_from_slice(&((flags << 11) | (len as u16 & 0x07ff)).to_be_bytes());
    h[2..4].copy_from_slice(&session.to_be_bytes());
    h[4..6].copy_from_slice(&ack.to_be_bytes());
    h
}

impl Session {
    /// The first packet: asks the switcher for a session.
    pub fn hello(&mut self) -> Vec<u8> {
        self.last = None;
        self.by_source = false;
        let mut p = header(NEW_SESSION, 20, self.id, 0);
        p[9] = 0x3a;
        p.extend([0x01, 0, 0, 0, 0, 0, 0, 0]);
        p
    }

    /// Handles a packet of the switcher: what to send back, what it said.
    pub fn on_packet(&mut self, p: &[u8]) -> (Vec<Vec<u8>>, Vec<Event>) {
        let (mut replies, mut events) = (Vec::new(), Vec::new());
        if p.len() < 12 {
            return (replies, events);
        }
        let word = u16::from_be_bytes([p[0], p[1]]);
        let flags = word >> 11;
        let len = ((word & 0x07ff) as usize).min(p.len());
        self.id = u16::from_be_bytes([p[2], p[3]]);
        let number = u16::from_be_bytes([p[10], p[11]]);
        if flags & NEW_SESSION != 0 {
            replies.push(header(ACK_REPLY, 12, self.id, number));
            return (replies, events);
        }
        if flags & ACK_REQUEST != 0 {
            replies.push(header(ACK_REPLY, 12, self.id, number));
            // A repeat (our acknowledgement was lost): already handled.
            if self.last == Some(number) {
                return (replies, events);
            }
            self.last = Some(number);
            self.commands(&p[12..len], &mut events);
        }
        (replies, events)
    }

    fn commands(&mut self, mut d: &[u8], events: &mut Vec<Event>) {
        while d.len() >= 8 {
            let n = u16::from_be_bytes([d[0], d[1]]) as usize;
            if n < 8 || n > d.len() {
                break;
            }
            let (name, data) = (&d[4..8], &d[8..n]);
            match name {
                b"TlSr" => {
                    self.by_source = true;
                    if let Some(t) = tally_by_source(data) {
                        events.push(Event::Tally(t));
                    }
                }
                b"TlIn" if !self.by_source => {
                    if let Some(t) = tally_by_index(data) {
                        events.push(Event::Tally(t));
                    }
                }
                b"InPr" if data.len() >= 22 => {
                    let id = u16::from_be_bytes([data[0], data[1]]);
                    let raw = &data[2..22];
                    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                    let name = String::from_utf8_lossy(&raw[..end]).trim().to_string();
                    events.push(Event::Name(id, name));
                }
                _ => {}
            }
            d = &d[n..];
        }
    }
}

/// `TlSr`: a count, then (input number, flags: 1 on air, 2 next) each.
fn tally_by_source(d: &[u8]) -> Option<Vec<(u16, bool, bool)>> {
    let n = u16::from_be_bytes([*d.first()?, *d.get(1)?]) as usize;
    Some(
        d.get(2..2 + 3 * n)?
            .chunks_exact(3)
            .map(|c| {
                (
                    u16::from_be_bytes([c[0], c[1]]),
                    c[2] & 1 != 0,
                    c[2] & 2 != 0,
                )
            })
            .collect(),
    )
}

/// `TlIn`: a count, then the flags of the inputs by position (input 1 first).
fn tally_by_index(d: &[u8]) -> Option<Vec<(u16, bool, bool)>> {
    let n = u16::from_be_bytes([*d.first()?, *d.get(1)?]) as usize;
    Some(
        d.get(2..2 + n)?
            .iter()
            .enumerate()
            .map(|(i, f)| (i as u16 + 1, f & 1 != 0, f & 2 != 0))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A packet of the switcher asking for an acknowledgement.
    fn packet(session: u16, number: u16, cmds: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, data) in cmds {
            body.extend(((data.len() + 8) as u16).to_be_bytes());
            body.extend([0, 0]);
            body.extend(*name);
            body.extend(data);
        }
        let mut p = header(ACK_REQUEST, 12 + body.len(), session, 0);
        p[10..12].copy_from_slice(&number.to_be_bytes());
        p.extend(body);
        p
    }

    #[test]
    fn the_handshake_is_acknowledged() {
        let mut s = Session::default();
        let hello = s.hello();
        assert_eq!(&hello[..2], &[0x10, 0x14]);
        assert_eq!(hello.len(), 20);
        // The switcher's answer: a new session.
        let mut answer = header(NEW_SESSION, 20, 0x53ab, 0);
        answer[10..12].copy_from_slice(&5u16.to_be_bytes());
        answer.extend([0u8; 8]);
        let (r, e) = s.on_packet(&answer);
        assert!(e.is_empty());
        assert_eq!(r[0][..2], [0x80, 0x0c]);
        assert_eq!(u16::from_be_bytes([r[0][4], r[0][5]]), 5);
    }

    #[test]
    fn tally_and_names_are_read() {
        let mut s = Session::default();
        let mut name = vec![0u8, 2];
        let mut long = b"Plateau".to_vec();
        long.resize(20, 0);
        name.extend(long);
        name.extend(b"PLT\0");
        let p = packet(
            0x8001,
            1,
            &[(b"TlIn", vec![0, 3, 1, 2, 0]), (b"InPr", name)],
        );
        let (r, e) = s.on_packet(&p);
        assert_eq!(r.len(), 1);
        assert_eq!(u16::from_be_bytes([r[0][2], r[0][3]]), 0x8001);
        assert_eq!(
            e,
            vec![
                Event::Tally(vec![(1, true, false), (2, false, true), (3, false, false)]),
                Event::Name(2, "Plateau".into()),
            ]
        );
        // The same packet again: acknowledged, not read twice.
        let (r, e) = s.on_packet(&p);
        assert_eq!((r.len(), e.len()), (1, 0));
        // By input number: preferred from then on.
        let p = packet(
            0x8001,
            2,
            &[(b"TlSr", vec![0, 1, 0, 4, 3]), (b"TlIn", vec![0, 1, 0])],
        );
        let (_, e) = s.on_packet(&p);
        assert_eq!(e, vec![Event::Tally(vec![(4, true, true)])]);
    }
}
