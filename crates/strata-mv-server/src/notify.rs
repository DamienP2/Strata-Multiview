//! Telling a supervision about the faults: SNMP v2c traps (STRATA-MV-MIB,
//! docs/STRATA-MV-MIB.txt) and syslog lines (RFC 5424, over UDP or TCP with
//! octet counting, RFC 6587), for the alarms and the audit journal.
//!
//! The traps are encoded here (BER): a few objects, no SNMP library needed.

use std::io::Write as _;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use strata_mv_core::alarm::{SnmpTarget, SyslogTarget, SNMP_BASE_OID};
use strata_mv_core::signal::Severity;

use crate::security::AuditEntry;
use crate::signals::AlarmEvent;

/// When the server started, for sysUpTime.
fn started() -> Instant {
    static T: OnceLock<Instant> = OnceLock::new();
    *T.get_or_init(Instant::now)
}

pub fn init() {
    started();
}

fn hostname() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .split('.')
        .next()
        .unwrap_or("strata")
        .to_string()
}

/// "host", "host:port", "[v6]:port" → the first address.
pub fn resolve(host: &str, default_port: u16) -> Result<SocketAddr, String> {
    let h = host.trim();
    if h.is_empty() {
        return Err("adresse vide".into());
    }
    let with_port = if h.parse::<SocketAddr>().is_ok()
        || (h.matches(':').count() == 1 && !h.starts_with('['))
        || (h.starts_with('[') && h.contains("]:"))
    {
        h.to_string()
    } else if h.contains(':') && !h.starts_with('[') {
        format!("[{h}]:{default_port}") // a bare IPv6 address
    } else {
        format!("{h}:{default_port}")
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("{h} : {e}"))?
        .next()
        .ok_or_else(|| format!("{h} : adresse introuvable"))
}

// ----------------------------------------------------------------- BER --

fn len(out: &mut Vec<u8>, n: usize) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.extend([0x81, n as u8]);
    } else {
        out.extend([0x82, (n >> 8) as u8, n as u8]);
    }
}

fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    len(&mut out, body.len());
    out.extend_from_slice(body);
    out
}

/// A two's complement integer, as short as it goes.
fn int(tag: u8, v: i64) -> Vec<u8> {
    let b = v.to_be_bytes();
    let mut i = 0;
    while i < 7 && ((b[i] == 0 && b[i + 1] & 0x80 == 0) || (b[i] == 0xff && b[i + 1] & 0x80 != 0)) {
        i += 1;
    }
    tlv(tag, &b[i..])
}

fn oid(dotted: &str) -> Vec<u8> {
    let arcs: Vec<u64> = dotted.split('.').filter_map(|a| a.parse().ok()).collect();
    let mut body = Vec::new();
    if arcs.len() >= 2 {
        let mut push = |mut v: u64| {
            let mut tmp = vec![(v & 0x7f) as u8];
            v >>= 7;
            while v > 0 {
                tmp.push(0x80 | (v & 0x7f) as u8);
                v >>= 7;
            }
            tmp.reverse();
            body.extend(tmp);
        };
        push(arcs[0] * 40 + arcs[1]);
        for &a in &arcs[2..] {
            push(a);
        }
    }
    tlv(0x06, &body)
}

fn octets(s: &str) -> Vec<u8> {
    tlv(0x04, s.as_bytes())
}

fn varbind(name: &str, value: Vec<u8>) -> Vec<u8> {
    let mut b = oid(name);
    b.extend(value);
    tlv(0x30, &b)
}

/// A valid base OID, or the default one.
pub fn base_oid(set: &str) -> String {
    let s = set.trim().trim_start_matches('.');
    let ok = s.split('.').count() >= 3
        && s.split('.')
            .all(|a| !a.is_empty() && a.parse::<u64>().is_ok());
    if ok {
        s.to_string()
    } else {
        SNMP_BASE_OID.to_string()
    }
}

/// The SNMP v2c trap of an alarm event (see docs/STRATA-MV-MIB.txt):
/// notification `base.0.{1 raised, 2 ended, 3 acked}` with the objects
/// `base.1.{1 input, 2 inputId, 3 kind, 4 severity, 5 text, 6 server, 7 state}`.
pub fn trap(
    community: &str,
    base: &str,
    request_id: i32,
    uptime_cs: u32,
    ev: &AlarmEvent,
) -> Vec<u8> {
    let n = match ev.state {
        "raised" => 1,
        "ended" => 2,
        _ => 3,
    };
    let kind = kind_name(ev);
    let sev = match ev.severity {
        Severity::Warning => 1,
        Severity::Critical => 2,
    };
    let mut binds = Vec::new();
    binds.extend(varbind("1.3.6.1.2.1.1.3.0", int(0x43, uptime_cs as i64)));
    binds.extend(varbind(
        "1.3.6.1.6.3.1.1.4.1.0",
        oid(&format!("{base}.0.{n}")),
    ));
    binds.extend(varbind(&format!("{base}.1.1"), octets(&ev.input)));
    binds.extend(varbind(&format!("{base}.1.2"), octets(&ev.input_id)));
    binds.extend(varbind(&format!("{base}.1.3"), octets(&kind)));
    binds.extend(varbind(&format!("{base}.1.4"), int(0x02, sev)));
    binds.extend(varbind(&format!("{base}.1.5"), octets(ev.kind.text("fr"))));
    binds.extend(varbind(&format!("{base}.1.6"), octets(&hostname())));
    binds.extend(varbind(&format!("{base}.1.7"), octets(ev.state)));
    let mut pdu = int(0x02, request_id as i64);
    pdu.extend(int(0x02, 0)); // error-status
    pdu.extend(int(0x02, 0)); // error-index
    pdu.extend(tlv(0x30, &binds));
    let mut msg = int(0x02, 1); // version: v2c
    msg.extend(octets(community));
    msg.extend(tlv(0xa7, &pdu)); // SNMPv2-Trap-PDU
    tlv(0x30, &msg)
}

fn kind_name(ev: &AlarmEvent) -> String {
    serde_json::to_value(ev.kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

pub fn send_trap(t: &SnmpTarget, base: &str, ev: &AlarmEvent) -> Result<(), String> {
    static ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
    let to = resolve(&t.host, 162)?;
    let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0x7fff_ffff;
    let uptime = (started().elapsed().as_millis() / 10) as u32;
    let community = if t.community.is_empty() {
        "public"
    } else {
        &t.community
    };
    let pkt = trap(community, &base_oid(base), id, uptime, ev);
    let sock = UdpSocket::bind(if to.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })
        .map_err(|e| e.to_string())?;
    sock.send_to(&pkt, to).map_err(|e| e.to_string())?;
    Ok(())
}

// -------------------------------------------------------------- syslog --

fn rfc3339(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .unwrap_or_else(|_| jiff::Timestamp::now())
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// A value of a key=value pair, quoted when it needs it.
fn kv(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:".contains(c))
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// One RFC 5424 line: `<PRI>1 TIME HOST strata-mv - MSGID - MSG`.
fn line(facility: u8, severity: u8, at: i64, msgid: &str, msg: &str) -> String {
    let pri = facility.min(23) as u16 * 8 + severity as u16;
    format!(
        "<{pri}>1 {} {} strata-mv {} {msgid} - {msg}",
        rfc3339(at),
        hostname(),
        std::process::id()
    )
}

pub fn alarm_line(facility: u8, ev: &AlarmEvent) -> String {
    // 2 critical, 4 warning, 5 notice (over, acknowledged).
    let sev = match (ev.state, ev.severity) {
        ("raised", Severity::Critical) => 2,
        ("raised", Severity::Warning) => 4,
        _ => 5,
    };
    let msg = format!(
        "state={} severity={} input={} inputId={} kind={} text={}",
        ev.state,
        match ev.severity {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
        },
        kv(&ev.input),
        kv(&ev.input_id),
        kind_name(ev),
        kv(ev.kind.text("fr")),
    );
    line(facility, sev, ev.at, "ALARM", &msg)
}

pub fn audit_line(facility: u8, e: &AuditEntry) -> String {
    // 6 informational, 5 notice when refused.
    let sev = if e.status >= 400 { 5 } else { 6 };
    let msg = format!(
        "user={} ip={} action={} method={} path={} status={}",
        kv(&e.user),
        kv(&e.ip),
        kv(&e.action),
        e.method,
        kv(&e.path),
        e.status
    );
    line(facility, sev, e.at, "AUDIT", &msg)
}

pub fn send_syslog(t: &SyslogTarget, line: &str) -> Result<(), String> {
    let tcp = t.transport.eq_ignore_ascii_case("tcp");
    let to = resolve(&t.host, 514)?;
    if tcp {
        let mut s =
            TcpStream::connect_timeout(&to, Duration::from_secs(3)).map_err(|e| e.to_string())?;
        let _ = s.set_write_timeout(Some(Duration::from_secs(3)));
        s.write_all(format!("{} {line}", line.len()).as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        let sock = UdpSocket::bind(if to.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })
            .map_err(|e| e.to_string())?;
        // 2048 octets: what every receiver takes (RFC 5424 § 6.1).
        let b = line.as_bytes();
        let mut end = b.len().min(2048);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        sock.send_to(&b[..end], to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_mv_core::signal::AlarmKind;

    fn ev() -> AlarmEvent {
        AlarmEvent {
            state: "raised",
            input_id: "cam1".into(),
            input: "Caméra 1".into(),
            kind: AlarmKind::Black,
            severity: Severity::Critical,
            at: 1_793_538_000_000,
        }
    }

    #[test]
    fn ber_basics() {
        assert_eq!(int(0x02, 0), [2, 1, 0]);
        assert_eq!(int(0x02, 127), [2, 1, 127]);
        assert_eq!(int(0x02, 128), [2, 2, 0, 128]);
        assert_eq!(int(0x02, -1), [2, 1, 0xff]);
        assert_eq!(int(0x43, 300), [0x43, 2, 1, 44]);
        // 1.3.6.1.4.1.8072 → 2B 06 01 04 01 BF 08
        assert_eq!(
            oid("1.3.6.1.4.1.8072"),
            [6, 7, 0x2b, 6, 1, 4, 1, 0xbf, 0x08]
        );
        let mut long = Vec::new();
        len(&mut long, 300);
        assert_eq!(long, [0x82, 1, 44]);
    }

    #[test]
    fn a_trap_is_a_v2c_trap_pdu() {
        let p = trap("public", SNMP_BASE_OID, 7, 1234, &ev());
        assert_eq!(p[0], 0x30);
        // version 1 (v2c), community "public", then the [7] PDU.
        let body_at = if p[1] & 0x80 != 0 {
            2 + (p[1] & 0x7f) as usize
        } else {
            2
        };
        assert_eq!(&p[body_at..body_at + 3], &[2, 1, 1]);
        assert_eq!(&p[body_at + 3..body_at + 11], b"\x04\x06public");
        assert_eq!(p[body_at + 11], 0xa7);
        // The notification OID is in it: base.0.1.
        let n = oid(&format!("{SNMP_BASE_OID}.0.1"));
        assert!(p.windows(n.len()).any(|w| w == n.as_slice()));
        assert!(p
            .windows("Caméra 1".len())
            .any(|w| w == "Caméra 1".as_bytes()));
    }

    #[test]
    fn syslog_lines() {
        let l = alarm_line(16, &ev());
        assert!(l.starts_with("<130>1 2026-11-01T"), "{l}"); // local0 (16) * 8 + crit (2)
        assert!(l.contains(" strata-mv "));
        assert!(
            l.contains(
                "ALARM - state=raised severity=critical input=\"Caméra 1\" inputId=cam1 kind=black"
            ),
            "{l}"
        );
        let a = audit_line(
            23,
            &AuditEntry {
                at: 0,
                user: "admin".into(),
                ip: "10.0.0.2".into(),
                method: "POST".into(),
                path: "/api/auth/login".into(),
                status: 401,
                action: "Connexion".into(),
            },
        );
        assert!(a.starts_with("<189>1 1970-01-01T00:00:00.000Z "), "{a}"); // 23 * 8 + 5
        assert!(a.ends_with(
            "user=admin ip=10.0.0.2 action=Connexion method=POST path=/api/auth/login status=401"
        ));
    }

    #[test]
    fn addresses() {
        assert_eq!(resolve("127.0.0.1", 162).unwrap().port(), 162);
        assert_eq!(resolve("127.0.0.1:1162", 162).unwrap().port(), 1162);
        assert_eq!(resolve("::1", 514).unwrap().port(), 514);
        assert_eq!(resolve("[::1]:6514", 514).unwrap().port(), 6514);
        assert_eq!(base_oid(""), SNMP_BASE_OID);
        assert_eq!(base_oid(".1.3.6.1.4.1.99999"), "1.3.6.1.4.1.99999");
        assert_eq!(base_oid("abc"), SNMP_BASE_OID);
    }
}
