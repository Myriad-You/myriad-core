//! Cards: what she puts before the owner in her talk with them, one thing
//! at a time, as facts the program can check, her view of it, and what
//! they may answer.
//!
//! A card is rows of her log in the owner's private audience
//! (`personal:<owner>`): `card.version` is the card as it stands now (her
//! row, or a rule's; the last in log order is the card, the earlier ones
//! its history), and `card.answered` the owner's answer to it (theirs
//! only). A message of hers carries a card by its id (`segment.line`'s
//! `card`): her line and the card are one message, and the line may be
//! empty. The same card in several messages is one card: where it shows
//! whole is the latest message that carries it.
//!
//! **Open.** `kind` and `scene` are open words. A side that knows no kind
//! shows the card's `title`, `facts` and `actions` all the same, so a new
//! kind of card needs no new release anywhere. The first kinds are
//! `decide`, `nod`, `progress` and `status`; the first scene is `agents`.
//!
//! **One id per thing.** The card of one scene, kind and node is always the
//! same card ([`card_id`]): a newer version of it takes its place. A card
//! about no node takes an id the writing side makes, after its own name
//! (`<origin>_…`), so two sides never make the same one.
//!
//! **Facts, not talk.** Facts are short things the program can check, each
//! with what it is about (`ref`: a node or a session) when that is one; her
//! view rests on them by number (`because`, from 1). No card holds the
//! words of a conversation. Its title, facts, view and labels pass the
//! secret scan, its ids the path and known-key rules (`wire`).
//!
//! **Waiting on the owner** ([`fold`]): a card waits on them while its
//! latest version has an action other than [`OPEN`] and no answer to that
//! very version came after it (an answer to an older one, however late,
//! is to what the owner saw then). A card stops waiting by itself when what it is
//! about no longer needs them (they answered in the session, it went on or
//! ended): its writer writes a new version with no action but `open` and
//! the state it is in.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::Row;
use myriad_persona_log::payloads::{CardAnswered, CardVersion};
use myriad_persona_sync::canonical::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::NodeError;
use crate::body::StandingState;

/// At most this many characters of title.
pub const TITLE_CHARS: usize = 60;
/// At most this many facts on a card.
pub const FACTS: usize = 8;
/// At most this many characters of one fact.
pub const FACT_CHARS: usize = 120;
/// At most this many characters of her view.
pub const VIEW_CHARS: usize = crate::limits::TEXT_CHARS;
/// At most this many actions on a card.
pub const ACTIONS: usize = 4;
/// At most this many characters of an action's label.
pub const LABEL_CHARS: usize = 12;
/// At most this many bytes of a word (`kind`, `scene`, an action's id).
pub const WORD_BYTES: usize = 32;
/// At most this many bytes of a card's id.
pub const ID_BYTES: usize = 80;
/// The fields of a card (`card.version`), as the wire takes them.
pub const FIELDS: [&str; 10] = [
    "at", "id", "kind", "scene", "subject", "title", "state", "facts", "view", "actions",
];
/// The action that opens what the card is about (a session) and answers
/// nothing: no `card.answered` is written of it.
pub const OPEN: &str = "open";

/// A word of a card: 1 to [`WORD_BYTES`] bytes of `a`–`z`, `0`–`9`, `_`.
pub fn word(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= WORD_BYTES
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A card's id: 1 to [`ID_BYTES`] bytes of ASCII letters, digits, `_`,
/// `-` and `.`.
pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// The id of the card of `scene` and `kind` about the node `subject`
/// (`node:<id>`): the same on every side, every time.
pub fn card_id(scene: &str, kind: &str, subject: &str) -> Result<String, NodeError> {
    if !word(scene) || !word(kind) {
        return Err(NodeError::new("a card's scene and kind are words"));
    }
    if !node_ref(subject) {
        return Err(NodeError::new("a card's subject is node:<id>"));
    }
    let digest = sha256_hex(format!("card\u{1f}{scene}\u{1f}{kind}\u{1f}{subject}").as_bytes());
    Ok(format!("c_{}", &digest[..32]))
}

fn node_ref(subject: &str) -> bool {
    subject
        .strip_prefix("node:")
        .is_some_and(|id| !id.is_empty() && id.len() <= ID_BYTES && !id.contains(char::is_control))
}

/// A card as one side writes it, checked: its words, ids and bounds, its
/// view's grounds, and that no id of a card about a node is other than
/// [`card_id`]'s. Not the secret scan: that is the wire's (`wire`), which
/// every body passes before it travels.
pub fn check(card: &CardVersion) -> Result<(), NodeError> {
    let bad = |why: &str| Err(NodeError::new(format!("card: {why}")));
    if !word(&card.kind) || !word(&card.scene) {
        return bad("its kind and scene are words");
    }
    if !id_ok(&card.id) {
        return bad("its id is not a card's id");
    }
    if let Some(subject) = &card.subject
        && card_id(&card.scene, &card.kind, subject)? != card.id
    {
        return bad("a card about a node goes by that node's card id");
    }
    let title = card.title.trim();
    if title.is_empty() || card.title.chars().count() > TITLE_CHARS {
        return bad("its title is empty or too long");
    }
    if let Some(state) = &card.state
        && serde_json::from_value::<StandingState>(Value::String(state.clone())).is_err()
    {
        return bad("its state is no word a standing says");
    }
    if card.facts.len() > FACTS {
        return bad("it has too many facts");
    }
    for fact in &card.facts {
        if fact.text.trim().is_empty() || fact.text.chars().count() > FACT_CHARS {
            return bad("a fact is empty or too long");
        }
        if fact
            .reference
            .as_deref()
            .is_some_and(|reference| reference.is_empty() || reference.contains(char::is_control))
        {
            return bad("a fact's ref is empty");
        }
    }
    if let Some(view) = &card.view {
        if view.text.trim().is_empty() || view.text.chars().count() > VIEW_CHARS {
            return bad("its view is empty or too long");
        }
        let mut seen = BTreeSet::new();
        for &ground in &view.because {
            if ground == 0 || ground as usize > card.facts.len() || !seen.insert(ground) {
                return bad("its view rests on a fact it does not have");
            }
        }
    }
    if card.actions.len() > ACTIONS {
        return bad("it has too many actions");
    }
    let mut ids = BTreeSet::new();
    for action in &card.actions {
        if !word(&action.id) || !ids.insert(action.id.as_str()) {
            return bad("an action's id is not a word, or comes twice");
        }
        if action.label.trim().is_empty() || action.label.chars().count() > LABEL_CHARS {
            return bad("an action's label is empty or too long");
        }
    }
    Ok(())
}

/// Her words on a card and its ids, for the wire's checks (`wire`).
pub(crate) fn parts(card: &CardVersion) -> (Vec<&str>, Vec<&str>) {
    let mut texts = vec![card.title.as_str()];
    let mut ids = vec![card.id.as_str(), card.kind.as_str(), card.scene.as_str()];
    ids.extend(card.subject.as_deref());
    ids.extend(card.state.as_deref());
    for fact in &card.facts {
        texts.push(&fact.text);
        ids.extend(fact.reference.as_deref());
    }
    if let Some(view) = &card.view {
        texts.push(&view.text);
    }
    for action in &card.actions {
        texts.push(&action.label);
        ids.push(&action.id);
    }
    (texts, ids)
}

/// Whether the card waits on the owner: an action other than [`OPEN`].
pub fn asks(card: &CardVersion) -> bool {
    card.actions.iter().any(|action| action.id != OPEN)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardIdInput {
    pub scene: String,
    pub kind: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardId {
    pub id: String,
}

/// `nodes.card_id`.
pub fn card_id_call(input: CardIdInput) -> Result<CardId, NodeError> {
    Ok(CardId {
        id: card_id(&input.scene, &input.kind, &input.subject)?,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardCheckInput {
    pub card: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardChecked {
    pub card: CardVersion,
}

/// `nodes.card_check`: a card read as its shape and checked, before a side
/// writes it.
pub fn check_call(input: CardCheckInput) -> Result<CardChecked, NodeError> {
    // A field the core does not name is one the wire would not take.
    if let Some(fields) = input.card.as_object()
        && let Some(unknown) = fields.keys().find(|key| !FIELDS.contains(&key.as_str()))
    {
        return Err(NodeError::new(format!("card: unknown field {unknown}")));
    }
    myriad_persona_log::payloads::check("card.version", &input.card)
        .map_err(|error| NodeError::new(error.to_string()))?;
    let card: CardVersion =
        serde_json::from_value(input.card).map_err(|_| NodeError::new("card: not of its shape"))?;
    check(&card)?;
    let (texts, ids) = parts(&card);
    let whole = serde_json::to_value(&card).map_err(|error| NodeError::new(error.to_string()))?;
    crate::wire::judge_parts(&texts, &ids, &whole)
        .map_err(|why| NodeError::new(format!("card: withheld on the wire ({why:?})")))?;
    Ok(CardChecked { card })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FoldInput {
    /// Her log's rows of the audience, any order; others than cards are
    /// passed by.
    pub rows: Vec<Row>,
    /// The bodies of those rows, by row id.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
}

/// One card as it stands.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardNow {
    pub card: CardVersion,
    /// The row id of its latest version.
    pub version: String,
    /// The answer to its latest version, when one came after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered: Option<CardAnswered>,
    /// It waits on the owner: its latest version asks something
    /// ([`asks`]) and no answer came after it.
    pub waiting: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Folded {
    /// Latest version first.
    pub cards: Vec<CardNow>,
}

/// `nodes.cards_fold`: every card of the rows as it stands now. A version
/// whose body is missing or does not hold is passed by, so the one before
/// it stands; an answer counts for the latest version when it came after
/// that version in log order, answers that very version (`version`: what
/// the owner saw), and names one of its actions. The rows are given with
/// forgets already taken in (as a host folds its log): a forgotten latest
/// version leaves the one before it standing.
pub fn fold(input: FoldInput) -> Result<Folded, NodeError> {
    let mut rows: Vec<Row> = input
        .rows
        .into_iter()
        .filter(|row| row.kind.starts_with("card.") && !row.forgotten)
        .collect();
    myriad_persona_log::row::sort(&mut rows);
    let mut cards: BTreeMap<String, (usize, CardNow)> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        let Some(body) = input.bodies.get(&row.id) else {
            continue;
        };
        match row.kind.as_str() {
            "card.version" => {
                let Ok(card) = serde_json::from_value::<CardVersion>(body.clone()) else {
                    continue;
                };
                if check(&card).is_err() || row.object != format!("card:{}", card.id) {
                    continue;
                }
                let waiting = asks(&card);
                cards.insert(
                    row.object.clone(),
                    (
                        at,
                        CardNow {
                            card,
                            version: row.id.clone(),
                            answered: None,
                            waiting,
                        },
                    ),
                );
            }
            "card.answered" => {
                let Ok(answer) = serde_json::from_value::<CardAnswered>(body.clone()) else {
                    continue;
                };
                if let Some((_, now)) = cards.get_mut(&row.object)
                    && answer.version == now.version
                    && answer.action != OPEN
                    && now
                        .card
                        .actions
                        .iter()
                        .any(|action| action.id == answer.action)
                {
                    now.waiting = false;
                    now.answered = Some(answer);
                }
            }
            _ => {}
        }
    }
    let mut cards: Vec<(usize, CardNow)> = cards.into_values().collect();
    cards.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    Ok(Folded {
        cards: cards.into_iter().map(|(_, now)| now).collect(),
    })
}
