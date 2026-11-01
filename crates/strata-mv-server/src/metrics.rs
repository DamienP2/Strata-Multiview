//! `GET /metrics`: the server's measures for Prometheus (text format 0.0.4):
//! machine, synchronisation, inputs (picture, audio, loudness), channels,
//! outputs and the faults on now. Read with an API key (`Authorization:
//! Bearer smv_…` in the scrape settings, or `?key=smv_…`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use strata_mv_core::signal::Severity;

use crate::auth::CurrentUser;
use crate::state::Shared;

/// A label value, escaped.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn num(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 { "+Inf" } else { "-Inf" }.into()
    } else {
        format!("{v}")
    }
}

/// The families, written each with its HELP and TYPE once.
#[derive(Default)]
pub struct Exposition {
    families: BTreeMap<&'static str, (&'static str, &'static str, Vec<String>)>,
}

impl Exposition {
    pub fn add(
        &mut self,
        name: &'static str,
        kind: &'static str,
        help: &'static str,
        labels: &[(&str, &str)],
        v: f64,
    ) {
        let l = if labels.is_empty() {
            String::new()
        } else {
            format!(
                "{{{}}}",
                labels
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{}\"", esc(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        self.families
            .entry(name)
            .or_insert_with(|| (kind, help, Vec::new()))
            .2
            .push(format!("{name}{l} {}", num(v)));
    }
    pub fn gauge(
        &mut self,
        name: &'static str,
        help: &'static str,
        labels: &[(&str, &str)],
        v: f64,
    ) {
        self.add(name, "gauge", help, labels, v);
    }
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, (kind, help, lines)) in &self.families {
            let _ = writeln!(out, "# HELP {name} {help}");
            let _ = writeln!(out, "# TYPE {name} {kind}");
            for l in lines {
                out.push_str(l);
                out.push('\n');
            }
        }
        out
    }
}

pub async fn metrics(State(s): State<Shared>, CurrentUser(_me): CurrentUser) -> impl IntoResponse {
    let mut m = Exposition::default();
    let sm = s.perf.sampled();
    let host = sm.machine.hostname.clone();
    m.gauge(
        "strata_info",
        "Version and machine of the server.",
        &[("version", env!("CARGO_PKG_VERSION")), ("hostname", &host)],
        1.0,
    );
    m.add(
        "strata_uptime_seconds",
        "counter",
        "Seconds since the server started.",
        &[],
        s.started_at
            .elapsed()
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
    );
    m.gauge(
        "strata_machine_cpu_percent",
        "Processor load of the machine.",
        &[],
        sm.machine.cpu as f64,
    );
    m.gauge(
        "strata_machine_cpu_cores",
        "Logical processors.",
        &[],
        sm.machine.cores as f64,
    );
    m.gauge(
        "strata_machine_memory_used_bytes",
        "Memory in use on the machine.",
        &[],
        sm.machine.mem_used as f64,
    );
    m.gauge(
        "strata_machine_memory_total_bytes",
        "Memory of the machine.",
        &[],
        sm.machine.mem_total as f64,
    );
    m.gauge(
        "strata_process_cpu_percent",
        "Processor share of Strata (of the machine).",
        &[],
        sm.process.cpu as f64,
    );
    m.gauge(
        "strata_process_memory_bytes",
        "Memory of Strata.",
        &[],
        sm.process.mem as f64,
    );
    m.gauge(
        "strata_ffmpeg_cpu_percent",
        "Processor share of the ffmpeg decoders.",
        &[],
        sm.process.ffmpeg_cpu as f64,
    );
    m.gauge(
        "strata_ffmpeg_processes",
        "ffmpeg decoders running.",
        &[],
        sm.process.ffmpeg as f64,
    );

    let signals = s.inputs.signals();
    let sys = signals.system();
    m.gauge(
        "strata_ptp_running",
        "PTP is followed.",
        &[],
        sys.ptp.running as u8 as f64,
    );
    m.gauge(
        "strata_ptp_locked",
        "Locked to the PTP grandmaster.",
        &[],
        sys.ptp.locked as u8 as f64,
    );
    if let Some(ns) = sys.ptp.offset_ns.filter(|_| sys.ptp.running) {
        m.gauge(
            "strata_ptp_offset_seconds",
            "Offset from the PTP grandmaster.",
            &[("grandmaster", &sys.ptp.grandmaster)],
            ns / 1e9,
        );
    }
    if let Some(ok) = sys.ntp.synced {
        m.gauge(
            "strata_ntp_synced",
            "The system clock is synchronised.",
            &[],
            ok as u8 as f64,
        );
    }
    if let Some(ms) = sys.ntp.offset_ms {
        m.gauge(
            "strata_ntp_offset_seconds",
            "Offset of the system clock from its NTP source.",
            &[("source", &sys.ntp.source)],
            ms / 1e3,
        );
    }

    // Inputs.
    let names: BTreeMap<&str, &str> = s
        .config
        .inputs
        .iter()
        .map(|i| (i.id.as_str(), i.name.as_str()))
        .collect();
    for st in s.inputs.status() {
        let name = names.get(st.id.as_str()).copied().unwrap_or(&st.id);
        let l = [("input", st.id.as_str()), ("name", name)];
        m.gauge(
            "strata_input_up",
            "A picture came in the last two seconds.",
            &l,
            st.live as u8 as f64,
        );
        m.add(
            "strata_input_frames_total",
            "counter",
            "Pictures received since the input started.",
            &l,
            st.frames as f64,
        );
        if let Some(r) = sm.inputs.get(&st.id) {
            m.gauge("strata_input_fps", "Pictures a second received.", &l, r.fps);
            m.gauge(
                "strata_input_dropped_10s",
                "Pictures lost in the last 10 seconds.",
                &l,
                r.count10,
            );
        }
        if let Some(fps) = st.fps {
            m.gauge(
                "strata_input_nominal_fps",
                "Pictures a second of the source.",
                &l,
                fps,
            );
        }
        if st.width > 0 {
            m.gauge(
                "strata_input_width_pixels",
                "Width of the picture.",
                &l,
                st.width as f64,
            );
            m.gauge(
                "strata_input_height_pixels",
                "Height of the picture.",
                &l,
                st.height as f64,
            );
        }
        for (i, lv) in signals.levels(&st.id).iter().enumerate() {
            let ch = (i + 1).to_string();
            let lc = [
                ("input", st.id.as_str()),
                ("name", name),
                ("channel", ch.as_str()),
            ];
            m.gauge(
                "strata_input_audio_peak_dbfs",
                "Peak level of an audio channel.",
                &lc,
                lv.peak as f64,
            );
            m.gauge(
                "strata_input_audio_rms_dbfs",
                "RMS level of an audio channel.",
                &lc,
                lv.rms as f64,
            );
        }
        if let (Some(ld), _) = signals.loudness_of(&st.id) {
            m.gauge(
                "strata_input_loudness_momentary_lufs",
                "Momentary loudness (400 ms, EBU R128).",
                &l,
                ld.momentary as f64,
            );
            m.gauge(
                "strata_input_loudness_short_term_lufs",
                "Short-term loudness (3 s).",
                &l,
                ld.short_term as f64,
            );
            m.gauge(
                "strata_input_loudness_integrated_lufs",
                "Integrated loudness.",
                &l,
                ld.integrated as f64,
            );
            m.gauge(
                "strata_input_loudness_range_lu",
                "Loudness range.",
                &l,
                ld.range as f64,
            );
            m.gauge(
                "strata_input_true_peak_dbtp",
                "True peak.",
                &l,
                ld.true_peak as f64,
            );
        }
    }

    // SRT inputs received by libsrt.
    for (id, st) in s.inputs.srt_stats() {
        let name = names.get(id.as_str()).copied().unwrap_or(&id);
        let l = [("input", id.as_str()), ("name", name)];
        m.gauge(
            "strata_srt_rtt_seconds",
            "SRT round trip time.",
            &l,
            st.rtt_ms / 1e3,
        );
        m.gauge(
            "strata_srt_rate_bits",
            "SRT receive rate, bits a second.",
            &l,
            st.rate_mbps * 1e6,
        );
        m.add(
            "strata_srt_packets_received_total",
            "counter",
            "SRT packets received.",
            &l,
            st.received as f64,
        );
        m.add(
            "strata_srt_packets_lost_total",
            "counter",
            "SRT packets lost (most sent again).",
            &l,
            st.lost as f64,
        );
        m.add(
            "strata_srt_packets_retransmitted_total",
            "counter",
            "SRT packets received again.",
            &l,
            st.retransmitted as f64,
        );
        m.add(
            "strata_srt_packets_dropped_total",
            "counter",
            "SRT packets lost for good (too late).",
            &l,
            st.dropped as f64,
        );
        m.gauge(
            "strata_srt_buffer_seconds",
            "What the SRT receiver holds.",
            &l,
            st.buffer_ms as f64 / 1e3,
        );
    }

    // Latency measured with the lip sync flash (lately).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    for l in crate::latency::all()
        .into_iter()
        .filter(|l| now - l.at < 30_000)
    {
        m.gauge(
            "strata_input_latency_seconds",
            "End-to-end latency measured with the generator's lip sync flash.",
            &[("input", &l.input), ("channel", &l.channel)],
            l.avg_ms / 1e3,
        );
    }

    // Faults on now.
    let (_, journal) = signals.journal();
    let mut count = [0u32; 2];
    for e in journal.iter().filter(|e| e.ended.is_none()) {
        let kind = serde_json::to_value(e.kind)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let sev = match e.severity {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
        };
        count[(e.severity == Severity::Critical) as usize] += 1;
        m.gauge(
            "strata_alarm",
            "A fault on now (input \"\" for the plant's).",
            &[
                ("input", &e.input_id),
                ("name", &e.input),
                ("kind", &kind),
                ("severity", sev),
                (
                    "acknowledged",
                    if e.acked.is_some() { "true" } else { "false" },
                ),
            ],
            1.0,
        );
    }
    m.gauge(
        "strata_alarms_active",
        "Faults on now.",
        &[("severity", "warning")],
        count[0] as f64,
    );
    m.gauge(
        "strata_alarms_active",
        "Faults on now.",
        &[("severity", "critical")],
        count[1] as f64,
    );

    // Channels and outputs.
    for c in &s.config.channels {
        let l = [("channel", c.id.as_str()), ("name", c.name.as_str())];
        m.gauge(
            "strata_channel_frame_rate",
            "Pictures a second of the channel.",
            &l,
            c.frame_rate.as_f64(),
        );
        let slowest = sm
            .pictures
            .iter()
            .filter(|(k, _)| k.split('/').next() == Some(c.id.as_str()))
            .map(|(_, r)| r.ms)
            .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
        if let Some(ms) = slowest {
            m.gauge(
                "strata_channel_render_seconds",
                "Render time of the slowest picture.",
                &l,
                ms / 1e3,
            );
        }
        if let Some(clock) = sm.clocks.get(&c.id) {
            m.gauge(
                "strata_channel_clock_late_seconds",
                "Worst lateness of the channel clock (last second).",
                &l,
                clock.max_ms / 1e3,
            );
            m.gauge(
                "strata_channel_ticks_skipped_10s",
                "Clock ticks skipped in the last 10 seconds.",
                &l,
                clock.count10,
            );
        }
    }
    for (key, r) in &sm.outputs {
        let mut parts = key.splitn(3, '/');
        let (ch, _pic, out) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        let l = [("channel", ch), ("output", out)];
        m.gauge("strata_output_fps", "Pictures a second sent.", &l, r.fps);
        m.gauge(
            "strata_output_dropped_10s",
            "Pictures lost in the last 10 seconds.",
            &l,
            r.count10,
        );
        m.gauge(
            "strata_output_send_seconds",
            "Time to send a picture.",
            &l,
            r.ms / 1e3,
        );
    }
    for (ch, outs) in s.output_status.snapshot() {
        for (out, st) in outs {
            m.gauge(
                "strata_output_ok",
                "The output works.",
                &[("channel", &ch), ("output", &out)],
                st.ok as u8 as f64,
            );
        }
    }

    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        m.render(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposition_format() {
        let mut m = Exposition::default();
        m.gauge("strata_x", "An x.", &[("input", "cam \"1\"")], 1.5);
        m.gauge("strata_x", "An x.", &[("input", "cam2")], f64::NAN);
        m.add("strata_a_total", "counter", "Some.", &[], 3.0);
        let t = m.render();
        assert_eq!(
            t,
            "# HELP strata_a_total Some.\n# TYPE strata_a_total counter\nstrata_a_total 3\n\
             # HELP strata_x An x.\n# TYPE strata_x gauge\nstrata_x{input=\"cam \\\"1\\\"\"} 1.5\nstrata_x{input=\"cam2\"} NaN\n"
        );
    }
}
