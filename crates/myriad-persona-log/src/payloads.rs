//! What the payload of a row says, for the kinds every host core writes of
//! her talk, her memories and her mood: one shape, so a row one side wrote
//! reads the same on every other.
//!
//! A row never holds content ([`crate::Row`]); its payload is kept beside
//! it, sealed, and read back by its reference. These are the payloads'
//! shapes, as canonical JSON. A side that writes one of these kinds writes
//! this shape; a side that reads one takes fields it does not know and
//! ignores them, so a newer side may add a field without breaking an older
//! one. Times are milliseconds since the Unix epoch.
//!
//! The node kinds (`node.*`) are not here: they come with the nodes' own
//! rules.
//!
//! | kind | object | payload |
//! |---|---|---|
//! | `segment.open` | `segment:<id>` | [`SegmentOpen`] |
//! | `segment.line` | `segment:<id>` | [`SegmentLine`] |
//! | `segment.close` | `segment:<id>` | [`SegmentClose`] |
//! | `card.version` | `card:<id>` | [`CardVersion`] |
//! | `card.answered` | `card:<id>` | [`CardAnswered`] |
//! | `memory.write` | `memory:<id>` | [`MemoryWrite`] |
//! | `memory.retire` | `memory:<id>` | [`MemoryRetire`] |
//! | `memory.restore` | `memory:<id>` | [`MemoryRestore`] |
//! | `affect.change` | `affect:<subject>` | [`AffectChange`] |
//! | `speech.said` | `speech:<id>` | [`SpeechSaid`] |
//! | `persona.field` | `persona:<field>` | [`PersonaField`] |
//! | `state.write` | `state:<entity kind>:<id>` | [`StateWrite`] |
//! | `case.*` | `case:<id>` | [`CaseDisagreed`] and the others below |
//!
//! Who said a line, or wrote a memory, is the row's `author` (the owner or
//! her); who a memory or a mood is about is its `subject`, and who she said
//! a `speech.said` line to is its `audience` and `subject`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::LogError;

/// A stretch of talk begins: where it happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentOpen {
    pub at: i64,
    /// Where the talk is: `web` (the site's page), `app` (one of the
    /// owner's devices), `group` (a group chat), or another surface a side
    /// names.
    pub place: String,
}

/// One line said in a segment, by the row's author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentLine {
    pub at: i64,
    #[serde(default)]
    pub text: String,
    /// The writing side's own id for the line (a message id), when it has
    /// one: the same line is not shown twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The line it answers, by that `line`, when it answers one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// How many pictures came with it. The pictures themselves stay on the
    /// side that took them; another shows that there were some. A line of
    /// pictures only has no `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pictures: Option<u32>,
    /// The card that comes with it (`card.version`, by its id): her line and
    /// the card are one message. A line of a card only has no `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
}

/// One card as it stands now (`card.version`): a thing put before the
/// owner in her talk with them, as facts the program can check, her view of
/// it, and what they may answer. An open shape: a side that knows no
/// `kind` shows its `title`, `facts` and `actions` all the same
/// (`myriad_nodes::cards` holds the rules: words, bounds, ids). Unlike the
/// other shapes here, no part of a card takes a field it does not name:
/// every string in it is one the wire checks by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardVersion {
    pub at: i64,
    pub id: String,
    pub kind: String,
    pub scene: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<CardFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<CardView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CardAction>,
}

/// One fact on a card, and what it is about when that is a node or a
/// session (`ref`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardFact {
    pub text: String,
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// Her view on a card, on the facts it rests on (`because`, numbered from 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardView {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub because: Vec<u32>,
}

/// One thing the owner may answer a card with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardAction {
    pub id: String,
    pub label: String,
}

/// The owner's answer to a card (`card.answered`): which action, on which
/// version of it (that version's row id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardAnswered {
    pub at: i64,
    pub action: String,
    pub version: String,
}

/// A stretch of talk is over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentClose {
    pub at: i64,
}

/// A memory written: what it says and what kind of thing it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWrite {
    pub at: i64,
    pub text: String,
    /// `fact`, `preference`, `lesson`, `pattern`, `narrative`,
    /// `knowledge`, `done`, or another a side names.
    pub kind: String,
    /// The memories (`memory:<id>`) this one takes the place of; each also
    /// gets its own `memory.retire`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
    /// The words it is found by, as the writing side made them when it was
    /// written: another side does not make them again (they come from a
    /// model, and would come out otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cues: Vec<String>,
    /// Who it came from: she said it or came to it herself (`her`), or the
    /// partner told her (`partner`). What a memory is worth as a fact
    /// about the partner turns on it, so every side keeps it as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<MemorySpeaker>,
    /// Where the writing side learned it, in that side's own word for it
    /// (a site's `chat` or `bit`, a Mac's own…): no list of them here, so
    /// no side has to speak another's. A side that knows the word keeps
    /// it; one that does not keeps the memory as one another side wrote.
    /// Lowercase letters, digits and `_`, at most [`MAX_MEMORY_SOURCE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// When it would come to mind: a few moments it was kept for (what
    /// someone might say or ask that it answers), as the writing side's
    /// extraction gave them (`chat_remember`'s `whenFor`). Another side
    /// keeps them and does not ask a model again; a memory without them
    /// comes to mind by its words alone. At most [`MAX_MEMORY_WHEN`], each
    /// at most [`MEMORY_WHEN_CHARS`] chars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<String>,
}

/// Who a memory came from ([`MemoryWrite::speaker`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySpeaker {
    Her,
    Partner,
}

/// The longest word a side names where it learned a memory with.
pub const MAX_MEMORY_SOURCE: usize = 32;
/// How many moments a memory is kept for, and how long each is: the same
/// bounds as the extraction that writes them (`chat_remember`'s `whenFor`).
pub const MAX_MEMORY_WHEN: usize = 3;
pub const MEMORY_WHEN_CHARS: usize = 60;

/// A memory retired: why, and the one that holds it now when another does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetire {
    pub at: i64,
    /// `superseded`, `deleted` (the owner took it back), `faded`, or
    /// another a side names.
    pub why: String,
    /// The memory (`memory:<id>`) that holds what it held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// A retired memory back in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRestore {
    pub at: i64,
}

/// Her mood toward the row's subject changed: where it is now, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectChange {
    pub at: i64,
    /// How she feels, 0 to 100.
    pub valence: f64,
    /// How stirred she is, 0 to 100.
    pub arousal: f64,
    /// The band `valence` falls in, by the mood rules.
    pub band: String,
    /// What moved it: a category, never what was said.
    pub cause: String,
}

/// One line she said, wherever and however it came to be said, and how far
/// it got: a row of the ledger of what she said (`myriad_merope::speech`).
/// Who it was said to is the row's audience (and subject, in private).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSaid {
    pub at: i64,
    /// Her words as they went out.
    pub text: String,
    /// Where it was said, as `segment.open` names it: `web`, `app`, `group`
    /// or another surface.
    pub place: String,
    /// Why she said it: one of [`SPEECH_SOURCES`], the names
    /// `myriad_merope::speech::Source` goes by.
    pub source: String,
    /// Whether she was called on (a reply, an @, a touch, her turn in a
    /// game), rather than speaking unasked.
    pub called: bool,
    /// The thing it was about, by its key: the same key is the same thing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thing: Option<String>,
    /// The way out it took on the side that said it: `face`, `voice`,
    /// `notice`, `im`, ... a side names its own.
    pub exit: String,
    /// How much of it went out: `full`, `gist`, `marker` (only her mark
    /// lit), `notice` (only a notification), or `none` (not sent).
    pub extent: String,
    /// How many of its characters were heard, when a voice said it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard: Option<u32>,
    /// Whether it was cut off partway.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    /// The writing side's own id for the line (a message id), when it has
    /// one, as in `segment.line`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The group's name as it was when she said it there: what she is told
    /// of it when the same thing comes up elsewhere. Only on a group's row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
}

/// How much of a said line went out, the words [`SpeechSaid::extent`] takes.
pub const SPEECH_EXTENTS: [&str; 5] = ["full", "gist", "marker", "notice", "none"];

/// Why she said a line, the words [`SpeechSaid::source`] takes: the names
/// of `myriad_merope::speech::Source`, sorted (`myriad-core-ffi`'s tests
/// hold the two to each other). A row is append-only, so a word not here
/// is refused when written, not found out when read.
pub const SPEECH_SOURCES: [&str; 18] = [
    "event",
    "first_line",
    "game_remark",
    "game_turn",
    "greeting",
    "group_at",
    "group_first",
    "group_interject",
    "group_share",
    "live_lull",
    "other_ai",
    "reach",
    "reply",
    "screen",
    "share",
    "touch",
    "wander",
    "work_done",
];

/// One field of her persona, as it is now (`persona:<field>`, the field
/// named here too): `value` is the field's value as `myriad-persona-sync`
/// writes it, `null` when it was cleared. The earlier rows are its history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaField {
    pub at: i64,
    pub field: String,
    pub value: Value,
}

/// One of her records as it is now, as `myriad-persona-sync` writes the
/// entity (`kind`, `id`, `state`, `body`; its revision and origin are the
/// row's own clock and side): her outfits, diary, what she proposed, her
/// pace, what she is doing, her vitals, her serials, the game of soup, her
/// stickers, her state toward the owner, the mood it settles from and each
/// change of that mood (whole, to replay from: `affect.change` says where it
/// went, to read). The earlier rows are its history. A record that went is
/// written `deleted` (body `null`, only the kinds that can go) and is live
/// again under its id when it comes back: never a `forget`, which takes every
/// row of its object, those after it too. Only the kinds of [`STATE_KINDS`]:
/// the others have rows of their own (memories, what she said, talk), are the
/// row's history already (a field's past values) or stay on each side (which
/// set it wears).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateWrite {
    pub at: i64,
    /// The entity format the body is written in
    /// ([`myriad_persona_sync::FORMAT_VERSION`] where it was written).
    pub format: u32,
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub state: myriad_persona_sync::State,
    /// In its one form ([`myriad_persona_sync::entity::check_body`]).
    pub body: Value,
}

/// The entity kinds a `state.write` row carries, sorted.
pub const STATE_KINDS: [&str; 16] = [
    "affect_anchor",
    "affect_event",
    "diary",
    "intention",
    "outfit",
    "owner_state",
    "pace",
    "present",
    "proactive_line",
    "serial_following",
    "serial_past",
    "serial_shelf",
    "serial_work",
    "soup_game",
    "sticker",
    "vitals_day",
];

/// Whom a `state.write` row of an entity kind is for: fixed by the kind,
/// never by the side that writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateAudience {
    /// Her own (`own`): what she is, wears, does and keeps for herself.
    Own,
    /// Between her and the partner alone (`personal:<partner>`, one member).
    Partner,
    /// Either: a diary of her own day, or of a conversation with them.
    Either,
}

/// Each kind of [`STATE_KINDS`] and whom its rows are for.
pub const STATE_AUDIENCES: [(&str, StateAudience); 16] = [
    ("affect_anchor", StateAudience::Partner),
    // A change of her mood toward the owner (the only one that crosses,
    // `AffectEventBody.subject`), as `affect.change` toward them is.
    ("affect_event", StateAudience::Partner),
    ("diary", StateAudience::Either),
    ("intention", StateAudience::Partner),
    ("outfit", StateAudience::Own),
    ("owner_state", StateAudience::Partner),
    ("pace", StateAudience::Own),
    ("present", StateAudience::Own),
    ("proactive_line", StateAudience::Partner),
    ("serial_following", StateAudience::Own),
    ("serial_past", StateAudience::Own),
    ("serial_shelf", StateAudience::Own),
    ("serial_work", StateAudience::Own),
    ("soup_game", StateAudience::Partner),
    ("sticker", StateAudience::Own),
    ("vitals_day", StateAudience::Own),
];

/// Whom a `state.write` row of entity `kind` is for; none for a kind it
/// does not carry.
pub fn state_audience(kind: &str) -> Option<StateAudience> {
    STATE_AUDIENCES
        .iter()
        .find(|(named, _)| *named == kind)
        .map(|(_, audience)| *audience)
}

/// The object a `state.write` row of entity `kind` and `id` is about.
pub fn state_object(kind: &str, id: &str) -> String {
    format!("state:{kind}:{id}")
}

/// She told the owner she disagrees with what they want to do or decided
/// (a judged case begins, `case:<id>`). Ids are the writing side's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDisagreed {
    pub at: i64,
    /// What they want, and what she thinks, in her words.
    pub about: String,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    /// Her reply that said it, and their message it answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theirs: Option<String>,
    /// The node the talk was tagged with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Opened before work they asked her to do: the kind of asking, her
    /// words then, and the act and its doer when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub act: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doer: Option<String>,
}

/// A case line with only its time (`case.insisted`: they had her go
/// ahead anyway).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseAt {
    pub at: i64,
}

/// A case line that names one message (`case.persuaded`: a reason changed
/// her mind; `case.owned`: she told them herself she was wrong).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMessage {
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A separate check of their pushback, without what she remembers of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReconsidered {
    pub at: i64,
    pub new_reason: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Their words that carry the reason, a short quote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The work under way when she said it: the files, paths only. A path
/// never leaves the side it is on, so these rows travel without a body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseWork {
    pub at: i64,
    pub files: Vec<String>,
}

/// A test run over that work: `passed` or `failed`, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseTested {
    pub at: i64,
    pub result: String,
    pub source: String,
}

/// How it turned out: `worked` or `failed`, on what basis (`rollback`,
/// `tests`, `they_said`), and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseOutcome {
    pub at: i64,
    pub result: String,
    pub basis: String,
    pub source: String,
}

/// What the outcome says of her view: right or not, on the outcome's basis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseVerdict {
    pub at: i64,
    pub right: bool,
    pub basis: String,
}

/// The bases a case's outcome and verdict take.
pub const CASE_BASES: [&str; 3] = ["rollback", "tests", "they_said"];

fn read<T: for<'de> Deserialize<'de>>(kind: &str, value: &Value) -> Result<T, LogError> {
    serde_json::from_value(value.clone())
        .map_err(|_| LogError::new(format!("a {kind} payload is not of its shape")))
}

fn check_text(kind: &str, text: &str) -> Result<(), LogError> {
    if text.trim().is_empty() {
        return Err(LogError::new(format!("a {kind} payload has no text")));
    }
    Ok(())
}

fn check_memory(kind: &str, object: &str) -> Result<(), LogError> {
    if crate::row::object_type(object) != Some("memory") {
        return Err(LogError::new(format!(
            "a {kind} payload names something that is not a memory"
        )));
    }
    Ok(())
}

fn check_scale(value: f64) -> Result<(), LogError> {
    if !(0.0..=100.0).contains(&value) {
        return Err(LogError::new("an affect.change payload is off its scale"));
    }
    Ok(())
}

/// Whether `payload` is of the shape rows of `kind` carry. `Ok(false)` for
/// a kind whose payload is not one of these (`node.*`, ...).
pub fn check(kind: &str, payload: &Value) -> Result<bool, LogError> {
    match kind {
        "segment.open" => {
            let open: SegmentOpen = read(kind, payload)?;
            if open.place.is_empty() {
                return Err(LogError::new("a segment.open payload has no place"));
            }
        }
        "segment.line" => {
            let line: SegmentLine = read(kind, payload)?;
            if line.pictures == Some(0) {
                return Err(LogError::new("a segment.line payload counts no pictures"));
            }
            if let Some(card) = &line.card
                && !crate::row::is_token(card)
            {
                return Err(LogError::new("a segment.line payload names no card"));
            }
            if line.pictures.is_none() && line.card.is_none() {
                check_text(kind, &line.text)?;
            }
        }
        "segment.close" => {
            read::<SegmentClose>(kind, payload)?;
        }
        "card.version" => {
            read::<CardVersion>(kind, payload)?;
        }
        "card.answered" => {
            let answered: CardAnswered = read(kind, payload)?;
            let word = !answered.action.is_empty()
                && answered.action.len() <= 32
                && answered
                    .action
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
            if !word || !crate::row::is_token(&answered.version) {
                return Err(LogError::new(
                    "a card.answered payload names no action or version",
                ));
            }
        }
        "memory.write" => {
            let memory: MemoryWrite = read(kind, payload)?;
            check_text(kind, &memory.text)?;
            for replaced in &memory.replaces {
                check_memory(kind, replaced)?;
            }
            if memory.cues.iter().any(|cue| cue.trim().is_empty()) {
                return Err(LogError::new("a memory.write payload has an empty cue"));
            }
            if let Some(source) = &memory.source
                && (source.is_empty()
                    || source.len() > MAX_MEMORY_SOURCE
                    || !source.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    }))
            {
                return Err(LogError::new(
                    "a memory.write payload names where it was learned with another kind of word",
                ));
            }
            if memory.when.len() > MAX_MEMORY_WHEN
                || memory
                    .when
                    .iter()
                    .any(|when| when.trim().is_empty() || when.chars().count() > MEMORY_WHEN_CHARS)
            {
                return Err(LogError::new(
                    "a memory.write payload says when it would come to mind out of its bounds",
                ));
            }
        }
        "memory.retire" => {
            let retire: MemoryRetire = read(kind, payload)?;
            if retire.why.is_empty() {
                return Err(LogError::new("a memory.retire payload has no why"));
            }
            if let Some(by) = &retire.by {
                check_memory(kind, by)?;
            }
        }
        "memory.restore" => {
            read::<MemoryRestore>(kind, payload)?;
        }
        "affect.change" => {
            let change: AffectChange = read(kind, payload)?;
            check_scale(change.valence)?;
            check_scale(change.arousal)?;
        }
        "speech.said" => {
            let said: SpeechSaid = read(kind, payload)?;
            check_text(kind, &said.text)?;
            if said.place.is_empty() || said.exit.is_empty() {
                return Err(LogError::new("a speech.said payload has no place or exit"));
            }
            if !SPEECH_SOURCES.contains(&said.source.as_str()) {
                return Err(LogError::new("a speech.said payload has no known source"));
            }
            if !SPEECH_EXTENTS.contains(&said.extent.as_str()) {
                return Err(LogError::new("a speech.said payload has no known extent"));
            }
        }
        "persona.field" => {
            let field: PersonaField = read(kind, payload)?;
            if field.field.is_empty()
                || field.field.chars().count() > myriad_persona_sync::persona::MAX_FIELD_CHARS
            {
                return Err(LogError::new("a persona.field payload names no field"));
            }
        }
        "state.write" => {
            let state: StateWrite = read(kind, payload)?;
            if !STATE_KINDS.contains(&state.kind.as_str()) {
                return Err(LogError::new(
                    "a state.write payload is of no kind it carries",
                ));
            }
            let entity = myriad_persona_sync::Kind::parse(&state.kind)
                .ok_or_else(|| LogError::new("a state.write payload is of no kind it carries"))?;
            // A body in a later format than this core's is one it cannot
            // check: not taken until this side reads that format. One in an
            // earlier format it still reads is read as it was written.
            if state.format > myriad_persona_sync::FORMAT_VERSION {
                return Ok(false);
            }
            if !myriad_persona_sync::READ_FORMAT_VERSIONS.contains(&state.format) {
                return Err(LogError::new(
                    "a state.write payload is in no format this core reads",
                ));
            }
            let body = myriad_persona_sync::entity::check_body(
                entity,
                state.state,
                &state.id,
                state.body.clone(),
            )
            .map_err(|_| LogError::new("a state.write payload's body is not of its kind"))?;
            if state.format == myriad_persona_sync::FORMAT_VERSION && body != state.body {
                return Err(LogError::new(
                    "a state.write payload's body is not in its one form",
                ));
            }
        }
        "case.disagreed" => {
            let case: CaseDisagreed = read(kind, payload)?;
            check_text(kind, &case.about)?;
            check_text(kind, &case.view)?;
        }
        "case.insisted" => {
            read::<CaseAt>(kind, payload)?;
        }
        "case.persuaded" | "case.owned" => {
            read::<CaseMessage>(kind, payload)?;
        }
        "case.reconsidered" => {
            read::<CaseReconsidered>(kind, payload)?;
        }
        "case.work" => {
            read::<CaseWork>(kind, payload)?;
        }
        "case.tested" => {
            let tested: CaseTested = read(kind, payload)?;
            if !["passed", "failed"].contains(&tested.result.as_str()) {
                return Err(LogError::new("a case.tested payload has no known result"));
            }
        }
        "case.outcome" => {
            let outcome: CaseOutcome = read(kind, payload)?;
            if !["worked", "failed"].contains(&outcome.result.as_str())
                || !CASE_BASES.contains(&outcome.basis.as_str())
            {
                return Err(LogError::new("a case.outcome payload has no known result"));
            }
        }
        "case.verdict" => {
            let verdict: CaseVerdict = read(kind, payload)?;
            if !CASE_BASES.contains(&verdict.basis.as_str()) {
                return Err(LogError::new("a case.verdict payload has no known basis"));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn each_kind_reads_its_shape_and_ignores_fields_it_does_not_know() {
        let cases = [
            ("segment.open", json!({"at": 1, "place": "web"})),
            (
                "segment.line",
                json!({"at": 2, "text": "早", "line": "m_1", "later": true}),
            ),
            ("segment.close", json!({"at": 3})),
            (
                "memory.write",
                json!({"at": 4, "text": "喜欢热可可", "kind": "preference", "replaces": ["memory:m-1"]}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "第一次一起看雨", "kind": "narrative",
                    "speaker": "her", "source": "spoke_up", "when": ["有人说起下雨天时"]}),
            ),
            (
                "memory.retire",
                json!({"at": 5, "why": "superseded", "by": "memory:m-2"}),
            ),
            ("memory.restore", json!({"at": 6})),
            (
                "affect.change",
                json!({"at": 7, "valence": 62.5, "arousal": 40.0, "band": "warm", "cause": "chat"}),
            ),
            (
                "speech.said",
                json!({"at": 8, "text": "早", "place": "app", "source": "event", "called": false,
                    "thing": "event:e-1", "exit": "voice", "extent": "full", "heard": 1, "later": 1}),
            ),
            (
                "speech.said",
                json!({"at": 9, "text": "群里说一声", "place": "group", "source": "group_share",
                    "called": false, "exit": "im", "extent": "full", "groupName": "猫猫群"}),
            ),
        ];
        for (kind, payload) in cases {
            assert_eq!(check(kind, &payload), Ok(true), "{kind}");
        }
        assert_eq!(check("node.noted", &json!({"anything": 1})), Ok(false));
    }

    #[test]
    fn speech_sources_are_sorted_and_unique() {
        assert!(SPEECH_SOURCES.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_payload_off_its_shape_is_refused() {
        for (kind, payload) in [
            ("segment.open", json!({"at": 1, "place": ""})),
            ("segment.line", json!({"at": 2, "text": "  "})),
            ("segment.line", json!({"text": "no time"})),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "replaces": ["node:a"]}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "source": "Chat Room"}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "source": ""}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "speaker": "someone"}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "when": ["a", "b", "c", "d"]}),
            ),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "when": [" "]}),
            ),
            ("memory.retire", json!({"at": 5, "why": ""})),
            (
                "affect.change",
                json!({"at": 7, "valence": 120.0, "arousal": 40.0, "band": "x", "cause": "y"}),
            ),
            (
                "speech.said",
                json!({"at": 8, "text": "早", "place": "app", "source": "event", "called": false,
                    "exit": "voice", "extent": "half"}),
            ),
            (
                "speech.said",
                json!({"at": 8, "text": "早", "place": "", "source": "event", "called": false,
                    "exit": "voice", "extent": "full"}),
            ),
            (
                "speech.said",
                json!({"at": 8, "text": "早", "place": "app", "source": "daydream", "called": false,
                    "exit": "voice", "extent": "full"}),
            ),
        ] {
            assert!(check(kind, &payload).is_err(), "{kind} {payload}");
        }
    }
}
