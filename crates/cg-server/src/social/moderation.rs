//! A project's social desk: the shared connections it follows, the
//! messages coming in (kept in memory), its rules (`rules`), what its
//! moderators approve, edit and reject, the hashtags it counts
//! (`hashtags`), and the pool of approved messages it turns on air.
//! Several moderators work at once: every change is sent to the others
//! (`Event::Social`), and actions are idempotent by message id.
//!
//! The desk never touches a channel: templates read it as the project data
//! source `social-<project id>` (`SourceValue`: the message on air, the
//! next, the pool, the hashtags), and the Operator takes the graphics
//! reading it on air like any other. The rotation goes to the next message
//! of the pool every N seconds; a message leaves the pool after its passes
//! or its time (the pool's rule, or its own).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cg_core::Project;
use serde::{Deserialize, Serialize};

use super::hashtags::{self, Hashtag};
use super::rules::{self, Rules, Status, Tag};
use super::{Author, Message, Platform};
use crate::api::{bad, not_found, require, visible_project, ApiResult};
use crate::auth::{Acting, CurrentUser, User};
use crate::state::{now_ms, Event, Shared};

/// Incoming messages kept per project.
pub const INCOMING: usize = 1000;
/// Ids of rejected and aired messages kept, so they never come back.
const FORGOTTEN: usize = 2000;
/// Texts kept to find duplicates.
const TEXTS: usize = 5000;
/// Messages kept after they left the pool (they can come back).
const RETIRED: usize = 200;

/// The data source templates read: `social-<project id>`.
pub fn source_id(project: &str) -> String {
    format!("social-{project}")
}

/// How the author is named on air.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameMode {
    /// "Sophie L."
    #[default]
    Initial,
    /// "Sophie Lefebvre"
    Full,
    /// No name.
    Anonymous,
}

/// The order the pool goes on air in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    /// As approved.
    #[default]
    Arrival,
    /// The starred ones first.
    Starred,
    Random,
}

/// When a message leaves the pool: after `airs` passes, or `minutes` after
/// it was approved (0: never); both 0: by hand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Leave {
    pub airs: u32,
    pub minutes: u32,
}

/// The pool on air: its rotation, order, how messages leave it, how authors
/// are named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Display {
    /// The next message by itself every `rotation_secs`.
    pub rotation: bool,
    pub rotation_secs: u32,
    pub name_mode: NameMode,
    pub order: Order,
    /// The pool's rule (a message may have its own).
    pub leave: Leave,
    /// Most messages in the pool (0: no limit): the oldest not starred leave.
    pub max: u32,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            rotation: true,
            rotation_secs: 8,
            name_mode: NameMode::Initial,
            order: Order::Arrival,
            leave: Leave::default(),
            max: 0,
        }
    }
}

/// A message on the desk: what the rules said of it, and what moderators did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub message: Message,
    pub status: Status,
    pub toxicity: f32,
    pub tags: Vec<Tag>,
    /// The author for moderators (phone numbers masked).
    pub author: String,
    /// The text for moderators: banned words masked.
    pub shown: String,
    /// Banned words found, as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<String>,
    /// The text on air after the rules.
    pub fixed: String,
    /// A moderator's text, on air instead of `fixed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<String>,
    /// A moderator's choice, instead of the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_mode: Option<NameMode>,
    #[serde(default)]
    pub starred: bool,
    /// Who approved it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
    /// Passes on air.
    #[serde(default)]
    pub airs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_air: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<i64>,
    /// Its own rule to leave the pool, instead of the pool's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leave: Option<Leave>,
    /// When it left the pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_at: Option<i64>,
    /// Same text came shortly before (kept to apply the rules again).
    #[serde(skip)]
    duplicate: bool,
    #[serde(skip)]
    author_messages: u32,
}

impl Card {
    fn new(message: Message, rules: &Rules, ctx: rules::Context) -> Self {
        let mut c = Self {
            author: author_label(&message.author),
            message,
            status: Status::Pending,
            toxicity: 0.0,
            tags: vec![],
            shown: String::new(),
            words: vec![],
            fixed: String::new(),
            edit: None,
            name_mode: None,
            starred: false,
            by: String::new(),
            airs: 0,
            last_air: None,
            approved_at: None,
            leave: None,
            retired_at: None,
            duplicate: ctx.duplicate,
            author_messages: ctx.author_messages,
        };
        c.apply(rules);
        c
    }

    fn apply(&mut self, rules: &Rules) {
        let v = rules::check(
            rules,
            &self.message,
            &rules::Context {
                duplicate: self.duplicate,
                author_messages: self.author_messages,
            },
        );
        self.status = v.status;
        self.toxicity = v.toxicity;
        self.tags = v.tags;
        self.shown = v.shown;
        self.words = v.words;
        self.fixed = v.fixed;
    }

    pub fn text(&self) -> &str {
        self.edit.as_deref().unwrap_or(&self.fixed)
    }

    fn id(&self) -> &str {
        &self.message.id
    }
}

/// What is kept of a project, in `<data>/social/projects/<id>.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    /// Shared connections followed.
    pub sources: Vec<String>,
    pub rules: Rules,
    pub hashtags: Vec<Hashtag>,
    pub display: Display,
    /// The pool, in the order they go on air (without the one on air).
    pub approved: Vec<Card>,
    pub on_air: Option<Card>,
    /// Left the pool, oldest first (they can come back).
    pub retired: VecDeque<Card>,
    pub rejected: VecDeque<String>,
    pub aired: VecDeque<String>,
}

/// A project's desk in memory.
#[derive(Default)]
struct Desk1 {
    saved: Saved,
    /// Newest last.
    incoming: VecDeque<Card>,
    /// Messages per author this session.
    authors: HashMap<String, u32>,
    /// (arrived, text as compared) to find duplicates.
    texts: VecDeque<(i64, String)>,
    /// When the rotation goes to the next message (ms).
    rotation_at: Option<i64>,
    /// Rejected by the toxicity score this session.
    tox_rejects: u32,
    /// Waiting for the next flush (`run`).
    new_cards: Vec<Card>,
    gone: Vec<String>,
    queue_changed: bool,
    /// Hashtags counted since the file was written.
    counted: bool,
    /// Hashtags as last sent.
    last_hashtags: Option<serde_json::Value>,
}

/// Where a message is on the desk.
#[derive(Debug, PartialEq)]
enum Place {
    Incoming(usize),
    Approved(usize),
    OnAir,
    Retired(usize),
}

impl Desk1 {
    fn place(&self, id: &str) -> Option<Place> {
        if self.saved.on_air.as_ref().is_some_and(|c| c.id() == id) {
            return Some(Place::OnAir);
        }
        if let Some(i) = self.saved.approved.iter().position(|c| c.id() == id) {
            return Some(Place::Approved(i));
        }
        if let Some(i) = self.incoming.iter().position(|c| c.id() == id) {
            return Some(Place::Incoming(i));
        }
        self.saved
            .retired
            .iter()
            .position(|c| c.id() == id)
            .map(Place::Retired)
    }

    fn card_mut(&mut self, id: &str) -> Option<&mut Card> {
        match self.place(id)? {
            Place::OnAir => self.saved.on_air.as_mut(),
            Place::Approved(i) => self.saved.approved.get_mut(i),
            Place::Incoming(i) => self.incoming.get_mut(i),
            Place::Retired(i) => self.saved.retired.get_mut(i),
        }
    }

    fn known(&self, id: &str) -> bool {
        self.place(id).is_some()
            || self.saved.rejected.iter().any(|x| x == id)
            || self.saved.aired.iter().any(|x| x == id)
    }

    /// A message of a followed connection comes in.
    fn ingest(&mut self, msg: &Message, now: i64) {
        if self.known(&msg.id) {
            return;
        }
        // Hashtags count everything, before moderation.
        let plain = rules::plain(&msg.text);
        for h in self.saved.hashtags.iter_mut() {
            self.counted |= h.count(&plain, msg.platform, msg.at, now);
        }
        let who = format!("{}:{}", msg.source_id, msg.author.id);
        let n = self.authors.entry(who).or_default();
        *n += 1;
        let author_messages = *n;
        let same: String = plain.chars().filter(|c| c.is_alphanumeric()).collect();
        let window = i64::from(self.saved.rules.duplicates.minutes) * 60_000;
        let duplicate = !same.is_empty()
            && self
                .texts
                .iter()
                .any(|(at, t)| *t == same && msg.at - *at < window);
        self.texts.push_back((msg.at, same));
        while self.texts.len() > TEXTS {
            self.texts.pop_front();
        }
        let card = Card::new(
            msg.clone(),
            &self.saved.rules,
            rules::Context {
                duplicate,
                author_messages,
            },
        );
        if card.status == Status::Rejected
            && card
                .tags
                .iter()
                .any(|t| t.kind == rules::TagKind::Tox && t.level == rules::Level::Bad)
        {
            self.tox_rejects += 1;
        }
        if card.status == Status::Approved {
            self.enter_pool(card, now);
            self.queue_changed = true;
            return;
        }
        self.new_cards.push(card.clone());
        self.incoming.push_back(card);
        while self.incoming.len() > INCOMING {
            if let Some(old) = self.incoming.pop_front() {
                self.gone.push(old.message.id);
            }
        }
    }

    fn forget(list: &mut VecDeque<String>, id: String) {
        list.push_back(id);
        while list.len() > FORGOTTEN {
            list.pop_front();
        }
    }

    fn approve(&mut self, id: &str, by: &str, now: i64) -> bool {
        let Some(Place::Incoming(i)) = self.place(id) else {
            return false;
        };
        let Some(mut c) = self.incoming.remove(i) else {
            return false;
        };
        c.status = Status::Approved;
        c.by = by.to_string();
        self.enter_pool(c, now);
        true
    }

    /// Into the pool, with a fresh clock; the pool's size rule applies.
    fn enter_pool(&mut self, mut c: Card, now: i64) {
        c.approved_at = Some(now);
        c.retired_at = None;
        self.saved.approved.push(c);
        self.cap(now);
    }

    /// The rule a message leaves the pool by.
    fn rule(&self, c: &Card) -> Leave {
        c.leave.unwrap_or(self.saved.display.leave)
    }

    fn airs_up(&self, c: &Card) -> bool {
        let r = self.rule(c);
        r.airs > 0 && c.airs >= r.airs
    }

    fn minutes_up(&self, c: &Card, now: i64) -> bool {
        let r = self.rule(c);
        r.minutes > 0
            && c.approved_at
                .is_some_and(|t| now - t >= i64::from(r.minutes) * 60_000)
    }

    /// Out of the pool, kept to come back.
    fn retire_card(&mut self, mut c: Card, now: i64) {
        c.retired_at = Some(now);
        Self::forget(&mut self.saved.aired, c.message.id.clone());
        self.saved.retired.push_back(c);
        while self.saved.retired.len() > RETIRED {
            self.saved.retired.pop_front();
        }
        self.queue_changed = true;
    }

    /// Messages whose time in the pool is up leave it (the one on air
    /// finishes its pass). True when some left.
    fn expire(&mut self, now: i64) -> bool {
        let mut gone = false;
        let mut i = 0;
        while i < self.saved.approved.len() {
            if self.minutes_up(&self.saved.approved[i], now) {
                let c = self.saved.approved.remove(i);
                self.retire_card(c, now);
                gone = true;
            } else {
                i += 1;
            }
        }
        gone
    }

    /// Too many in the pool: the oldest not starred leave (the oldest at all
    /// when every one is starred).
    fn cap(&mut self, now: i64) -> bool {
        let max = self.saved.display.max as usize;
        if max == 0 {
            return false;
        }
        let mut gone = false;
        while self.saved.approved.len() > max {
            let i = self
                .saved
                .approved
                .iter()
                .position(|c| !c.starred)
                .unwrap_or(0);
            let c = self.saved.approved.remove(i);
            self.retire_card(c, now);
            gone = true;
        }
        gone
    }

    /// The pass on air ends: back in the pool, or out of it.
    fn end_pass(&mut self, now: i64) {
        if let Some(c) = self.saved.on_air.take() {
            if self.airs_up(&c) || self.minutes_up(&c, now) {
                self.retire_card(c, now);
            } else {
                self.saved.approved.push(c);
            }
        }
    }

    /// The next message of the pool, by the order.
    fn pick_next(&mut self, now: i64) -> Option<Card> {
        if self.saved.approved.is_empty() {
            return None;
        }
        let i = match self.saved.display.order {
            Order::Arrival => 0,
            Order::Starred => self
                .saved
                .approved
                .iter()
                .position(|c| c.starred)
                .unwrap_or(0),
            Order::Random => {
                let n = self.saved.approved.len() as u64;
                ((now as u64).wrapping_mul(6_364_136_223_846_793_005) >> 33) as usize % n as usize
            }
        };
        Some(self.saved.approved.remove(i))
    }

    /// SUIVANT: the next message on air (only while `from`, when given, is
    /// still the one on air). False when nothing changed.
    fn advance(&mut self, from: Option<&str>, now: i64) -> bool {
        if from.is_some() && from != self.saved.on_air.as_ref().map(|c| c.id()) {
            return false;
        }
        self.expire(now);
        if self.saved.approved.is_empty() {
            // The one on air stays (its pass goes on).
            self.rotation_at = self
                .saved
                .display
                .rotation
                .then(|| now + self.rotation_ms());
            return false;
        }
        self.end_pass(now);
        let Some(mut c) = self.pick_next(now) else {
            return false;
        };
        c.airs += 1;
        c.last_air = Some(now);
        self.saved.on_air = Some(c);
        self.queue_changed = true;
        self.rotation_at = self
            .saved
            .display
            .rotation
            .then(|| now + self.rotation_ms());
        true
    }

    /// Nothing on air while the pool has messages: the first goes on air.
    fn fill_air(&mut self, now: i64) -> bool {
        self.saved.on_air.is_none() && !self.saved.approved.is_empty() && self.advance(None, now)
    }

    /// Out of the pool by hand (the one on air goes too: the next comes).
    fn retire(&mut self, id: &str, now: i64) -> bool {
        match self.place(id) {
            Some(Place::Approved(i)) => {
                let c = self.saved.approved.remove(i);
                self.retire_card(c, now);
                true
            }
            Some(Place::OnAir) => {
                let c = self.saved.on_air.take().unwrap();
                self.retire_card(c, now);
                self.fill_air(now);
                true
            }
            _ => false,
        }
    }

    /// Back in the pool, with a fresh clock and its passes kept.
    fn restore(&mut self, id: &str, now: i64) -> bool {
        let Some(Place::Retired(i)) = self.place(id) else {
            return false;
        };
        let Some(mut c) = self.saved.retired.remove(i) else {
            return false;
        };
        c.status = Status::Approved;
        if self.airs_up(&c) {
            // Its passes are spent: counted again from here.
            c.airs = 0;
        }
        self.enter_pool(c, now);
        self.queue_changed = true;
        true
    }

    fn reject(&mut self, id: &str) -> bool {
        let c = match self.place(id) {
            Some(Place::Incoming(i)) => self.incoming.remove(i),
            Some(Place::Approved(i)) => Some(self.saved.approved.remove(i)),
            Some(Place::Retired(i)) => self.saved.retired.remove(i),
            _ => None,
        };
        let Some(c) = c else { return false };
        Self::forget(&mut self.saved.rejected, c.message.id);
        true
    }

    /// Out of the queue, back with the incoming ones.
    fn unapprove(&mut self, id: &str) -> bool {
        let Some(Place::Approved(i)) = self.place(id) else {
            return false;
        };
        let mut c = self.saved.approved.remove(i);
        c.apply(&self.saved.rules);
        if c.status == Status::Approved {
            c.status = Status::Pending;
        }
        c.by.clear();
        self.incoming.push_back(c);
        true
    }

    /// The approved queue in this order (ids left out keep theirs, after).
    fn reorder(&mut self, ids: &[String]) -> bool {
        let before: Vec<String> = self
            .saved
            .approved
            .iter()
            .map(|c| c.id().to_string())
            .collect();
        let mut rest = std::mem::take(&mut self.saved.approved);
        let mut out = Vec::with_capacity(rest.len());
        for id in ids {
            if let Some(i) = rest.iter().position(|c| c.id() == id) {
                out.push(rest.remove(i));
            }
        }
        out.extend(rest);
        self.saved.approved = out;
        before
            .iter()
            .zip(&self.saved.approved)
            .any(|(a, c)| a != c.id())
    }

    fn rotation_ms(&self) -> i64 {
        i64::from(self.saved.display.rotation_secs.clamp(2, 3600)) * 1000
    }

    fn settings(&self) -> Box<Settings> {
        Box::new(Settings {
            sources: self.saved.sources.clone(),
            rules: self.saved.rules.clone(),
            display: self.saved.display.clone(),
        })
    }

    fn queue(&self) -> Change {
        Change::Queue {
            approved: self.saved.approved.clone(),
            on_air: self.saved.on_air.clone().map(Box::new),
            retired: self.saved.retired.iter().cloned().collect(),
            rotation_at: self.rotation_at,
        }
    }

    fn hashtag_views(&mut self, now: i64) -> Vec<hashtags::View> {
        self.saved
            .hashtags
            .iter_mut()
            .map(|h| h.view(now))
            .collect()
    }

    fn value(&mut self, now: i64, tz: &jiff::tz::TimeZone) -> SourceValue {
        let mode = self.saved.display.name_mode;
        let msg = |c: &Card| Msg::of(c, c.name_mode.unwrap_or(mode), tz);
        SourceValue {
            on_air: self.saved.on_air.as_ref().map(msg),
            next: self.saved.approved.first().map(msg),
            approved: self.saved.approved.iter().map(msg).collect(),
            count: self.saved.approved.len(),
            hashtags: self
                .saved
                .hashtags
                .iter_mut()
                .map(|h| (h.key().to_string(), h.stats(now)))
                .collect(),
        }
    }
}

/// Connections followed, rules and display options.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub sources: Vec<String>,
    pub rules: Rules,
    pub display: Display,
}

/// A change of a project's desk, sent to everyone who sees the project.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    /// Messages new in (or changed in) the incoming list.
    Incoming {
        cards: Vec<Card>,
    },
    /// Messages gone from the incoming list.
    Removed {
        ids: Vec<String>,
    },
    /// The approved queue and what is on air.
    #[serde(rename_all = "camelCase")]
    Queue {
        approved: Vec<Card>,
        on_air: Option<Box<Card>>,
        retired: Vec<Card>,
        rotation_at: Option<i64>,
    },
    Settings {
        settings: Box<Settings>,
    },
    Hashtags {
        hashtags: Vec<hashtags::View>,
    },
    /// Much changed (rules applied again, connections followed): read it again.
    Reload,
}

// ------------------------------------------------------- data source ----

/// A message as templates read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Msg {
    pub id: String,
    /// "youtube", "x"…
    pub platform: String,
    /// "YouTube", "X"…
    pub platform_name: String,
    /// After the name mode.
    pub name: String,
    pub handle: String,
    pub city: String,
    /// After edits and masking.
    pub text: String,
    pub avatar: String,
    /// The first photo, or "".
    pub photo: String,
    pub amount: String,
    /// When it came (ms).
    pub at: i64,
    /// "20:14", the server's time zone.
    pub time: String,
}

impl Msg {
    fn of(c: &Card, mode: NameMode, tz: &jiff::tz::TimeZone) -> Self {
        let m = &c.message;
        let handle = match mode {
            NameMode::Full if !m.author.handle.is_empty() => {
                format!("@{}", m.author.handle.trim_start_matches('@'))
            }
            _ => String::new(),
        };
        Self {
            id: m.id.clone(),
            platform: hashtags::platform_key(m.platform),
            platform_name: platform_name(m.platform).into(),
            name: display_name(&m.author, mode),
            handle,
            city: m.author.city.clone(),
            text: c.text().to_string(),
            avatar: if mode == NameMode::Anonymous {
                String::new()
            } else {
                m.author.avatar.clone()
            },
            photo: m.photos.first().cloned().unwrap_or_default(),
            amount: m.amount.clone(),
            at: m.at,
            time: local_time(m.at, tz),
        }
    }
}

/// The value of `social-<project id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceValue {
    pub on_air: Option<Msg>,
    pub next: Option<Msg>,
    /// In queue order, without the one on air.
    pub approved: Vec<Msg>,
    pub count: usize,
    /// By tag without its "#".
    pub hashtags: BTreeMap<String, hashtags::Stats>,
}

pub fn platform_name(p: Platform) -> &'static str {
    match p {
        Platform::Youtube => "YouTube",
        Platform::Twitch => "Twitch",
        Platform::Facebook => "Facebook",
        Platform::Instagram => "Instagram",
        Platform::X => "X",
        Platform::Tiktok => "TikTok",
        Platform::Whatsapp => "WhatsApp",
        Platform::Sms => "SMS",
    }
}

/// A phone number as a name (WhatsApp, SMS), masked or not.
fn is_phone(name: &str) -> bool {
    let digits = name.chars().filter(|c| c.is_ascii_digit()).count();
    let hidden = name.chars().filter(|c| *c == '•').count();
    digits >= 4
        && digits + hidden >= 8
        && name
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '.' | '-' | '(' | ')' | '•'))
}

/// The author for moderators: "+33 6 •• •• 41 07" for a phone number.
fn author_label(a: &Author) -> String {
    let name = a.name.trim();
    if name.is_empty() {
        return if a.handle.is_empty() {
            a.id.clone()
        } else {
            format!("@{}", a.handle.trim_start_matches('@'))
        };
    }
    if !is_phone(name) {
        return name.to_string();
    }
    let total = name.chars().filter(|c| c.is_ascii_digit()).count();
    let mut seen = 0;
    name.chars()
        .map(|c| {
            if !c.is_ascii_digit() {
                return c;
            }
            seen += 1;
            // The country code and first digit, and the last four.
            if seen <= 3 || seen > total - 4 {
                c
            } else {
                '•'
            }
        })
        .collect()
}

/// The author's name on air.
pub fn display_name(a: &Author, mode: NameMode) -> String {
    let name = a.name.trim();
    let name = if name.is_empty() {
        a.handle.trim()
    } else {
        name
    };
    if mode == NameMode::Anonymous || is_phone(name) {
        return String::new();
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    if mode == NameMode::Full || name.starts_with('@') || words.len() < 2 {
        return name.to_string();
    }
    let last = words[words.len() - 1];
    let initial: String = last.chars().take(1).flat_map(char::to_uppercase).collect();
    format!("{} {initial}.", words[0])
}

fn local_time(at: i64, tz: &jiff::tz::TimeZone) -> String {
    jiff::Timestamp::from_millisecond(at)
        .map(|t| t.to_zoned(tz.clone()).strftime("%H:%M").to_string())
        .unwrap_or_default()
}

fn timezone(s: &Shared) -> jiff::tz::TimeZone {
    let name = s.config.timezone.trim();
    if !name.is_empty() {
        if let Ok(tz) = jiff::tz::TimeZone::get(name) {
            return tz;
        }
    }
    jiff::tz::TimeZone::system()
}

// ------------------------------------------------------------- desks ----

/// Every project's desk.
pub struct Desk {
    dir: PathBuf,
    projects: Mutex<HashMap<String, Desk1>>,
}

impl Desk {
    /// Every project's file in `<data>/social/projects/`.
    pub fn load(data_dir: &Path) -> Self {
        let dir = data_dir.join("social").join("projects");
        let mut projects = HashMap::new();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = e.path();
            let (Some(id), Some("json")) = (
                path.file_stem().and_then(|s| s.to_str()),
                path.extension().and_then(|s| s.to_str()),
            ) else {
                continue;
            };
            match std::fs::read(&path).map(|b| serde_json::from_slice::<Saved>(&b)) {
                Ok(Ok(saved)) => {
                    projects.insert(
                        id.to_string(),
                        Desk1 {
                            saved,
                            ..Default::default()
                        },
                    );
                }
                Ok(Err(e)) => tracing::warn!("social: cannot read {}: {e}", path.display()),
                Err(e) => tracing::warn!("social: cannot read {}: {e}", path.display()),
            }
        }
        Self {
            dir,
            projects: Mutex::new(projects),
        }
    }

    fn with<R>(&self, project: &str, f: impl FnOnce(&mut Desk1) -> R) -> R {
        let mut all = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        f(all.entry(project.to_string()).or_default())
    }

    /// Changes a project's desk and writes it.
    fn edit<R>(&self, project: &str, f: impl FnOnce(&mut Desk1) -> R) -> R {
        let mut all = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        let d = all.entry(project.to_string()).or_default();
        let r = f(d);
        d.counted = false;
        if let Err(e) = self.write(project, &d.saved) {
            tracing::warn!("social: cannot save {project}: {e}");
        }
        r
    }

    fn write(&self, project: &str, saved: &Saved) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{project}.json"));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(saved)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    /// A new message: into every project following its connection.
    pub fn incoming(&self, msg: &Message) {
        let now = now_ms();
        let mut all = self.projects.lock().unwrap_or_else(|e| e.into_inner());
        for d in all.values_mut() {
            if d.saved.sources.contains(&msg.source_id) {
                d.ingest(msg, now);
            }
        }
    }

    /// The project's desk as the Social page reads it.
    pub fn view(&self, project: &str) -> View {
        let now = now_ms();
        self.with(project, |d| View {
            source_id: source_id(project),
            sources: d.saved.sources.clone(),
            rules: d.saved.rules.clone(),
            display: d.saved.display.clone(),
            hashtags: d.hashtag_views(now),
            incoming: d.incoming.iter().cloned().collect(),
            approved: d.saved.approved.clone(),
            on_air: d.saved.on_air.clone(),
            retired: d.saved.retired.iter().cloned().collect(),
            rotation_at: d.rotation_at,
            tox_rejects: d.tox_rejects,
            builtin_words: rules::builtin_count(),
        })
    }
}

/// A project's desk, as the Social page reads it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub source_id: String,
    pub sources: Vec<String>,
    pub rules: Rules,
    pub display: Display,
    pub hashtags: Vec<hashtags::View>,
    /// Oldest first.
    pub incoming: Vec<Card>,
    pub approved: Vec<Card>,
    pub on_air: Option<Card>,
    pub retired: Vec<Card>,
    pub rotation_at: Option<i64>,
    pub tox_rejects: u32,
    pub builtin_words: usize,
}

fn send(s: &Shared, project: &str, change: Change) {
    let _ = s.events.send(Event::Social {
        project_id: project.to_string(),
        change: Box::new(change),
    });
}

/// Creates the project's data source (a push JSON source of the project).
fn ensure_source(s: &Shared, project: &str) {
    let id = source_id(project);
    if s.data.get(&id).is_some() {
        return;
    }
    // From JSON: the fields left out take their defaults.
    let src = serde_json::json!({
        "id": id,
        "name": "Modération",
        "project": project,
        "type": "push",
        "format": "json",
    });
    let src: crate::datasources::Source = match serde_json::from_value(src) {
        Ok(src) => src,
        Err(e) => return tracing::warn!("social: data source: {e}"),
    };
    match s.data.put(src) {
        Ok(()) => s.changed("sources"),
        Err(e) => tracing::warn!("social: cannot create the data source of {project}: {e}"),
    }
}

/// Writes the source's value; true when it changed. Graphics are only
/// refreshed by `datasources::updated` (or a command) after.
fn record(s: &Shared, project: &str) -> bool {
    let tz = timezone(s);
    let now = now_ms();
    let (value, follows) = s.moderation.with(project, |d| {
        let v = d.value(now, &tz);
        d.last_hashtags = serde_json::to_value(&v.hashtags).ok();
        (v, !d.saved.sources.is_empty())
    });
    let id = source_id(project);
    if s.data.get(&id).is_none() {
        if !follows {
            return false;
        }
        ensure_source(s, project);
    }
    let Ok(json) = serde_json::to_value(&value) else {
        return false;
    };
    let bytes = json.to_string().len();
    s.data.record(&id, Ok((json, bytes)))
}

/// Writes the source's value and refreshes the graphics reading it.
async fn publish(s: &Shared, project: &str) {
    let changed = record(s, project);
    crate::datasources::updated(s, &source_id(project), changed).await;
}

// ---------------------------------------------------------------- pool ----

/// A change of the pool: everyone sees it, and the source is written.
async fn pool_changed(s: &Shared, project: &str) {
    flush_one(s, project);
    publish(s, project).await;
}

/// Sends what changed since the last flush (new messages, the queue).
fn flush_one(s: &Shared, project: &str) -> bool {
    let (cards, gone, queue) = s.moderation.with(project, |d| {
        let queue = std::mem::take(&mut d.queue_changed).then(|| d.queue());
        (
            std::mem::take(&mut d.new_cards),
            std::mem::take(&mut d.gone),
            queue,
        )
    });
    if !gone.is_empty() {
        send(s, project, Change::Removed { ids: gone });
    }
    if !cards.is_empty() {
        send(s, project, Change::Incoming { cards });
    }
    let q = queue.is_some();
    if let Some(q) = queue {
        send(s, project, q);
    }
    q
}

/// Brings the messages of the followed connections to the projects,
/// turns the rotations, and sends the hashtags once a second. Runs for
/// an engine cycle.
pub async fn run(s: Shared) {
    let mut rx = s.social.subscribe();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_second = 0;
    let mut last_save = now_ms();
    loop {
        tokio::select! {
            m = rx.recv() => match m {
                Ok(msg) => s.moderation.incoming(&msg),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("social: {n} messages missed (too many at once)");
                }
                Err(_) => return,
            },
            _ = tick.tick() => {
                let now = now_ms();
                let projects: Vec<String> = s.moderation.with_all(|all| all.keys().cloned().collect());
                for p in &projects {
                    // Trusted accounts approved by themselves change the queue.
                    if flush_one(&s, p) {
                        s.moderation.edit(p, |_| ());
                        publish(&s, p).await;
                    }
                }
                rotate(&s, &projects, now).await;
                if now - last_second >= 1000 {
                    last_second = now;
                    hashtags_tick(&s, &projects, now, now - last_save >= 10_000).await;
                    if now - last_save >= 10_000 {
                        last_save = now;
                    }
                }
            }
        }
    }
}

impl Desk {
    fn with_all<R>(&self, f: impl FnOnce(&HashMap<String, Desk1>) -> R) -> R {
        f(&self.projects.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// The pool turns: messages whose time is up leave, nothing on air while
/// the pool has messages is filled, the rotation goes to the next one.
async fn rotate(s: &Shared, projects: &[String], now: i64) {
    for p in projects {
        let changed = s.moderation.with(p, |d| {
            let mut changed = d.expire(now);
            changed |= d.fill_air(now);
            let due = d.saved.display.rotation && d.rotation_at.is_some_and(|t| t <= now);
            if due {
                changed |= d.advance(None, now);
            }
            changed
        });
        if changed {
            s.moderation.edit(p, |_| ());
            pool_changed(s, p).await;
        }
    }
}

async fn hashtags_tick(s: &Shared, projects: &[String], now: i64, save: bool) {
    for p in projects {
        let (views, changed, counted) = s.moderation.with(p, |d| {
            if d.saved.hashtags.is_empty() {
                return (vec![], false, false);
            }
            let stats: BTreeMap<String, hashtags::Stats> = d
                .saved
                .hashtags
                .iter_mut()
                .map(|h| (h.key().to_string(), h.stats(now)))
                .collect();
            let json = serde_json::to_value(&stats).ok();
            let changed = json != d.last_hashtags;
            (
                if changed {
                    d.hashtag_views(now)
                } else {
                    vec![]
                },
                changed,
                d.counted,
            )
        });
        if changed {
            send(s, p, Change::Hashtags { hashtags: views });
            publish(s, p).await;
        }
        if save && counted {
            s.moderation.edit(p, |_| ());
        }
    }
}

// ---------------------------------------------------------------- API ----

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api/projects/{id}/social", get(get_desk))
        .route("/api/projects/{id}/social/follow", put(put_follow))
        .route("/api/projects/{id}/social/rules", put(put_rules))
        .route("/api/projects/{id}/social/hashtags", put(put_hashtags))
        .route("/api/projects/{id}/social/display", put(put_display))
        .route("/api/projects/{id}/social/{action}", post(post_action))
}

/// Moderators: who sees the project and may operate.
async fn moderator(s: &Shared, me: &User, project: &str) -> ApiResult<std::sync::Arc<Project>> {
    let p = visible_project(s, me, project).await?;
    require(me.can_edit_rundown(project))?;
    Ok(p)
}

async fn get_desk(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<View>> {
    visible_project(&s, &me, &id).await?;
    // The project's source exists from the first look at its Social page:
    // the Designer has it to wire before any connection is followed.
    if s.data.get(&source_id(&id)).is_none() {
        ensure_source(&s, &id);
        record(&s, &id);
    }
    Ok(Json(s.moderation.view(&id)))
}

#[derive(Deserialize)]
struct Follow {
    sources: Vec<String>,
}

async fn put_follow(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(f): Json<Follow>,
) -> ApiResult<StatusCode> {
    moderator(&s, &me, &id).await?;
    let mut sources: Vec<String> = Vec::new();
    for src in f.sources {
        if s.social.get(&src).is_none() {
            return Err(not_found("social connection"));
        }
        if !sources.contains(&src) {
            sources.push(src);
        }
    }
    follow(&s, &id, sources).await;
    Ok(StatusCode::NO_CONTENT)
}

/// The connections a project follows: the last messages of new ones come
/// in, the incoming ones of those left go.
pub async fn follow(s: &Shared, project: &str, sources: Vec<String>) {
    let mut recent: Vec<Message> = sources
        .iter()
        .filter(|x| !s.moderation.with(project, |d| d.saved.sources.contains(x)))
        .flat_map(|x| s.social.recent(x))
        .collect();
    recent.sort_by_key(|m| m.at);
    let now = now_ms();
    s.moderation.edit(project, |d| {
        d.saved.sources = sources;
        let keep = d.saved.sources.clone();
        d.incoming.retain(|c| keep.contains(&c.message.source_id));
        for m in &recent {
            d.ingest(m, now);
        }
        d.new_cards.clear();
        d.gone.clear();
    });
    if !s.moderation.with(project, |d| d.saved.sources.is_empty()) {
        ensure_source(s, project);
    }
    let settings = s.moderation.with(project, |d| d.settings());
    send(s, project, Change::Settings { settings });
    send(s, project, Change::Reload);
    // The connections read their platform only while followed.
    s.social.changed.notify_one();
    publish(s, project).await;
}

async fn put_rules(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(mut rules): Json<Rules>,
) -> ApiResult<StatusCode> {
    moderator(&s, &me, &id).await?;
    rules.normalize();
    let settings = s.moderation.edit(&id, |d| {
        d.saved.rules = rules;
        // The messages waiting are read again with them.
        let rules = d.saved.rules.clone();
        for c in d.incoming.iter_mut() {
            c.apply(&rules);
            if c.status == Status::Approved {
                c.status = Status::Pending;
            }
        }
        d.settings()
    });
    send(&s, &id, Change::Settings { settings });
    send(&s, &id, Change::Reload);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct HashtagIn {
    tag: String,
    #[serde(default = "yes")]
    running: bool,
}
fn yes() -> bool {
    true
}

async fn put_hashtags(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(list): Json<Vec<HashtagIn>>,
) -> ApiResult<StatusCode> {
    moderator(&s, &me, &id).await?;
    let now = now_ms();
    let views = s.moderation.edit(&id, |d| {
        let mut old = std::mem::take(&mut d.saved.hashtags);
        for h in list {
            let tag = hashtags::clean(&h.tag);
            if tag.len() < 2
                || d.saved
                    .hashtags
                    .iter()
                    .any(|x| rules::plain(&x.tag) == rules::plain(&tag))
            {
                continue;
            }
            let mut kept = match old
                .iter()
                .position(|x| rules::plain(&x.tag) == rules::plain(&tag))
            {
                Some(i) => old.remove(i),
                None => Hashtag::new(&tag, now),
            };
            kept.tag = tag;
            kept.running = h.running;
            d.saved.hashtags.push(kept);
        }
        d.hashtag_views(now)
    });
    send(&s, &id, Change::Hashtags { hashtags: views });
    publish(&s, &id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn put_display(
    State(s): State<Shared>,
    CurrentUser(me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(mut display): Json<Display>,
) -> ApiResult<StatusCode> {
    moderator(&s, &me, &id).await?;
    display.rotation_secs = display.rotation_secs.clamp(2, 3600);
    display.max = display.max.min(10_000);
    let now = now_ms();
    let settings = s.moderation.edit(&id, |d| {
        d.saved.display = display;
        d.rotation_at = (d.saved.display.rotation && d.saved.on_air.is_some())
            .then(|| d.rotation_at.unwrap_or(now + d.rotation_ms()));
        d.cap(now);
        d.expire(now);
        d.queue_changed = true;
        d.settings()
    });
    send(&s, &id, Change::Settings { settings });
    pool_changed(&s, &id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ActionIn {
    id: Option<String>,
    ids: Vec<String>,
    text: Option<String>,
    name_mode: Option<NameMode>,
    on: Option<bool>,
    secs: Option<u32>,
    /// SUIVANT: only while this one is on air.
    from: Option<String>,
    /// Hashtag reset.
    tag: Option<String>,
    /// A message's own rule to leave the pool (`common`: the pool's).
    airs: Option<u32>,
    minutes: Option<u32>,
    common: Option<bool>,
}

/// approve, reject, edit, star, remove, reorder, next, retire, restore,
/// rule, rotation, reset.
async fn post_action(
    State(s): State<Shared>,
    Acting(me, actor): Acting,
    UrlPath((id, action)): UrlPath<(String, String)>,
    Json(a): Json<ActionIn>,
) -> ApiResult<StatusCode> {
    moderator(&s, &me, &id).await?;
    let msg = || a.id.clone().ok_or_else(|| bad("which message (id)?"));
    let now = now_ms();
    match action.as_str() {
        "approve" => {
            let m = msg()?;
            if s.moderation.edit(&id, |d| {
                let ok = d.approve(&m, &actor.name, now);
                if ok {
                    d.gone.push(m.clone());
                    d.fill_air(now);
                    d.queue_changed = true;
                }
                ok
            }) {
                pool_changed(&s, &id).await;
            }
        }
        "reject" => {
            let m = msg()?;
            let was = s.moderation.with(&id, |d| d.place(&m));
            if s.moderation.edit(&id, |d| d.reject(&m)) {
                s.moderation.with(&id, |d| match was {
                    Some(Place::Incoming(_)) => d.gone.push(m),
                    _ => d.queue_changed = true,
                });
                if flush_one(&s, &id) {
                    publish(&s, &id).await;
                }
            }
        }
        "remove" => {
            let m = msg()?;
            let card = s.moderation.edit(&id, |d| {
                d.unapprove(&m).then(|| {
                    d.queue_changed = true;
                    d.incoming.back().cloned()
                })
            });
            if let Some(c) = card.flatten() {
                s.moderation.with(&id, |d| d.new_cards.push(c));
                flush_one(&s, &id);
                publish(&s, &id).await;
            }
        }
        "edit" => {
            let m = msg()?;
            let res = s.moderation.edit(&id, |d| {
                let place = d.place(&m)?;
                let mode = d.saved.display.name_mode;
                let c = d.card_mut(&m)?;
                if let Some(t) = &a.text {
                    let t = t.trim().to_string();
                    c.edit = (!t.is_empty() && t != c.fixed).then_some(t);
                }
                if let Some(n) = a.name_mode {
                    c.name_mode = (n != mode).then_some(n);
                }
                let c = c.clone();
                match place {
                    Place::Incoming(_) => d.new_cards.push(c),
                    _ => d.queue_changed = true,
                }
                Some(place)
            });
            let Some(place) = res else {
                return Err(not_found("message"));
            };
            flush_one(&s, &id);
            if !matches!(place, Place::Incoming(_)) {
                publish(&s, &id).await;
            }
        }
        "star" => {
            let m = msg()?;
            let place = s.moderation.edit(&id, |d| {
                let place = d.place(&m)?;
                let c = d.card_mut(&m)?;
                c.starred = a.on.unwrap_or(!c.starred);
                let c = c.clone();
                match place {
                    Place::Incoming(_) => d.new_cards.push(c),
                    _ => d.queue_changed = true,
                }
                Some(place)
            });
            if place.is_none() {
                return Err(not_found("message"));
            }
            flush_one(&s, &id);
        }
        "reorder" => {
            if s.moderation.edit(&id, |d| d.reorder(&a.ids)) {
                s.moderation.with(&id, |d| d.queue_changed = true);
                flush_one(&s, &id);
                publish(&s, &id).await;
            }
        }
        "next" => {
            if s.moderation
                .edit(&id, |d| d.advance(a.from.as_deref(), now))
            {
                pool_changed(&s, &id).await;
            }
        }
        "retire" => {
            let m = msg()?;
            if s.moderation.edit(&id, |d| d.retire(&m, now)) {
                pool_changed(&s, &id).await;
            }
        }
        "restore" => {
            let m = msg()?;
            if s.moderation.edit(&id, |d| d.restore(&m, now)) {
                pool_changed(&s, &id).await;
            }
        }
        "rule" => {
            let m = msg()?;
            let ok = s.moderation.edit(&id, |d| {
                let place = d.place(&m)?;
                let c = d.card_mut(&m)?;
                c.leave = if a.common.unwrap_or(false) {
                    None
                } else {
                    Some(Leave {
                        airs: a.airs.unwrap_or(0),
                        minutes: a.minutes.unwrap_or(0),
                    })
                };
                let c = c.clone();
                match place {
                    Place::Incoming(_) => d.new_cards.push(c),
                    _ => d.queue_changed = true,
                }
                d.expire(now);
                Some(())
            });
            if ok.is_none() {
                return Err(not_found("message"));
            }
            pool_changed(&s, &id).await;
        }
        "rotation" => {
            let settings = s.moderation.edit(&id, |d| {
                let disp = &mut d.saved.display;
                disp.rotation = a.on.unwrap_or(!disp.rotation);
                if let Some(n) = a.secs {
                    disp.rotation_secs = n.clamp(2, 3600);
                }
                d.rotation_at = (d.saved.display.rotation && d.saved.on_air.is_some())
                    .then(|| now + d.rotation_ms());
                d.queue_changed = true;
                d.settings()
            });
            send(&s, &id, Change::Settings { settings });
            flush_one(&s, &id);
        }
        "reset" => {
            let tag = a.tag.clone().ok_or_else(|| bad("which hashtag (tag)?"))?;
            let views = s.moderation.edit(&id, |d| {
                let want = rules::plain(&hashtags::clean(&tag));
                let h = d
                    .saved
                    .hashtags
                    .iter_mut()
                    .find(|h| rules::plain(&h.tag) == want)?;
                h.reset(now);
                Some(d.hashtag_views(now))
            });
            let views = views.ok_or_else(|| not_found("hashtag"))?;
            send(&s, &id, Change::Hashtags { hashtags: views });
            publish(&s, &id).await;
        }
        _ => return Err(not_found("action")),
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::SocialSource;
    use std::sync::Arc;

    fn author(name: &str) -> Author {
        Author {
            id: format!("id-{name}"),
            name: name.into(),
            handle: "sophie_l".into(),
            city: "Lyon".into(),
            ..Default::default()
        }
    }

    fn message(source: &str, n: u32, name: &str, text: &str) -> Message {
        Message {
            id: format!("{source}:{n}"),
            source_id: source.into(),
            platform: Platform::Youtube,
            author: author(name),
            text: text.into(),
            photos: vec![],
            at: now_ms() + i64::from(n),
            amount: String::new(),
            url: String::new(),
        }
    }

    #[test]
    fn names_on_air_follow_the_mode() {
        let a = author("Sophie Lefebvre");
        assert_eq!(display_name(&a, NameMode::Initial), "Sophie L.");
        assert_eq!(display_name(&a, NameMode::Full), "Sophie Lefebvre");
        assert_eq!(display_name(&a, NameMode::Anonymous), "");
        assert_eq!(
            display_name(&author("Jean-Marc ollivier"), NameMode::Initial),
            "Jean-Marc O."
        );
        assert_eq!(
            display_name(&author("kevin_bzh"), NameMode::Initial),
            "kevin_bzh"
        );
        assert_eq!(
            display_name(&author("@claire.v"), NameMode::Initial),
            "@claire.v"
        );
        // A phone number is never a name on air, and is masked for moderators.
        let phone = author("+33 6 12 34 41 07");
        assert_eq!(display_name(&phone, NameMode::Full), "");
        assert_eq!(author_label(&phone), "+33 6 •• •• 41 07");
        assert_eq!(
            display_name(&author("+33 6 •• •• 41 07"), NameMode::Initial),
            ""
        );
    }

    #[test]
    fn the_desk_moves_messages_and_the_source_keeps_its_contract() {
        let mut d = Desk1::default();
        d.saved.sources = vec!["yt".into()];
        d.saved.hashtags = vec![Hashtag::new("#JT20H", 0)];
        d.saved.rules.trusted.accounts = vec!["Jean-Marc Ollivier".into()];
        let now = now_ms();
        let m1 = message(
            "yt",
            1,
            "Sophie Lefebvre",
            "Pourquoi la réforme ne concerne pas les indépendants ? #jt20h",
        );
        let m2 = message("yt", 2, "kevin_bzh", "Encore des connards sur ce plateau");
        let m3 = message(
            "yt",
            3,
            "Jean-Marc Ollivier",
            "Bravo pour l’invité, très clair #JT20H",
        );
        for m in [&m1, &m2, &m3, &m1] {
            d.ingest(m, now);
        }
        // Once each; the trusted one straight to the pool, the insult rejected.
        assert_eq!(d.incoming.len(), 2);
        assert_eq!(d.incoming[1].status, Status::Rejected);
        assert_eq!(d.tox_rejects, 1);
        assert_eq!(d.saved.approved.len(), 1);
        assert_eq!(d.saved.approved[0].approved_at, Some(now));
        assert!(d.queue_changed);
        // Idempotent actions.
        assert!(d.approve("yt:1", "Camille", now));
        assert!(!d.approve("yt:1", "Camille", now));
        assert!(d.reject("yt:2"));
        assert!(!d.reject("yt:2"));
        d.ingest(&m2, now);
        assert!(d.incoming.is_empty(), "a rejected message never comes back");
        assert!(d.reorder(&["yt:1".into(), "yt:3".into()]));
        assert!(!d.reorder(&["yt:1".into()]));
        let c = d.card_mut("yt:1").unwrap();
        c.edit = Some("Pourquoi la réforme ne concerne pas les indépendants ?".into());
        c.starred = true;
        // Nothing on air: the first of the pool goes.
        assert!(d.fill_air(now));
        assert!(!d.fill_air(now));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:1");
        assert_eq!(d.saved.on_air.as_ref().unwrap().airs, 1);
        // SUIVANT only while `from` is still on air.
        assert!(!d.advance(Some("other"), now));
        let tz = jiff::tz::TimeZone::UTC;
        let v = d.value(now, &tz);
        let json = serde_json::to_value(&v).unwrap();
        let keys = |v: &serde_json::Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(
            keys(&json),
            ["approved", "count", "hashtags", "next", "onAir"]
        );
        assert_eq!(
            keys(&json["onAir"]),
            [
                "amount",
                "at",
                "avatar",
                "city",
                "handle",
                "id",
                "name",
                "photo",
                "platform",
                "platformName",
                "text",
                "time"
            ]
        );
        assert_eq!(json["onAir"]["name"], "Sophie L.");
        assert_eq!(json["onAir"]["platform"], "youtube");
        assert_eq!(json["onAir"]["platformName"], "YouTube");
        assert_eq!(
            json["onAir"]["text"],
            "Pourquoi la réforme ne concerne pas les indépendants ?"
        );
        assert_eq!(json["onAir"]["time"].as_str().unwrap().len(), 5);
        assert_eq!(json["next"]["id"], "yt:3");
        assert_eq!(json["approved"].as_array().unwrap().len(), 1);
        assert_eq!(json["count"], 1);
        let h = &json["hashtags"]["JT20H"];
        assert_eq!(
            (h["tag"].as_str(), h["count"].as_u64()),
            (Some("#JT20H"), Some(2))
        );
        assert_eq!(h["series"].as_array().unwrap().len(), 60);
        assert_eq!(h["perMin"], 2);
        assert_eq!(h["platforms"]["youtube"], 100);
        // Read back as the contract says.
        let back: SourceValue = serde_json::from_value(json).unwrap();
        assert_eq!(back, v);
        // SUIVANT: no rule to leave, the message goes back to the end of the pool.
        assert!(d.advance(Some("yt:1"), now));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:3");
        assert_eq!(d.saved.approved[0].id(), "yt:1");
        assert!(d.saved.retired.is_empty());
        // The pool's rule: one pass. yt:3 leaves at the next SUIVANT; yt:1 (two
        // passes then) leaves after.
        d.saved.display.leave.airs = 1;
        assert!(d.advance(None, now));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:1");
        assert_eq!(d.saved.on_air.as_ref().unwrap().airs, 2);
        assert_eq!(d.saved.retired.len(), 1);
        assert_eq!(d.saved.retired[0].id(), "yt:3");
        assert!(d.saved.aired.contains(&"yt:3".to_string()));
        // The pool is empty: the one on air stays.
        assert!(!d.advance(None, now));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:1");
        // Back in the pool: passes counted again (they were spent), and on air
        // at the next SUIVANT; a message's own rule wins over the pool's.
        assert!(d.restore("yt:3", now));
        assert!(!d.restore("yt:3", now));
        assert_eq!(d.saved.approved[0].airs, 0);
        d.card_mut("yt:3").unwrap().leave = Some(Leave {
            airs: 0,
            minutes: 0,
        });
        assert!(d.advance(None, now));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:3");
        assert_eq!(
            d.saved.retired.iter().map(|c| c.id()).collect::<Vec<_>>(),
            ["yt:1"]
        );
        // The pool is empty again (yt:1 spent its pass): yt:3 stays, its own
        // rule never ends it.
        assert!(!d.advance(None, now + 1));
        assert_eq!(d.saved.on_air.as_ref().unwrap().id(), "yt:3");
        // Minutes: a message too long in the pool leaves it, by its own clock.
        assert!(d.restore("yt:1", now - 120_000));
        d.saved.display.leave = Leave {
            airs: 0,
            minutes: 1,
        };
        assert!(d.expire(now));
        assert!(d.saved.approved.is_empty());
        // Out of the pool by hand, from the air: nothing on air.
        assert!(d.retire("yt:3", now));
        assert!(d.saved.on_air.is_none());
        assert_eq!(d.saved.retired.len(), 2);
        // Back with the incoming ones.
        assert!(d.restore("yt:1", now));
        assert!(d.unapprove("yt:1"));
        assert_eq!(d.incoming.back().unwrap().status, Status::Pending);
        // The pool's size: the oldest not starred leave.
        d.saved.display.leave = Leave::default();
        d.saved.display.max = 1;
        d.approve("yt:1", "Camille", now);
        d.saved.approved.last_mut().unwrap().starred = false;
        assert!(d.restore("yt:3", now));
        assert_eq!(d.saved.approved.len(), 1);
        assert_eq!(d.saved.approved[0].id(), "yt:3");
    }

    #[test]
    fn duplicates_and_authors_are_counted_per_project() {
        let mut d = Desk1::default();
        let now = now_ms();
        d.ingest(
            &message("yt", 1, "Sophie Lefebvre", "Bravo pour l'émission !"),
            now,
        );
        d.ingest(
            &message("yt", 2, "Paul Martin", "bravo pour l’EMISSION"),
            now,
        );
        d.ingest(
            &message("yt", 3, "Sophie Lefebvre", "Une question pour le ministre"),
            now,
        );
        assert_eq!(d.incoming[1].status, Status::Rejected);
        assert!(d.incoming[1]
            .tags
            .iter()
            .any(|t| t.kind == rules::TagKind::Duplicate));
        let msgs = d.incoming[2]
            .tags
            .iter()
            .find(|t| t.kind == rules::TagKind::Msgs);
        assert_eq!(msgs.map(|t| t.value.as_str()), Some("2"));
    }

    /// A server with its demo project and channel, nothing running.
    fn test_state(dir: &Path) -> Shared {
        use crate::state::{AppState, ChannelRt};
        use tokio::sync::RwLock;
        let config = crate::config::ServerConfig::load(dir);
        let channels = config
            .channels
            .iter()
            .map(|c| {
                let rt = ChannelRt {
                    config: c.clone(),
                    project_id: RwLock::new(None),
                    state: RwLock::new(Default::default()),
                    loaded: Default::default(),
                    published: Default::default(),
                    dirty: Default::default(),
                    ops: Default::default(),
                };
                (c.id.clone(), Arc::new(rt))
            })
            .collect();
        Arc::new(AppState {
            data_dir: dir.to_path_buf(),
            library: RwLock::new({
                let mut lib = crate::library::Library::load(dir).unwrap();
                lib.save(cg_core::sample::demo_project()).unwrap();
                lib
            }),
            channels,
            events: crate::state::Events::new(256),
            renderer: Mutex::new(cg_render::Renderer::new(dir.join("assets"))),
            thumbs: Default::default(),
            saving: Default::default(),
            auth: Arc::new(crate::auth::Auth::load(dir)),
            mos: Arc::new(crate::mos::Store::load(dir)),
            data: Arc::new(crate::datasources::Hub::load(dir)),
            social: Arc::new(crate::social::Hub::load(dir)),
            moderation: Arc::new(Desk::load(dir)),
            locks: Default::default(),
            edits: Default::default(),
            output_status: Default::default(),
            inputs: Default::default(),
            perf: Default::default(),
            config,
            bind: "127.0.0.1:0".parse().unwrap(),
            started_at: std::time::SystemTime::now(),
            restart: Default::default(),
            closing: tokio::sync::watch::channel(false).0,
            presence: Default::default(),
            schedule: crate::schedule::Schedule::load(dir),
        })
    }

    #[tokio::test]
    async fn from_a_connection_to_the_source_through_the_pool() {
        let dir =
            std::env::temp_dir().join(format!("social-desk-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = test_state(&dir);
        let pid = s.summaries().await[0].id.clone();
        let sid = source_id(&pid);
        // A shared connection, followed by the project.
        s.social
            .put(SocialSource {
                id: "yt".into(),
                name: "YouTube · JT 20H".into(),
                platform: Platform::Youtube,
                enabled: true,
                settings: Default::default(),
            })
            .unwrap();
        follow(&s, &pid, vec!["yt".into()]).await;
        let src = s.data.get(&sid).unwrap();
        assert_eq!(src.project.as_deref(), Some(pid.as_str()));
        assert_eq!(src.name, "Modération");
        let mut rx = s.social.subscribe();
        for (n, text) in [
            (1, "Bravo pour l’invité, très clair"),
            (2, "Question pour le ministre : et les retraités ?"),
        ] {
            s.social.publish(message("yt", n, "Sophie Lefebvre", text));
            s.moderation.incoming(&rx.recv().await.unwrap());
        }
        assert_eq!(s.moderation.view(&pid).incoming.len(), 2);
        // Approved: the first goes on air by itself, the source says so.
        let now = now_ms();
        for id in ["yt:1", "yt:2"] {
            assert!(s.moderation.edit(&pid, |d| d.approve(id, "Camille", now)));
        }
        rotate(&s, std::slice::from_ref(&pid), now).await;
        let v = s.data.value(&sid).unwrap();
        assert_eq!(v["onAir"]["id"], "yt:1");
        assert_eq!(v["onAir"]["text"], "Bravo pour l’invité, très clair");
        assert_eq!(v["next"]["id"], "yt:2");
        // The rotation: the next one when its time comes, not before.
        rotate(&s, std::slice::from_ref(&pid), now + 1000).await;
        assert_eq!(s.data.value(&sid).unwrap()["onAir"]["id"], "yt:1");
        rotate(&s, std::slice::from_ref(&pid), now + 9000).await;
        let view = s.moderation.view(&pid);
        assert_eq!(view.on_air.as_ref().unwrap().message.id, "yt:2");
        assert_eq!(view.approved[0].message.id, "yt:1");
        assert_eq!(view.approved[0].airs, 1);
        assert_eq!(s.data.value(&sid).unwrap()["onAir"]["id"], "yt:2");
        // An edit of the message on air reaches the source.
        s.moderation.edit(&pid, |d| {
            d.card_mut("yt:2").unwrap().edit = Some("Et les retraités ?".into())
        });
        publish(&s, &pid).await;
        assert_eq!(
            s.data.value(&sid).unwrap()["onAir"]["text"],
            "Et les retraités ?"
        );
        // Time in the pool is up: the message leaves it at the next turn.
        s.moderation
            .edit(&pid, |d| d.saved.display.leave.minutes = 1);
        rotate(&s, std::slice::from_ref(&pid), now + 61_000).await;
        let view = s.moderation.view(&pid);
        assert!(view.approved.is_empty());
        assert_eq!(view.retired.len(), 1);
        assert_eq!(view.on_air.as_ref().unwrap().message.id, "yt:2");
        assert!(s.data.value(&sid).unwrap()["approved"]
            .as_array()
            .unwrap()
            .is_empty());
        // Written, and read back after a restart.
        let back = Desk::load(&dir);
        assert_eq!(back.view(&pid).sources, vec!["yt".to_string()]);
        assert_eq!(back.view(&pid).retired[0].message.id, "yt:1");
        assert_eq!(back.view(&pid).display.leave.minutes, 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
