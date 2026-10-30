//! A project's moderation rules: a toxicity score computed here (a French
//! and English lexicon read through accents, look-alike digits and repeated
//! letters; no network), filters each with an action, trusted and blocked
//! accounts, and the tags shown on each message. Pure: `check` reads a
//! message with the rules and says what to do with it.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::Message;

/// What a filter does to a message it catches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Rejected without a moderator.
    Reject,
    /// Left to a moderator, flagged.
    Check,
    /// Corrected (sentence case, extra emojis off, cut to the length).
    Fix,
    /// What it caught replaced by "••••".
    Mask,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Toxicity {
    pub enabled: bool,
    /// Rejected from this score.
    pub high: f32,
    /// To check from this score.
    pub low: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Banned {
    pub enabled: bool,
    pub action: Action,
    /// The project's own words (and phrases).
    pub words: Vec<String>,
    /// With the built-in lists of insults, slurs and threats.
    pub builtin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Links {
    pub enabled: bool,
    pub action: Action,
    /// Domains allowed (and their subdomains).
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Caps {
    pub enabled: bool,
    pub action: Action,
    /// Caught above this share of capital letters.
    pub percent: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Length {
    pub enabled: bool,
    pub action: Action,
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Emojis {
    pub enabled: bool,
    pub action: Action,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Duplicates {
    pub enabled: bool,
    pub action: Action,
    /// The same text again within this many minutes.
    pub minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Personal {
    pub enabled: bool,
    pub action: Action,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Accounts {
    pub enabled: bool,
    /// Names, @handles or platform ids.
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rules {
    pub toxicity: Toxicity,
    pub banned: Banned,
    pub links: Links,
    pub caps: Caps,
    pub length: Length,
    pub emojis: Emojis,
    pub duplicates: Duplicates,
    pub personal: Personal,
    /// Approved without a moderator, unless toxic.
    pub trusted: Accounts,
    /// Rejected without a moderator.
    pub blocked: Accounts,
}

impl Default for Toxicity {
    fn default() -> Self {
        Self {
            enabled: true,
            high: 0.8,
            low: 0.4,
        }
    }
}
impl Default for Banned {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Reject,
            words: vec![],
            builtin: true,
        }
    }
}
impl Default for Links {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Check,
            allow: vec![],
        }
    }
}
impl Default for Caps {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Fix,
            percent: 70,
        }
    }
}
impl Default for Length {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Check,
            min: 10,
            max: 220,
        }
    }
}
impl Default for Emojis {
    fn default() -> Self {
        Self {
            enabled: false,
            action: Action::Fix,
            max: 5,
        }
    }
}
impl Default for Duplicates {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Reject,
            minutes: 10,
        }
    }
}
impl Default for Personal {
    fn default() -> Self {
        Self {
            enabled: true,
            action: Action::Mask,
        }
    }
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            toxicity: Toxicity::default(),
            banned: Banned::default(),
            links: Links::default(),
            caps: Caps::default(),
            length: Length::default(),
            emojis: Emojis::default(),
            duplicates: Duplicates::default(),
            personal: Personal::default(),
            trusted: Accounts {
                enabled: true,
                accounts: vec![],
            },
            blocked: Accounts {
                enabled: true,
                accounts: vec![],
            },
        }
    }
}

impl Rules {
    /// Thresholds in order and in 0..1, numbers in range.
    pub fn normalize(&mut self) {
        let t = &mut self.toxicity;
        t.high = t.high.clamp(0.0, 1.0);
        t.low = t.low.clamp(0.0, t.high);
        self.caps.percent = self.caps.percent.clamp(10, 100);
        self.length.max = self.length.max.clamp(1, 10_000);
        self.length.min = self.length.min.min(self.length.max);
        self.duplicates.minutes = self.duplicates.minutes.clamp(1, 24 * 60);
        for list in [
            &mut self.banned.words,
            &mut self.links.allow,
            &mut self.trusted.accounts,
            &mut self.blocked.accounts,
        ] {
            for w in list.iter_mut() {
                *w = w.trim().to_string();
            }
            list.retain(|w| !w.is_empty());
            list.dedup();
        }
        for d in self.links.allow.iter_mut() {
            *d = host(&d.to_lowercase()).to_string();
        }
    }
}

/// Where a message stands after the rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Pending,
    /// Flagged for a moderator.
    Check,
    Rejected,
    Approved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagKind {
    Question,
    Photo,
    Link,
    Tox,
    Banned,
    Personal,
    Caps,
    Length,
    Emojis,
    Duplicate,
    Fixed,
    Check,
    AutoReject,
    Blocked,
    Trusted,
    /// Messages of the same author this session (`value`).
    Msgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Ok,
    Info,
    Warn,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub kind: TagKind,
    pub level: Level,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

fn tag(kind: TagKind, level: Level) -> Tag {
    Tag {
        kind,
        level,
        value: String::new(),
    }
}

/// What the rules say of a message.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub status: Status,
    /// 0..1, two decimals.
    pub toxicity: f32,
    pub tags: Vec<Tag>,
    /// For moderators: banned words masked.
    pub shown: String,
    /// For the air: what the filters correct or mask, done.
    pub fixed: String,
    /// The banned words found, as written.
    pub words: Vec<String>,
}

/// About the message among the others.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The same text came in a short while ago.
    pub duplicate: bool,
    /// Messages of its author this session, this one included.
    pub author_messages: u32,
}

pub const MASK: &str = "••••";

pub fn check(rules: &Rules, msg: &Message, ctx: &Context) -> Verdict {
    let text = msg.text.as_str();
    let toks = tokens(text);
    let mut tags = Vec::new();
    let mut reject = false;
    let mut flag = false;
    // Replaced in the text on air: (byte range, by).
    let mut cuts: Vec<(usize, usize)> = Vec::new();

    if is_question(text, &toks) {
        tags.push(tag(TagKind::Question, Level::Ok));
    }
    if !msg.photos.is_empty() {
        tags.push(Tag {
            kind: TagKind::Photo,
            level: Level::Ok,
            value: msg.photos.len().to_string(),
        });
    }
    // Links.
    if rules.links.enabled {
        let found: Vec<_> = links(text)
            .into_iter()
            .filter(|(_, _, h)| !allowed(h, &rules.links.allow))
            .collect();
        if !found.is_empty() {
            act(rules.links.action, &mut reject, &mut flag);
            tags.push(Tag {
                kind: TagKind::Link,
                level: level_of(rules.links.action),
                value: found[0].2.clone(),
            });
            if matches!(rules.links.action, Action::Fix | Action::Mask) {
                cuts.extend(found.iter().map(|(a, b, _)| (*a, *b)));
            }
        }
    }
    // Toxicity.
    let score = toxicity(text, &toks);
    if rules.toxicity.enabled {
        let level = if score >= rules.toxicity.high {
            reject = true;
            Level::Bad
        } else if score >= rules.toxicity.low {
            flag = true;
            Level::Warn
        } else {
            Level::Ok
        };
        tags.push(Tag {
            kind: TagKind::Tox,
            level,
            value: format!("{score:.2}"),
        });
    }
    // Banned words: always masked for moderators.
    let mut words = Vec::new();
    let mut hidden: Vec<(usize, usize)> = Vec::new();
    if rules.banned.enabled {
        let own: Vec<Term> = rules
            .banned
            .words
            .iter()
            .map(|w| Term::new(w, 1.0))
            .collect();
        let builtin = rules
            .banned
            .builtin
            .then(|| lexicon().iter().filter(|t| t.weight >= BUILTIN))
            .into_iter()
            .flatten();
        let found = find(&toks, own.iter().chain(builtin));
        for m in &found {
            let (a, b) = (toks[m.from].start, toks[m.to - 1].end);
            if !hidden.contains(&(a, b)) {
                hidden.push((a, b));
                words.push(text[a..b].to_string());
            }
        }
        if !hidden.is_empty() {
            act(rules.banned.action, &mut reject, &mut flag);
            tags.push(Tag {
                kind: TagKind::Banned,
                level: level_of(rules.banned.action).max(Level::Warn),
                value: words.len().to_string(),
            });
            if matches!(rules.banned.action, Action::Fix | Action::Mask) {
                cuts.extend(hidden.iter().copied());
            }
        }
    }
    // Personal data.
    if rules.personal.enabled {
        let found = personal(text, &toks);
        if !found.is_empty() {
            act(rules.personal.action, &mut reject, &mut flag);
            tags.push(tag(TagKind::Personal, level_of(rules.personal.action)));
            if matches!(rules.personal.action, Action::Fix | Action::Mask) {
                cuts.extend(found);
            }
        }
    }
    let mut fixed = replace(text, &cuts, |_| MASK.to_string());
    let mut corrected = !cuts.is_empty();
    // Emojis.
    if rules.emojis.enabled && emoji_count(text) > rules.emojis.max as usize {
        act(rules.emojis.action, &mut reject, &mut flag);
        tags.push(tag(TagKind::Emojis, level_of(rules.emojis.action)));
        if matches!(rules.emojis.action, Action::Fix | Action::Mask) {
            fixed = keep_emojis(&fixed, rules.emojis.max as usize);
            corrected = true;
        }
    }
    // Capitals.
    if rules.caps.enabled && caps_ratio(text) * 100.0 > rules.caps.percent as f32 {
        act(rules.caps.action, &mut reject, &mut flag);
        tags.push(tag(TagKind::Caps, level_of(rules.caps.action)));
        if matches!(rules.caps.action, Action::Fix | Action::Mask) {
            fixed = sentence_case(&fixed);
            corrected = true;
        }
    }
    // Length.
    let n = text.trim().chars().count() as u32;
    if rules.length.enabled && (n < rules.length.min || n > rules.length.max) {
        act(rules.length.action, &mut reject, &mut flag);
        tags.push(tag(TagKind::Length, level_of(rules.length.action)));
        if n > rules.length.max && matches!(rules.length.action, Action::Fix | Action::Mask) {
            fixed = cut(&fixed, rules.length.max as usize);
            corrected = true;
        }
    }
    if rules.duplicates.enabled && ctx.duplicate {
        act(rules.duplicates.action, &mut reject, &mut flag);
        tags.push(tag(TagKind::Duplicate, level_of(rules.duplicates.action)));
    }
    if corrected {
        tags.push(tag(TagKind::Fixed, Level::Info));
    }
    // Accounts.
    let blocked = rules.blocked.enabled && is_account(msg, &rules.blocked.accounts);
    let trusted = rules.trusted.enabled && is_account(msg, &rules.trusted.accounts);
    if blocked {
        reject = true;
        tags.push(tag(TagKind::Blocked, Level::Bad));
    }
    let status = if reject {
        tags.push(tag(TagKind::AutoReject, Level::Bad));
        Status::Rejected
    } else if trusted && !flag {
        Status::Approved
    } else if flag {
        tags.push(tag(TagKind::Check, Level::Warn));
        Status::Check
    } else {
        Status::Pending
    };
    if trusted {
        tags.push(tag(TagKind::Trusted, Level::Ok));
    }
    if ctx.author_messages > 1 {
        tags.push(Tag {
            kind: TagKind::Msgs,
            level: Level::Info,
            value: ctx.author_messages.to_string(),
        });
    }
    let shown = replace(text, &hidden, |s| "•".repeat(s.chars().count().clamp(4, 8)));
    Verdict {
        status,
        toxicity: score,
        tags,
        shown,
        fixed: fixed.trim().to_string(),
        words,
    }
}

fn act(a: Action, reject: &mut bool, flag: &mut bool) {
    match a {
        Action::Reject => *reject = true,
        Action::Check => *flag = true,
        Action::Fix | Action::Mask => {}
    }
}

fn level_of(a: Action) -> Level {
    match a {
        Action::Reject => Level::Bad,
        Action::Check => Level::Warn,
        Action::Fix | Action::Mask => Level::Info,
    }
}

/// The message's author is one of `list` (name, @handle or id, any case).
pub fn is_account(msg: &Message, list: &[String]) -> bool {
    let a = &msg.author;
    let norm = |s: &str| plain(s.trim().trim_start_matches('@'));
    let mine = [norm(&a.name), norm(&a.handle), norm(&a.id)];
    list.iter()
        .map(|x| norm(x))
        .any(|x| !x.is_empty() && mine.contains(&x))
}

// ------------------------------------------------------------------ text ---

/// Lower case without accents: what searches, duplicates and hashtags compare.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match unaccent(c) {
            Some(s) => out.push_str(s),
            None => out.extend(c.to_lowercase()),
        }
    }
    out
}

fn unaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'ç' | 'Ç' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
        'ñ' | 'Ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => "y",
        'œ' | 'Œ' => "oe",
        'æ' | 'Æ' => "ae",
        'ß' => "ss",
        _ => return None,
    })
}

/// Digits and signs written for letters (c0nn4rd, s@lope, $alope).
fn leet(c: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' => 'i',
        '3' => 'e',
        '4' => 'a',
        '5' => 's',
        '7' => 't',
        '@' => 'a',
        '$' => 's',
        '€' => 'e',
        _ => return None,
    })
}

/// A word of a message: folded (lower case, no accents, look-alikes read),
/// and where it is in the text.
#[derive(Debug, Clone)]
struct Token {
    word: String,
    start: usize,
    end: usize,
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '@' | '$' | '*' | '€')
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut start = None;
    let flush = |from: usize, to: usize, out: &mut Vec<Token>| {
        let raw = &text[from..to];
        // "@handle": the sign is not a letter there.
        let (raw, from) = match raw.strip_prefix('@') {
            Some(r) if r.starts_with(|c: char| c.is_alphabetic()) => (r, from + 1),
            _ => (raw, from),
        };
        let letters = raw.chars().any(|c| c.is_alphabetic());
        let mut word = String::new();
        for c in raw.chars() {
            if letters {
                if let Some(l) = leet(c) {
                    word.push(l);
                    continue;
                }
            } else if !c.is_alphanumeric() {
                continue;
            }
            match unaccent(c) {
                Some(s) => word.push_str(s),
                None => word.extend(c.to_lowercase()),
            }
        }
        if !word.is_empty() {
            out.push(Token {
                word,
                start: from,
                end: to,
            });
        }
    };
    for (i, c) in text.char_indices() {
        if word_char(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            flush(s, i, &mut out);
        }
    }
    if let Some(s) = start {
        flush(s, text.len(), &mut out);
    }
    // "c o n n a r d", "f.u.c.k": letters one by one read as a word.
    let mut joined: Vec<Token> = Vec::with_capacity(out.len());
    let mut i = 0;
    while i < out.len() {
        let single = |t: &Token, prev: Option<&Token>| {
            t.word.chars().count() == 1
                && prev.is_none_or(|p| {
                    text[p.end..t.start]
                        .chars()
                        .all(|c| matches!(c, ' ' | '.' | '-' | '_'))
                })
        };
        let mut j = i;
        while j < out.len() && single(&out[j], (j > i).then(|| &out[j - 1])) {
            j += 1;
        }
        if j - i >= 3 {
            joined.push(Token {
                word: out[i..j].iter().map(|t| t.word.as_str()).collect(),
                start: out[i].start,
                end: out[j - 1].end,
            });
            i = j;
        } else {
            joined.push(out[i].clone());
            i += 1;
        }
    }
    joined
}

/// Repeated letters once: "connnnard", "connard" → "conard".
fn squeeze(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = None;
    for c in s.chars() {
        if last != Some(c) {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

// --------------------------------------------------------------- lexicon ---

/// Weights from this one are in the built-in banned words.
const BUILTIN: f32 = 0.8;
/// Words under this weight only count aimed at someone.
const WEAK: f32 = 0.4;

/// Insults, slurs and threats (French and English), by weight.
const LEXICON: &[(&str, f32)] = &[
    // French: strong.
    ("connard", 0.9),
    ("connasse", 0.9),
    ("conne", 0.8),
    ("connes", 0.8),
    ("salope", 0.92),
    ("salaud", 0.85),
    ("salopard", 0.88),
    ("pute", 0.9),
    ("putes", 0.9),
    ("putain", 0.5),
    ("encule", 0.92),
    ("enculer", 0.9),
    ("enfoire", 0.88),
    ("batard", 0.88),
    ("fils de pute", 0.97),
    ("fdp", 0.92),
    ("ntm", 0.92),
    ("nique ta mere", 0.97),
    ("nique", 0.82),
    ("niquer", 0.82),
    ("ta gueule", 0.75),
    ("ferme ta gueule", 0.88),
    ("tg", 0.6),
    ("va te faire foutre", 0.9),
    ("va te faire enculer", 0.95),
    ("va crever", 0.92),
    ("crevez tous", 0.92),
    ("te tuer", 0.96),
    ("vous tuer", 0.92),
    ("te buter", 0.95),
    ("te crever", 0.92),
    ("egorger", 0.95),
    ("mort aux", 0.9),
    ("pd", 0.88),
    ("pede", 0.95),
    ("tapette", 0.85),
    ("tafiole", 0.88),
    ("gouine", 0.9),
    ("negro", 0.95),
    ("negre", 0.92),
    ("bougnoule", 0.97),
    ("bicot", 0.95),
    ("youpin", 0.97),
    ("chinetoque", 0.95),
    ("bamboula", 0.92),
    ("raclure", 0.85),
    ("sous merde", 0.9),
    ("trou du cul", 0.85),
    ("trouduc", 0.85),
    ("petasse", 0.85),
    ("pouffiasse", 0.88),
    ("grognasse", 0.82),
    ("journalope", 0.85),
    ("journalopes", 0.85),
    ("merdia", 0.6),
    // French: milder.
    ("con", 0.6),
    ("ordure", 0.75),
    ("pourriture", 0.7),
    ("branleur", 0.7),
    ("bouffon", 0.55),
    ("tocard", 0.55),
    ("abruti", 0.55),
    ("debile", 0.5),
    ("cretin", 0.55),
    ("imbecile", 0.55),
    ("idiot", 0.5),
    ("stupide", 0.45),
    ("attarde", 0.6),
    ("mongol", 0.6),
    ("merde", 0.45),
    ("nul", 0.3),
    ("nuls", 0.3),
    ("guignol", 0.35),
    ("degage", 0.35),
    ("facho", 0.5),
    ("nazi", 0.55),
    // English: strong.
    ("fuck", 0.82),
    ("fucking", 0.75),
    ("fucker", 0.9),
    ("motherfucker", 0.95),
    ("bitch", 0.9),
    ("bastard", 0.85),
    ("asshole", 0.9),
    ("dickhead", 0.9),
    ("cunt", 0.96),
    ("whore", 0.95),
    ("slut", 0.92),
    ("retard", 0.9),
    ("faggot", 0.98),
    ("fag", 0.95),
    ("nigger", 1.0),
    ("nigga", 0.95),
    ("kike", 0.98),
    ("spic", 0.95),
    ("chink", 0.95),
    ("wanker", 0.85),
    ("twat", 0.85),
    ("piece of shit", 0.9),
    ("kill yourself", 0.98),
    ("kys", 0.95),
    ("kill you", 0.96),
    ("go die", 0.9),
    ("hope you die", 0.96),
    // English: milder.
    ("shit", 0.5),
    ("dick", 0.6),
    ("scum", 0.7),
    ("moron", 0.6),
    ("idiots", 0.5),
    ("stupid", 0.45),
    ("dumb", 0.45),
    ("loser", 0.45),
    ("shut up", 0.5),
    ("stfu", 0.65),
    ("gtfo", 0.6),
];

/// "sale(s) …" before these: a slur.
const GROUPS: &[&str] = &[
    "arabe",
    "arabes",
    "juif",
    "juive",
    "juifs",
    "noir",
    "noire",
    "noirs",
    "negre",
    "blanc",
    "blancs",
    "chinois",
    "musulman",
    "musulmans",
    "gitan",
    "gitans",
    "roumain",
    "roumains",
    "rebeu",
    "rebeus",
    "renoi",
    "feuj",
    "pede",
    "pedes",
];

/// Who a message speaks to: an insult near one of these is aimed.
const TARGETS: &[&str] = &[
    "tu", "t", "te", "toi", "ta", "ton", "tes", "vous", "votre", "vos", "you", "your", "ur", "u",
    "youre", "yourself", "espece",
];

#[derive(Debug, Clone)]
struct Term {
    /// Folded, repeated letters once.
    words: Vec<String>,
    /// Folded as written (for "f*ck").
    full: String,
    weight: f32,
}

impl Term {
    fn new(text: &str, weight: f32) -> Self {
        let words: Vec<String> = tokens(text).into_iter().map(|t| t.word).collect();
        Self {
            full: words.join(" "),
            words: words.iter().map(|w| squeeze(w)).collect(),
            weight,
        }
    }
}

fn lexicon() -> &'static [Term] {
    static TERMS: OnceLock<Vec<Term>> = OnceLock::new();
    TERMS.get_or_init(|| {
        let mut all: Vec<Term> = LEXICON.iter().map(|(w, n)| Term::new(w, *n)).collect();
        for g in GROUPS {
            all.push(Term::new(&format!("sale {g}"), 0.95));
            all.push(Term::new(&format!("sales {g}"), 0.95));
        }
        all
    })
}

/// How many words the built-in banned list has.
pub fn builtin_count() -> usize {
    lexicon().iter().filter(|t| t.weight >= BUILTIN).count()
}

/// A word of a message is `term` (a plural or feminine of a long one too).
fn same_word(token: &str, term: &str, full: &str) -> bool {
    if token.contains('*') {
        // "f*ck", "f**k": as long as the word, the stars any letter.
        let (a, b): (Vec<char>, Vec<char>) = (token.chars().collect(), full.chars().collect());
        return a.len() == b.len()
            && a[0] != '*'
            && a.iter().filter(|c| **c != '*').count() >= 2
            && a.iter().zip(&b).all(|(x, y)| *x == '*' || x == y);
    }
    let t = squeeze(token);
    t == term
        || (term.chars().count() >= 5
            && ["s", "e", "es", "x"]
                .iter()
                .any(|s| t.strip_suffix(s) == Some(term)))
}

#[derive(Debug, Clone, Copy)]
struct Found {
    from: usize,
    to: usize,
    weight: f32,
}

/// The terms found in the tokens, the longest first where they overlap.
fn find<'a>(toks: &[Token], terms: impl Iterator<Item = &'a Term>) -> Vec<Found> {
    let terms: Vec<&Term> = terms.collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let mut best: Option<Found> = None;
        for t in &terms {
            let n = t.words.len();
            if n == 0 || i + n > toks.len() {
                continue;
            }
            let full: Vec<&str> = t.full.split(' ').collect();
            let hit = (0..n).all(|k| {
                let w = &toks[i + k].word;
                if k + 1 < n {
                    squeeze(w) == t.words[k]
                } else {
                    same_word(w, &t.words[k], full.get(k).copied().unwrap_or(""))
                }
            });
            if hit
                && best.is_none_or(|b| {
                    n > b.to - b.from || (n == b.to - b.from && t.weight > b.weight)
                })
            {
                best = Some(Found {
                    from: i,
                    to: i + n,
                    weight: t.weight,
                });
            }
        }
        match best {
            Some(b) => {
                found.push(b);
                i = b.to;
            }
            None => i += 1,
        }
    }
    found
}

/// 0 (fine) to 1 (an insult, a slur or a threat): the words of the
/// lexicon, more when aimed at someone, shouted or with "!!!".
fn toxicity(text: &str, toks: &[Token]) -> f32 {
    let aimed_at = |f: &Found| {
        let lo = f.from.saturating_sub(3);
        let hi = (f.to + 3).min(toks.len());
        (lo..hi)
            .filter(|k| *k < f.from || *k >= f.to)
            .any(|k| TARGETS.contains(&toks[k].word.as_str()))
    };
    // Weak words ("nul") only count said to someone.
    let found: Vec<(Found, bool)> = find(toks, lexicon().iter())
        .into_iter()
        .map(|f| (f, aimed_at(&f)))
        .filter(|(f, aimed)| f.weight >= WEAK || *aimed)
        .collect();
    let shouted = caps_ratio(text) > 0.7;
    let bangs = text.contains("!!!");
    let s = if found.is_empty() {
        0.01 + if shouted { 0.04 } else { 0.0 } + if bangs { 0.03 } else { 0.0 }
    } else {
        let mut ws: Vec<f32> = found.iter().map(|(f, _)| f.weight).collect();
        ws.sort_by(|a, b| b.total_cmp(a));
        let mut s = 1.0 - ws.iter().take(4).fold(1.0, |p, w| p * (1.0 - w));
        if found.iter().any(|(_, aimed)| *aimed) {
            s += 0.12;
        }
        if shouted {
            s += 0.06;
        }
        if bangs {
            s += 0.04;
        }
        s
    };
    (s.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

// ---------------------------------------------------------------- shapes ---

const QUESTION_WORDS: &[&str] = &[
    "pourquoi", "comment", "quand", "ou", "qui", "quel", "quelle", "quels", "quelles", "combien",
    "est", "why", "how", "when", "where", "who", "what", "which", "is", "are", "do", "does", "can",
    "could", "should", "will", "would",
];

fn is_question(text: &str, toks: &[Token]) -> bool {
    // Hashtags, mentions and emojis after the question mark.
    let mut end = text;
    loop {
        let t = end.trim_end_matches(|c: char| {
            c.is_whitespace() || is_emoji(c) || matches!(c, '\u{FE0F}' | '\u{200D}')
        });
        match t.rsplit_once(char::is_whitespace) {
            Some((head, last)) if last.starts_with(['#', '@']) => end = head,
            None if t.starts_with(['#', '@']) => return false,
            _ => {
                end = t;
                break;
            }
        }
    }
    if end.ends_with('?') {
        return true;
    }
    let first = toks.first().map(|t| t.word.as_str()).unwrap_or("");
    // "est-ce que", not "est" alone.
    if first == "est" {
        return toks.get(1).is_some_and(|t| t.word == "ce");
    }
    QUESTION_WORDS.contains(&first) && toks.len() > 2
}

/// Share of capital letters (0 below 8 letters: "OK", "BRAVO" are fine).
pub fn caps_ratio(text: &str) -> f32 {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 8 {
        return 0.0;
    }
    letters.iter().filter(|c| c.is_uppercase()).count() as f32 / letters.len() as f32
}

/// "BRAVO À TOUS. MERCI" → "Bravo à tous. Merci".
pub fn sentence_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '\n') {
                start = true;
            }
        }
    }
    out
}

pub fn is_emoji(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1F3FA | 0x1F400..=0x1FAFF | 0x1F000..=0x1F2FF | 0x2600..=0x27BF
        | 0x2B50 | 0x2B55 | 0x231A..=0x231B | 0x23E9..=0x23FA)
}

/// Skin tones, variation selectors and joiners belong to the emoji before.
fn emoji_part(c: char) -> bool {
    matches!(c as u32, 0x1F3FB..=0x1F3FF | 0xFE0F | 0x200D)
}

pub fn emoji_count(text: &str) -> usize {
    let mut n = 0;
    let mut prev_flag = false;
    for c in text.chars() {
        let flag = (0x1F1E6..=0x1F1FF).contains(&(c as u32));
        // A flag is two regional letters.
        if is_emoji(c) && !(flag && prev_flag) {
            n += 1;
        }
        prev_flag = flag && !prev_flag;
    }
    n
}

/// The first `max` emojis kept, the others off.
fn keep_emojis(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    let mut dropping = false;
    for c in text.chars() {
        if is_emoji(c) {
            n += 1;
            dropping = n > max;
            if dropping {
                continue;
            }
        } else if emoji_part(c) {
            if dropping {
                continue;
            }
        } else {
            dropping = false;
        }
        out.push(c);
    }
    // No double spaces where emojis were.
    let mut clean = String::with_capacity(out.len());
    for c in out.chars() {
        if c == ' ' && clean.ends_with(' ') {
            continue;
        }
        clean.push(c);
    }
    clean.trim().to_string()
}

/// At most `max` characters, cut between words, with "…".
fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    let head = match head.rfind(' ') {
        Some(i) if i > head.len() / 2 => &head[..i],
        _ => head.as_str(),
    };
    format!("{}…", head.trim_end_matches([' ', ',', ';', ':', '.']))
}

/// `text` with the byte ranges replaced (overlapping ones merged).
fn replace(text: &str, ranges: &[(usize, usize)], by: impl Fn(&str) -> String) -> String {
    let mut rs = ranges.to_vec();
    rs.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a, b) in rs {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (a, b) in merged {
        out.push_str(&text[at..a]);
        out.push_str(&by(&text[a..b]));
        at = b;
    }
    out.push_str(&text[at..]);
    out
}

// ----------------------------------------------------------------- links ---

const TLDS: &[&str] = &[
    "com", "fr", "net", "org", "xyz", "io", "co", "be", "ch", "info", "ly", "me", "tv", "app",
    "link", "site", "online", "shop", "top", "ru", "de", "uk", "eu", "gl", "gg", "biz", "club",
    "live", "news", "ca", "es", "it", "nl", "pt", "us", "click", "store", "vip", "win", "bet",
];

fn host(url: &str) -> &str {
    let u = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let u = u.split(['/', '?', '#']).next().unwrap_or("");
    u.trim_start_matches("www.")
}

/// Links in the text: byte range and host.
fn links(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    for chunk in text.split_inclusive(char::is_whitespace) {
        let start = at;
        at += chunk.len();
        let trimmed = chunk.trim_end();
        let lead = trimmed.len() - trimmed.trim_start_matches(['(', '"', '\'', '«', '[']).len();
        let word = trimmed[lead..]
            .trim_end_matches(['.', ',', '!', '?', ';', ':', ')', '"', '\'', '»', ']']);
        if word.is_empty() {
            continue;
        }
        let lower = word.to_lowercase();
        let h = host(&lower);
        let explicit = lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("www.");
        let labels: Vec<&str> = h.split('.').collect();
        let domain = labels.len() >= 2
            && labels
                .iter()
                .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            && TLDS.contains(labels.last().unwrap_or(&""));
        if (explicit && !h.is_empty()) || domain {
            out.push((start + lead, start + lead + word.len(), h.to_string()));
        }
    }
    out
}

fn allowed(host: &str, allow: &[String]) -> bool {
    allow
        .iter()
        .any(|d| host == d || host.ends_with(&format!(".{d}")))
}

// --------------------------------------------------------- personal data ---

const STREETS: &[&str] = &[
    "rue",
    "avenue",
    "av",
    "bd",
    "boulevard",
    "chemin",
    "allee",
    "impasse",
    "place",
    "route",
    "quai",
    "cours",
    "square",
    "passage",
    "street",
    "st",
    "road",
    "rd",
    "lane",
    "ave",
    "drive",
];

/// Phone numbers, e-mail addresses and street addresses: byte ranges.
fn personal(text: &str, toks: &[Token]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    // E-mail addresses.
    for (i, _) in text.match_indices('@') {
        let local =
            |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-');
        let dom = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-');
        let mut a = i;
        while a > 0 && local(b[a - 1]) {
            a -= 1;
        }
        let mut z = i + 1;
        while z < b.len() && dom(b[z]) {
            z += 1;
        }
        while z > i + 1 && b[z - 1] == b'.' {
            z -= 1;
        }
        if a < i && text[i + 1..z].contains('.') {
            out.push((a, z));
        }
    }
    // Phone numbers: + or 0, then 9 to 15 digits with single separators.
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let starts = (c == b'+' || c == b'0') && (i == 0 || !b[i - 1].is_ascii_alphanumeric());
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i + usize::from(c == b'+');
        let mut digits = 0;
        let mut last_digit = j;
        while j < b.len() {
            if b[j].is_ascii_digit() {
                digits += 1;
                j += 1;
                last_digit = j;
            } else if matches!(b[j], b' ' | b'.' | b'-' | b'/' | b'(' | b')')
                && j + 1 < b.len()
                && (b[j + 1].is_ascii_digit() || b[j + 1] == b'(')
            {
                j += 1;
            } else {
                break;
            }
        }
        let alone = last_digit >= b.len() || !b[last_digit].is_ascii_alphanumeric();
        if (9..=15).contains(&digits) && alone {
            out.push((i, last_digit));
            i = last_digit;
        } else {
            i += 1;
        }
    }
    // "12 rue des Lilas", "4 bis avenue Foch": up to the punctuation.
    for (k, t) in toks.iter().enumerate() {
        let raw = &text[t.start..t.end];
        if !(1..=4).contains(&raw.len()) || !raw.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let mut s = k + 1;
        if toks
            .get(s)
            .is_some_and(|n| matches!(n.word.as_str(), "bis" | "ter"))
        {
            s += 1;
        }
        if !toks
            .get(s)
            .is_some_and(|n| STREETS.contains(&n.word.as_str()))
        {
            continue;
        }
        let mut end = s;
        while end + 1 < toks.len()
            && end - s < 4
            && !text[toks[end].end..toks[end + 1].start].contains([',', '.', ';', '!', '?', '\n'])
        {
            end += 1;
        }
        out.push((t.start, toks[end].end));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::{Author, Platform};

    fn msg(text: &str) -> Message {
        Message {
            id: "t:1".into(),
            source_id: "t".into(),
            platform: Platform::Youtube,
            author: Author {
                id: "u1".into(),
                name: "Sophie Lefebvre".into(),
                handle: "sophie_l".into(),
                ..Default::default()
            },
            text: text.into(),
            photos: vec![],
            at: 0,
            amount: String::new(),
            url: String::new(),
        }
    }

    fn tox(text: &str) -> f32 {
        toxicity(text, &tokens(text))
    }

    fn kinds(v: &Verdict) -> Vec<TagKind> {
        v.tags.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn clear_insults_score_high_and_neutral_text_low() {
        for t in [
            "Espèce de connard",
            "Bande de salopes",
            "Ferme ta gueule",
            "Tu es un enculé",
            "Nique ta mère",
            "Fils de pute",
            "Je vais te tuer",
            "Sale arabe",
            "fuck you",
            "You stupid bitch",
            "Go kill yourself",
            "What an asshole",
        ] {
            assert!(tox(t) >= 0.8, "{t}: {}", tox(t));
        }
        for t in [
            "Bravo pour l’invité, enfin quelqu’un qui explique clairement.",
            "Pourquoi la réforme ne concerne pas les indépendants ?",
            "Photo des inondations à Quimperlé ce soir 🌊",
            "Je suis déconnecté depuis ce matin, le concours est annulé",
            "Le conseil municipal se réunit, contenu intéressant",
            "Great show tonight, thanks for the explanation!",
            "Match nul ce soir à Brest",
            "Je suis crevé après cette journée",
            "Passionnant, les pros and cons sont bien expliqués",
        ] {
            assert!(tox(t) <= 0.1, "{t}: {}", tox(t));
        }
    }

    #[test]
    fn look_alikes_accents_and_repeats_are_read() {
        for t in [
            "c0nn4rd",
            "CONNNNNARD",
            "s@lope",
            "$alope",
            "encule",
            "ENCULÉ",
            "f*ck you",
            "c o n n a r d",
            "fuuuuuck you",
        ] {
            assert!(tox(t) >= 0.75, "{t}: {}", tox(t));
        }
        // Words that only contain an insult are not one.
        assert_eq!(
            find(&tokens("déconnecté, connardise?"), lexicon().iter()).len(),
            0
        );
        assert_eq!(find(&tokens("dispute réputée"), lexicon().iter()).len(), 0);
        assert_eq!(find(&tokens("des connards"), lexicon().iter()).len(), 1);
    }

    #[test]
    fn aimed_shouted_insults_score_more() {
        assert!(tox("t'es un idiot") > tox("idiot"));
        assert!(tox("IDIOT IDIOT IDIOT !!!") > tox("idiot idiot idiot"));
        let mild = tox("quel idiot");
        assert!((0.4..0.8).contains(&mild), "{mild}");
    }

    #[test]
    fn banned_words_are_masked_for_moderators_and_rejected() {
        let rules = Rules::default();
        let v = check(
            &rules,
            &msg("Encore des connards sur ce plateau, nique les journalistes"),
            &Context::default(),
        );
        assert_eq!(v.status, Status::Rejected);
        assert!(v.shown.contains("••••••"), "{}", v.shown);
        assert!(!v.shown.contains("connards"));
        assert_eq!(v.words, vec!["connards", "nique"]);
        let k = kinds(&v);
        assert!(
            k.contains(&TagKind::Banned)
                && k.contains(&TagKind::AutoReject)
                && k.contains(&TagKind::Tox)
        );
        // A word of the project, masked on air.
        let mut rules = Rules::default();
        rules.toxicity.enabled = false;
        rules.banned.words = vec!["Dupont".into()];
        rules.banned.action = Action::Mask;
        let v = check(
            &rules,
            &msg("Bonjour à Mme DUPONT de Lyon"),
            &Context::default(),
        );
        assert_eq!(v.status, Status::Pending);
        assert_eq!(v.fixed, "Bonjour à Mme •••• de Lyon");
    }

    #[test]
    fn filters_do_their_action() {
        let rules = Rules::default();
        let ctx = Context::default();
        // Links: checked, unless allowed.
        let v = check(
            &rules,
            &msg("Super le reportage sur Brest !! va voir www.promo-crypto.xyz"),
            &ctx,
        );
        assert_eq!(v.status, Status::Check);
        assert_eq!(
            v.tags
                .iter()
                .find(|t| t.kind == TagKind::Link)
                .unwrap()
                .value,
            "promo-crypto.xyz"
        );
        let mut allow = Rules::default();
        allow.links.allow = vec!["lefigaro.fr".into()];
        let v = check(
            &allow,
            &msg("À lire sur https://www.lefigaro.fr/politique, très clair"),
            &ctx,
        );
        assert!(!kinds(&v).contains(&TagKind::Link));
        assert_eq!(v.status, Status::Pending);
        // Capitals: sentence case.
        let v = check(&rules, &msg("BRAVO À TOUTE L'ÉQUIPE. MERCI"), &ctx);
        assert_eq!(v.fixed, "Bravo à toute l'équipe. Merci");
        assert!(kinds(&v).contains(&TagKind::Caps) && kinds(&v).contains(&TagKind::Fixed));
        // Length: too short to check.
        assert_eq!(check(&rules, &msg("ok"), &ctx).status, Status::Check);
        let mut long = Rules::default();
        long.length.max = 20;
        long.length.action = Action::Fix;
        let v = check(&long, &msg("Un message bien trop long pour la carte"), &ctx);
        assert!(
            v.fixed.chars().count() <= 20 && v.fixed.ends_with('…'),
            "{}",
            v.fixed
        );
        // Emojis: the extra ones off.
        let mut emo = Rules::default();
        emo.emojis.enabled = true;
        emo.emojis.max = 2;
        let v = check(&emo, &msg("Bravo 👏👏🏽👏 🎉🎉"), &ctx);
        assert_eq!(v.fixed, "Bravo 👏👏🏽");
        // Duplicates: rejected.
        let v = check(
            &rules,
            &msg("Bravo pour l'émission"),
            &Context {
                duplicate: true,
                author_messages: 3,
            },
        );
        assert_eq!(v.status, Status::Rejected);
        assert!(v
            .tags
            .iter()
            .any(|t| t.kind == TagKind::Msgs && t.value == "3"));
        // Questions and photos.
        for q in [
            "Et les retraités agricoles ? #Retraites @franceinfo",
            "C’est vrai ?? 🤔",
        ] {
            assert!(
                kinds(&check(&rules, &msg(q), &ctx)).contains(&TagKind::Question),
                "{q}"
            );
        }
        let mut m = msg("Pourquoi la réforme ne concerne pas les indépendants");
        m.photos = vec!["a.jpg".into()];
        let k = kinds(&check(&rules, &m, &ctx));
        assert!(k.contains(&TagKind::Question) && k.contains(&TagKind::Photo));
    }

    #[test]
    fn personal_data_is_masked() {
        let rules = Rules::default();
        let ctx = Context::default();
        for (text, want) in [
            (
                "Appelez-moi au 06 12 34 56 78 svp",
                "Appelez-moi au •••• svp",
            ),
            ("Mon numéro +33 6 12 34 56 78.", "Mon numéro ••••."),
            (
                "écrivez à jean.dupont@gmail.com merci",
                "écrivez à •••• merci",
            ),
            ("J'habite 12 rue des Lilas, à Lyon", "J'habite ••••, à Lyon"),
            ("Venez au 4 bis avenue Foch ce soir", "Venez au ••••"),
        ] {
            let v = check(&rules, &msg(text), &ctx);
            assert_eq!(v.fixed, want, "{text}");
            assert!(kinds(&v).contains(&TagKind::Personal));
        }
        // Years, scores and prices are not phone numbers.
        let v = check(
            &rules,
            &msg("En 2024 le score était 3-1, pour 12 000 000 €"),
            &ctx,
        );
        assert!(!kinds(&v).contains(&TagKind::Personal), "{:?}", v.fixed);
    }

    #[test]
    fn trusted_accounts_are_approved_and_blocked_rejected() {
        let mut rules = Rules::default();
        rules.trusted.accounts = vec!["@sophie_l".into()];
        let v = check(
            &rules,
            &msg("Bravo pour l’invité, très clair"),
            &Context::default(),
        );
        assert_eq!(v.status, Status::Approved);
        assert!(kinds(&v).contains(&TagKind::Trusted));
        // Unless toxic.
        let v = check(&rules, &msg("quel idiot ce ministre"), &Context::default());
        assert_eq!(v.status, Status::Check);
        rules.blocked.accounts = vec!["Sophie Lefebvre".into()];
        let v = check(&rules, &msg("Bravo pour l’invité"), &Context::default());
        assert_eq!(v.status, Status::Rejected);
        assert!(kinds(&v).contains(&TagKind::Blocked));
    }
}
