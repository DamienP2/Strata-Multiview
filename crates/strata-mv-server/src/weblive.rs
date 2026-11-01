//! Live streams of the web platforms (YouTube, TikTok, Twitch…): the page's
//! address is turned into the stream it plays by yt-dlp, which the server
//! fetches into its data folder on first use (with Deno, the JavaScript
//! runtime YouTube needs) and keeps up to date. The stream's address expires
//! after a few hours: it is asked again each time the stream restarts.
//!
//! And the HLS playlists: of a master playlist, the variant the input is
//! decoded at; a list of channels (IPTV M3U) is told apart, to choose one.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Where yt-dlp and Deno are kept (`<data>/tools`).
static TOOLS: OnceLock<PathBuf> = OnceLock::new();

/// Sets where the tools are kept (at startup).
pub fn set_tools_dir(dir: PathBuf) {
    let _ = TOOLS.set(dir);
}

fn tools_dir() -> PathBuf {
    TOOLS
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("strata-mv-tools"))
}

/// The platform of a page's address ("YouTube", "TikTok"…), when known.
pub fn platform(url: &str) -> Option<&'static str> {
    let host = url
        .trim()
        .split("://")
        .nth(1)?
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?
        .to_ascii_lowercase();
    let is = |d: &str| host == d || host.ends_with(&format!(".{d}"));
    Some(if is("youtube.com") || is("youtu.be") {
        "YouTube"
    } else if is("tiktok.com") {
        "TikTok"
    } else if is("twitch.tv") {
        "Twitch"
    } else if is("facebook.com") || is("fb.watch") {
        "Facebook"
    } else if is("instagram.com") {
        "Instagram"
    } else if is("kick.com") {
        "Kick"
    } else if is("dailymotion.com") || is("dai.ly") {
        "Dailymotion"
    } else if is("x.com") || is("twitter.com") {
        "X"
    } else if is("vimeo.com") {
        "Vimeo"
    } else if is("rumble.com") {
        "Rumble"
    } else if is("france.tv") {
        "france.tv"
    } else if is("arte.tv") {
        "Arte"
    } else {
        return None;
    })
}

// ------------------------------------------------------------- the tools --

/// The yt-dlp of this system's release ("yt-dlp_macos"…) and its file name here.
fn ytdlp_asset() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        ("yt-dlp.exe", "yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        ("yt-dlp_macos", "yt-dlp")
    } else if cfg!(target_arch = "aarch64") {
        ("yt-dlp_linux_aarch64", "yt-dlp")
    } else {
        ("yt-dlp_linux", "yt-dlp")
    }
}

/// Deno's archive for this system.
fn deno_asset() -> &'static str {
    match (
        cfg!(target_os = "windows"),
        cfg!(target_os = "macos"),
        cfg!(target_arch = "aarch64"),
    ) {
        (true, _, _) => "deno-x86_64-pc-windows-msvc.zip",
        (_, true, true) => "deno-aarch64-apple-darwin.zip",
        (_, true, false) => "deno-x86_64-apple-darwin.zip",
        (_, _, true) => "deno-aarch64-unknown-linux-gnu.zip",
        _ => "deno-x86_64-unknown-linux-gnu.zip",
    }
}

fn exe(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `name` in the PATH.
fn in_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(exe(name)))
        .find(|p| p.is_file())
}

/// A tool: the setting's (environment), the one kept here, next to the
/// server, then the system's.
fn find(name: &str, env: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    let here = tools_dir().join(exe(name));
    if here.is_file() {
        return Some(here);
    }
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(exe(name))))
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    in_path(name)
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(300)))
        .max_redirects(10)
        .user_agent(concat!("strata-multiview/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut r = agent().get(url).call().map_err(|e| format!("{url}: {e}"))?;
    r.body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// Writes an executable whole (a half-written one is never run).
fn install(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// One download at a time (inputs starting together wait for the first).
static FETCHING: Mutex<()> = Mutex::new(());

/// yt-dlp, fetched the first time.
fn ytdlp() -> Result<PathBuf, String> {
    if let Some(p) = find("yt-dlp", "STRATA_YTDLP") {
        return Ok(p);
    }
    let _one = FETCHING.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(p) = find("yt-dlp", "STRATA_YTDLP") {
        return Ok(p);
    }
    let (asset, file) = ytdlp_asset();
    tracing::info!("web live: fetching yt-dlp ({asset})");
    let bytes = download(
        &format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}"),
        200 << 20,
    )
    .map_err(|e| format!("yt-dlp could not be fetched ({e})"))?;
    let path = tools_dir().join(file);
    install(&path, &bytes)?;
    Ok(path)
}

/// Deno (YouTube's pages need a JavaScript runtime), fetched the first time.
fn deno() -> Option<PathBuf> {
    if let Some(p) = find("deno", "STRATA_DENO") {
        return Some(p);
    }
    let _one = FETCHING.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(p) = find("deno", "STRATA_DENO") {
        return Some(p);
    }
    let asset = deno_asset();
    tracing::info!("web live: fetching Deno ({asset})");
    let zip = download(
        &format!("https://github.com/denoland/deno/releases/latest/download/{asset}"),
        400 << 20,
    )
    .map_err(|e| tracing::warn!("Deno could not be fetched: {e}"))
    .ok()?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).ok()?;
    let mut bin = Vec::new();
    archive
        .by_name(&exe("deno"))
        .ok()?
        .read_to_end(&mut bin)
        .ok()?;
    let path = tools_dir().join(exe("deno"));
    install(&path, &bin)
        .map_err(|e| tracing::warn!("Deno: {e}"))
        .ok()?;
    Some(path)
}

/// When yt-dlp was last brought up to date (it follows the platforms' changes).
static UPDATED: Mutex<Option<Instant>> = Mutex::new(None);

/// Brings yt-dlp up to date, at most twice a day; true when it ran.
fn update(path: &Path) -> bool {
    {
        let mut at = UPDATED.lock().unwrap_or_else(|p| p.into_inner());
        if at.is_some_and(|t| t.elapsed() < Duration::from_secs(12 * 3600)) {
            return false;
        }
        *at = Some(Instant::now());
    }
    let _one = FETCHING.lock().unwrap_or_else(|p| p.into_inner());
    run(Command::new(path).arg("-U"), Duration::from_secs(180)).is_ok()
}

/// Runs a command, its output; killed after `limit`.
fn run(cmd: &mut Command, limit: Duration) -> Result<(String, String, bool), String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("yt-dlp could not start ({e})"))?;
    let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
    let o = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let e = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let start = Instant::now();
    let ok = loop {
        if let Ok(Some(st)) = child.try_wait() {
            if !st.success() {
                tracing::debug!("{:?} ended: {st}", cmd.get_program());
            }
            break st.success();
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("yt-dlp gave no answer in 90 s".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Ok((
        o.join().unwrap_or_default(),
        e.join().unwrap_or_default(),
        ok,
    ))
}

// ----------------------------------------------------------- the stream --

/// The stream a page plays.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    /// What ffmpeg reads.
    pub url: String,
    /// Headers the stream needs (User-Agent, Referer, Cookie…).
    #[serde(skip)]
    pub headers: Vec<(String, String)>,
    pub platform: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channel: String,
    pub live: bool,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// m3u8, https, …
    pub protocol: String,
    /// The sound apart, when the platform gives picture and sound as two
    /// streams (YouTube's lives in high definition): read as a second input.
    #[serde(skip)]
    pub audio: String,
}

/// The format asked of yt-dlp: picture and sound together, the best up to
/// `height` lines.
fn format(height: u32) -> String {
    let h = height.clamp(144, 4320);
    format!("best[height<=?{h}]/best/bestvideo[height<=?{h}]+bestaudio/bestvideo+bestaudio")
}

/// yt-dlp's answer.
fn parse(json: &str, page: &str) -> Result<Resolved, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    // One format (picture and sound) at the top, or the two of a pair.
    let f = if v.get("url").and_then(|u| u.as_str()).is_some() {
        &v
    } else {
        v.get("requested_formats")
            .and_then(|r| r.as_array())
            .and_then(|r| {
                r.iter()
                    .find(|f| f.get("vcodec").and_then(|c| c.as_str()) != Some("none"))
                    .or(r.first())
            })
            .ok_or("no stream found on this page")?
    };
    let s = |o: &serde_json::Value, k: &str| {
        o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let n = |o: &serde_json::Value, k: &str| o.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let headers = f
        .get("http_headers")
        .and_then(|h| h.as_object())
        .map(|h| {
            h.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                // ffmpeg asks for the encodings it reads.
                .filter(|(k, _)| !k.eq_ignore_ascii_case("accept-encoding"))
                .collect()
        })
        .unwrap_or_default();
    // The sound of a pair (picture and sound apart).
    let audio = v
        .get("requested_formats")
        .and_then(|r| r.as_array())
        .and_then(|r| {
            r.iter().find(|a| {
                a.get("vcodec").and_then(|c| c.as_str()) == Some("none")
                    && a.get("acodec").and_then(|c| c.as_str()) != Some("none")
            })
        })
        .map(|a| s(a, "url"))
        .filter(|u| *u != s(f, "url"))
        .unwrap_or_default();
    Ok(Resolved {
        url: s(f, "url"),
        audio,
        headers,
        platform: platform(page)
            .map(String::from)
            .unwrap_or_else(|| s(&v, "extractor_key")),
        title: s(&v, "title"),
        channel: [s(&v, "channel"), s(&v, "uploader")]
            .into_iter()
            .find(|c| !c.is_empty())
            .unwrap_or_default(),
        live: v.get("is_live").and_then(|x| x.as_bool()).unwrap_or(false)
            || s(&v, "live_status") == "is_live",
        width: n(f, "width"),
        height: n(f, "height"),
        fps: f.get("fps").and_then(|x| x.as_f64()).filter(|x| *x > 0.0),
        protocol: s(f, "protocol"),
    })
}

/// yt-dlp's error, in a few words ("ERROR: [youtube] id: This live event
/// will begin in 3 hours." → "This live event will begin in 3 hours.").
fn error_of(stderr: &str) -> String {
    let line = stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("ERROR:"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("no stream found on this page")
        .trim_start_matches("ERROR:")
        .trim();
    // "[youtube] abc: message" → "message"
    let line = match line.strip_prefix('[') {
        Some(rest) => rest
            .split_once("] ")
            .map(|(_, m)| m.split_once(": ").map_or(m, |(_, m)| m))
            .unwrap_or(line),
        None => line,
    };
    line.chars().take(300).collect()
}

/// The JavaScript runtimes yt-dlp may use (YouTube's pages need one).
fn js_runtimes(c: &mut Command, page: &str) {
    if platform(page) == Some("YouTube") {
        if let Some(d) = deno() {
            c.arg("--js-runtimes").arg(format!("deno:{}", d.display()));
        }
    }
    // The system's Node or Bun, when there is one.
    for r in ["node", "bun"] {
        if let Some(p) = in_path(r) {
            c.arg("--js-runtimes").arg(format!("{r}:{}", p.display()));
        }
    }
}

/// yt-dlp reading the live itself and writing it on its standard output
/// (MPEG-TS; picture and sound put together by ffmpeg when they come
/// apart): the stream as the platform serves it to yt-dlp, its headers,
/// cookies and address the same as when it was found. Used when ffmpeg
/// cannot open the stream found.
pub fn pipe_command(page: &str, height: u32) -> Result<Command, String> {
    let page = page.trim();
    if !(page.starts_with("https://") || page.starts_with("http://")) {
        return Err("the address must start with https://".into());
    }
    let mut c = Command::new(ytdlp()?);
    c.args([
        "--no-playlist",
        "--no-warnings",
        "--no-progress",
        "--no-part",
        "--hls-use-mpegts",
        "-f",
    ])
    .arg(format(height));
    // Strata's ffmpeg, by its full path (yt-dlp does not search a bare name).
    let ff = strata_mv_output::ffmpeg::path();
    let ff = if ff.is_absolute() {
        Some(ff)
    } else {
        in_path(&ff.to_string_lossy().replace(".exe", ""))
    };
    if let Some(ff) = ff {
        c.arg("--ffmpeg-location").arg(ff);
    }
    js_runtimes(&mut c, page);
    c.args(["-o", "-", "--"]).arg(page);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW
        c.creation_flags(0x0800_0000);
    }
    Ok(c)
}

/// The stream of a page (a few seconds: yt-dlp reads the page).
pub fn resolve(page: &str, height: u32) -> Result<Resolved, String> {
    let page = page.trim();
    if !(page.starts_with("https://") || page.starts_with("http://")) {
        return Err("the address must start with https://".into());
    }
    let tool = ytdlp()?;
    let ask = || {
        let mut c = Command::new(&tool);
        c.args([
            "-J",
            "--no-playlist",
            "--no-warnings",
            "--no-progress",
            "-f",
        ])
        .arg(format(height));
        js_runtimes(&mut c, page);
        c.arg("--").arg(page);
        run(&mut c, Duration::from_secs(90))
    };
    let mut answer = ask()?;
    // The platforms change: an old yt-dlp, brought up to date, tries again.
    if !answer.2 && update(&tool) {
        answer = ask()?;
    }
    let (out, err, ok) = answer;
    if !ok || out.trim().is_empty() {
        let why = if err.trim().is_empty() {
            // Stopped without a word: killed by the system (a binary it refuses?).
            "yt-dlp stopped without an answer (the system may refuse to run it)".to_string()
        } else {
            error_of(&err)
        };
        tracing::warn!(page, "web live: {why}\n{}", err.trim());
        return Err(why);
    }
    let r = parse(&out, page)?;
    if r.url.is_empty() {
        return Err("no stream found on this page".into());
    }
    Ok(r)
}

/// What « Test the link » tells: the stream found, or why there is none, and
/// the tools used.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<Resolved>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// yt-dlp's version and where it is ("" when it could not be had).
    pub ytdlp: String,
    pub ytdlp_path: String,
    /// Deno's (YouTube only).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub deno: String,
    /// Seconds it took.
    pub seconds: f64,
}

/// Looks for the stream of a page and says how it went (the input page's
/// « Test the link »).
pub fn check(page: &str, height: u32) -> Check {
    let start = Instant::now();
    let mut c = Check::default();
    match ytdlp() {
        Ok(p) => {
            c.ytdlp_path = p.display().to_string();
            c.ytdlp = run(Command::new(&p).arg("--version"), Duration::from_secs(60))
                .map(|(o, e, ok)| {
                    if ok {
                        o.trim().to_string()
                    } else {
                        format!("? ({})", e.trim())
                    }
                })
                .unwrap_or_else(|e| e);
        }
        Err(e) => c.error = e,
    }
    if c.error.is_empty() {
        if platform(page) == Some("YouTube") {
            c.deno = deno().map_or_else(|| "absent".into(), |p| p.display().to_string());
        }
        match resolve(page, height) {
            Ok(r) => c.found = Some(r),
            Err(e) => c.error = e,
        }
    }
    c.seconds = start.elapsed().as_secs_f64();
    c
}

/// The ffmpeg options that send a stream's headers.
pub fn header_args(headers: &[(String, String)]) -> Vec<String> {
    let mut a = Vec::new();
    let mut rest = String::new();
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("user-agent") {
            a.extend(["-user_agent".to_string(), v.clone()]);
        } else if !v.contains(['\r', '\n']) {
            rest.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    if !rest.is_empty() {
        a.extend(["-headers".to_string(), rest]);
    }
    a
}

// ---------------------------------------------------------- playlists --

/// A channel of a list (IPTV M3U).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub logo: String,
}

/// What a playlist is.
#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    /// A media playlist: read as it is.
    Media,
    /// A master playlist: the variant to decode (its number, its height),
    /// its own playlist, and the playlist of its sound when it comes apart
    /// (an audio rendition).
    Master {
        variant: usize,
        height: u32,
        video: String,
        audio: Option<String>,
    },
    /// A list of channels: one is to be chosen.
    Channels(Vec<Channel>),
}

/// `key="value"` attributes of an #EXTINF line.
fn attribute(line: &str, key: &str) -> String {
    let k = format!("{key}=\"");
    line.find(&k)
        .and_then(|i| line[i + k.len()..].split('"').next())
        .unwrap_or("")
        .to_string()
}

/// Reads a playlist: of a master, the best variant up to `height` lines (the
/// smallest when all are taller).
pub fn read(base: &str, text: &str, height: u32) -> Playlist {
    if text.contains("#EXT-X-STREAM-INF") {
        // Each variant: its height, its bandwidth, its playlist, its audio group.
        let mut variants: Vec<(u32, u64, String, String)> = Vec::new();
        // The audio renditions: group, default, playlist.
        let mut renditions: Vec<(String, bool, String)> = Vec::new();
        let mut lines = text.lines().map(str::trim);
        while let Some(l) = lines.next() {
            if let Some(attrs) = l.strip_prefix("#EXT-X-MEDIA:") {
                let uri = attribute(attrs, "URI");
                if attrs.contains("TYPE=AUDIO") && !uri.is_empty() {
                    renditions.push((
                        attribute(attrs, "GROUP-ID"),
                        attrs.contains("DEFAULT=YES"),
                        crate::probe::hls::resolve(base, &uri),
                    ));
                }
            } else if let Some(attrs) = l.strip_prefix("#EXT-X-STREAM-INF:") {
                let h = attrs
                    .split("RESOLUTION=")
                    .nth(1)
                    .and_then(|r| r.split([',', 'x']).nth(1))
                    .and_then(|h| h.trim().parse().ok())
                    .unwrap_or(0);
                let bw = attrs
                    .split("BANDWIDTH=")
                    .nth(1)
                    .and_then(|b| b.split(',').next())
                    .and_then(|b| b.trim().parse().ok())
                    .unwrap_or(0);
                // Its address on the next line that is not a tag.
                if let Some(uri) = lines
                    .by_ref()
                    .find(|u| !u.is_empty() && !u.starts_with('#'))
                {
                    variants.push((
                        h,
                        bw,
                        crate::probe::hls::resolve(base, uri),
                        attribute(attrs, "AUDIO"),
                    ));
                }
            }
        }
        let fits = |v: &&(u32, u64, String, String)| v.0 == 0 || v.0 <= height;
        let pick = variants
            .iter()
            .enumerate()
            .filter(|(_, v)| fits(v))
            .max_by_key(|(_, v)| (v.0, v.1))
            .or_else(|| variants.iter().enumerate().min_by_key(|(_, v)| (v.0, v.1)));
        if let Some((i, (h, _, video, group))) = pick {
            // Its sound apart: the group's default rendition, else its first.
            let in_group: Vec<_> = renditions.iter().filter(|r| &r.0 == group).collect();
            let audio = (!group.is_empty())
                .then(|| {
                    in_group
                        .iter()
                        .find(|r| r.1)
                        .or(in_group.first())
                        .map(|r| r.2.clone())
                })
                .flatten();
            return Playlist::Master {
                variant: i,
                height: *h,
                video: video.clone(),
                audio,
            };
        }
        return Playlist::Media;
    }
    if text.contains("#EXT-X-TARGETDURATION") || !text.contains("#EXTINF") {
        return Playlist::Media;
    }
    // #EXTINF lines whose entries are streams, not segments: channels.
    let mut out = Vec::new();
    let mut info: Option<&str> = None;
    for l in text.lines().map(str::trim) {
        if l.starts_with("#EXTINF") {
            info = Some(l);
        } else if !l.is_empty() && !l.starts_with('#') {
            let i = info.take().unwrap_or("");
            let name = i.rsplit_once(',').map_or("", |(_, n)| n.trim());
            out.push(Channel {
                name: if name.is_empty() {
                    l.to_string()
                } else {
                    name.to_string()
                },
                url: crate::probe::hls::resolve(base, l),
                group: attribute(i, "group-title"),
                logo: attribute(i, "tvg-logo"),
            });
        }
    }
    Playlist::Channels(out)
}

/// Fetches a playlist and reads it (`read`).
pub fn playlist(url: &str, height: u32) -> Result<Playlist, String> {
    let a: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(10)
        .build()
        .into();
    let mut r = a.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let text = r
        .body_mut()
        .with_config()
        .limit(16 << 20)
        .read_to_string()
        .map_err(|e| format!("{url}: {e}"))?;
    if !text.trim_start().starts_with("#EXTM3U") && !text.contains("#EXTINF") {
        return Ok(Playlist::Media);
    }
    Ok(read(url, &text, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_and_sound_apart() {
        // A YouTube live in high definition: a video-only HLS and an audio one.
        let json = r#"{"title":"Live","is_live":true,"requested_formats":[
            {"url":"https://manifest.googlevideo.com/itag/270/index.m3u8","vcodec":"avc1.640028","acodec":"none","width":1920,"height":1080,"fps":30,"protocol":"m3u8_native","http_headers":{"User-Agent":"UA","Accept-Encoding":"gzip"}},
            {"url":"https://manifest.googlevideo.com/itag/234/index.m3u8","vcodec":"none","acodec":"mp4a.40.2","protocol":"m3u8_native"}]}"#;
        let r = parse(json, "https://www.youtube.com/watch?v=x").unwrap();
        assert!(r.url.contains("itag/270"));
        assert!(r.audio.contains("itag/234"));
        assert_eq!((r.width, r.height, r.live), (1920, 1080, true));
        assert_eq!(
            r.headers,
            vec![("User-Agent".to_string(), "UA".to_string())]
        );
        // Together: no sound apart.
        let one = r#"{"url":"https://a/b.m3u8","vcodec":"avc1","acodec":"mp4a"}"#;
        assert_eq!(parse(one, "https://kick.com/c").unwrap().audio, "");
    }

    #[test]
    fn platforms_by_address() {
        assert_eq!(
            platform("https://www.youtube.com/watch?v=abc"),
            Some("YouTube")
        );
        assert_eq!(platform("https://youtu.be/abc"), Some("YouTube"));
        assert_eq!(
            platform("https://www.tiktok.com/@someone/live"),
            Some("TikTok")
        );
        assert_eq!(platform("https://m.twitch.tv/chan"), Some("Twitch"));
        assert_eq!(platform("https://kick.com/chan"), Some("Kick"));
        assert_eq!(platform("https://notyoutube.com/x"), None);
        assert_eq!(platform("youtube.com"), None);
    }

    #[test]
    fn the_variant_that_fits() {
        let master = "#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=3440800,RESOLUTION=1280x720,CODECS=\"avc1.42c01f,mp4a.40.2\"
v0/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=6000000,RESOLUTION=1920x1080
v1/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=800800,RESOLUTION=640x360
v2/index.m3u8
";
        assert_eq!(
            read("https://a/m.m3u8", master, 1080),
            Playlist::Master {
                variant: 1,
                height: 1080,
                video: "https://a/v1/index.m3u8".into(),
                audio: None
            }
        );
        assert_eq!(
            read("https://a/m.m3u8", master, 720),
            Playlist::Master {
                variant: 0,
                height: 720,
                video: "https://a/v0/index.m3u8".into(),
                audio: None
            }
        );
        assert_eq!(
            read("https://a/m.m3u8", master, 200),
            Playlist::Master {
                variant: 2,
                height: 360,
                video: "https://a/v2/index.m3u8".into(),
                audio: None
            }
        );
        assert_eq!(
            read(
                "https://a/v.m3u8",
                "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6,\ns1.ts\n",
                1080
            ),
            Playlist::Media
        );
    }

    #[test]
    fn a_variant_with_its_sound_apart() {
        let master = "#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"en\",URI=\"a/en.m3u8\"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"fr\",DEFAULT=YES,URI=\"a/fr.m3u8\"
#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,AUDIO=\"aud\"
https://cdn/v1080.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1000000,RESOLUTION=640x360,AUDIO=\"aud\"
v360.m3u8
";
        assert_eq!(
            read("https://h/live/master.m3u8", master, 540),
            Playlist::Master {
                variant: 1,
                height: 360,
                video: "https://h/live/v360.m3u8".into(),
                audio: Some("https://h/live/a/fr.m3u8".into())
            }
        );
    }

    #[test]
    fn a_list_of_channels() {
        let list = "#EXTM3U
#EXTINF:-1 tvg-id=\"a.fr\" tvg-logo=\"https://l/a.png\" group-title=\"News\",Chaîne A
https://cdn/a/index.m3u8
#EXTINF:-1,Chaîne B
b/index.m3u8
";
        let Playlist::Channels(c) = read("https://iptv/list.m3u", list, 1080) else {
            panic!("not channels");
        };
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "Chaîne A");
        assert_eq!(c[0].group, "News");
        assert_eq!(c[0].logo, "https://l/a.png");
        assert_eq!(c[1].url, "https://iptv/b/index.m3u8");
    }

    #[test]
    fn yt_dlp_answers() {
        let r = parse(
            r#"{"title":"Live","is_live":true,"channel":"Chan","url":"https://m/x.m3u8","protocol":"m3u8_native","width":1920,"height":1080,"fps":30,"http_headers":{"User-Agent":"UA","Accept-Encoding":"gzip","Referer":"https://www.tiktok.com/"}}"#,
            "https://www.tiktok.com/@a/live",
        )
        .unwrap();
        assert!(r.live);
        assert_eq!(r.platform, "TikTok");
        assert_eq!(r.height, 1080);
        assert_eq!(r.headers.len(), 2);
        let a = header_args(&r.headers);
        assert_eq!(a[0], "-user_agent");
        assert_eq!(a[3], "Referer: https://www.tiktok.com/\r\n");
        assert_eq!(
            error_of(
                "WARNING: x\nERROR: [youtube] abc123: This live event will begin in 3 hours.\n"
            ),
            "This live event will begin in 3 hours."
        );
    }
}
