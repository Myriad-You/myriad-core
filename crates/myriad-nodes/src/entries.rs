//! Facts on nodes: where a fact she keeps goes, and the rows that put it
//! there.
//!
//! A fact she keeps for good (what a host keeps as a memory row, with its
//! strength and what replaced it) has a place among her nodes: an entry
//! (`node.entry`) on a topic of the kind it is of, on the person it is only
//! about (「关于你」, `person:<id>`), or on none. The memory row stays where
//! it is; the entry names it (`memory:<id>`) and carries its text, so the
//! node reads whole without it.
//!
//! - **Where it goes** is hers to judge, by the model, by one rule
//!   ([`OF_A_KIND`]): going over a stretch of talk (the settling prompt,
//!   `myriad_merope::chat_remember`, reads her answer with [`read_under`])
//!   or sorting what she knew before, a batch at a time ([`sort_prepare`],
//!   [`sort_read`]). The prompt, what she is shown and how her answer is
//!   read are `myriad_merope::topics`, re-exported here: they were measured
//!   on real models and are the same bytes on every host. Each answer is a
//!   [`Placement`]: a topic shown, one she starts, about them, or nowhere
//!   (a label she was not shown, or nothing readable: decided, not asked
//!   again).
//! - **The rows** ([`fact_rows`]): a topic she starts is a `node.created`
//!   (a topic, key `minted:t…` made from the audience and its name
//!   ([`topic_key`]), so the same name in the same audience is the same
//!   node on every side) and its other names a `node.revised` of `aliases`;
//!   a fact is a `node.entry` whose refs hold `memory:<id>` and any
//!   original found (`message:<id>`…), quote-only when there is none. Every
//!   row's id comes from what it is made from ([`row_id_from`]), so writing
//!   the same again (a migration run twice) writes nothing new.
//!
//! Tidying the topics a host keeps in its own tables
//! (`myriad_merope::topics::tidy_system_prompt`) is not here: once topics
//! are node rows, her upkeep ([`crate::upkeep`]) keeps them.

use std::collections::BTreeSet;

use myriad_persona_sync::canonical::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use myriad_merope::happened::{
    HAPPENED_AND_STATE, Happened, STATE_CHARS, Standing, read_standing, span_of, state_of,
};
pub use myriad_merope::topics::{
    ALIASES, HOLDS_CHARS, NAME_CHARS, OF_A_KIND, OFFERED, SORT_BATCH, SORT_FACT_CHARS, Topic,
    Under, goes_by, name_words, offered, read_sorted, read_sorted_standing, read_under, same_name,
    shown, sort_input, sort_input_on, sort_schema, sort_system_prompt, under_schema,
};

use crate::{
    NodeError,
    body::{BODY_VERSION, Body, Created, Entry, Field, MEMORY_REF, Revised, is_original},
    check_token,
    key::{Key, Kind, id_audience, node_id},
    limits::{self, cut},
    place::clean,
    tags::{SaidBy, Sync, audience_fits},
};

/// A topic as a host offers it: its node id, its names, what is under it
/// (newest first).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TopicIn {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub holds: Vec<String>,
}

impl TopicIn {
    fn topic(&self) -> Topic<'_> {
        Topic {
            name: &self.name,
            aliases: &self.aliases,
            holds: &self.holds,
        }
    }
}

/// A fact to sort: the host's id for it (its memory row), its text, and
/// the day she learned it (`YYYY-MM-DD` in her zone), when the host gives
/// it, so what the fact tells of can be placed in time.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactIn {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub said: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SortInput {
    /// The topics of the audience, the host's order (most recently added
    /// to first).
    #[serde(default)]
    pub topics: Vec<TopicIn>,
    /// Facts under no topic yet, the first [`SORT_BATCH`] taken.
    pub facts: Vec<FactIn>,
    /// Her today (`YYYY-MM-DD`), when the host gives it.
    #[serde(default)]
    pub today: Option<String>,
}

/// What to ask her: the prompt, the schema, the input (labels and numbers,
/// no id), the topics shown in their order, and how many facts went in.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SortPrepared {
    pub system: String,
    pub schema: Value,
    pub input: String,
    /// The ids of the topics shown, `t1` first: what [`sort_read`] is
    /// given back.
    pub offered: Vec<String>,
    /// The first `taken` facts went in; the rest go in the next call.
    pub taken: usize,
}

fn check_unique<'a>(what: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), NodeError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        check_token(what, id)?;
        if !seen.insert(id) {
            return Err(NodeError::new(format!("{what}s repeat")));
        }
    }
    Ok(())
}

/// Sorting a batch of facts under topics: the topics offered over the
/// facts' words ([`offered`]), shown as [`shown`] shows them, and the facts
/// numbered from 1.
pub fn sort_prepare(input: SortInput) -> Result<SortPrepared, NodeError> {
    if input.facts.is_empty() {
        return Err(NodeError::new("no facts to sort"));
    }
    check_unique("fact id", input.facts.iter().map(|fact| fact.id.as_str()))?;
    check_unique(
        "topic id",
        input.topics.iter().map(|topic| topic.id.as_str()),
    )?;
    for day in input
        .facts
        .iter()
        .filter_map(|fact| fact.said.as_deref())
        .chain(input.today.as_deref())
    {
        if day.len() != 10 || span_of(day).is_none() {
            return Err(NodeError::new("said and today are a day, YYYY-MM-DD"));
        }
    }
    let taken: Vec<&FactIn> = input.facts.iter().take(SORT_BATCH).collect();
    let facts: Vec<String> = taken.iter().map(|fact| fact.text.clone()).collect();
    let dated: Vec<(&str, Option<&str>)> = taken
        .iter()
        .map(|fact| (fact.text.as_str(), fact.said.as_deref()))
        .collect();
    let topics: Vec<Topic> = input.topics.iter().map(TopicIn::topic).collect();
    let order = offered(&facts.join("\n"), &topics);
    Ok(SortPrepared {
        system: sort_system_prompt(),
        schema: sort_schema(),
        input: sort_input_on(shown(&topics, &order), &dated, input.today.as_deref()),
        offered: order
            .iter()
            .map(|index| input.topics[*index].id.clone())
            .collect(),
        taken: facts.len(),
    })
}

/// Where a fact goes, as a host stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Placement {
    /// On this topic node.
    Node { node: String },
    /// On a topic she starts ([`fact_rows`] with `start`).
    Start { name: String, aliases: Vec<String> },
    /// On their own node (`person:<id>`, 「关于你」).
    AboutThem,
    /// On none: decided, not asked again.
    Nowhere,
}

/// A topic the host already keeps, for a topic she starts under a name it
/// goes by ([`goes_by`]): that one, not a second.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Known {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Where `under` puts a fact, `offered` (ids, `t1` first) shown and `known`
/// kept: a topic she starts under a name a known one goes by is that one.
pub fn placement(under: &Under, offered: &[String], known: &[Known]) -> Placement {
    match under {
        Under::AboutThem => Placement::AboutThem,
        Under::Nowhere => Placement::Nowhere,
        Under::Offered(index) => {
            offered
                .get(*index)
                .map_or(Placement::Nowhere, |node| Placement::Node {
                    node: node.clone(),
                })
        }
        Under::New { name, aliases } => known
            .iter()
            .find(|topic| {
                goes_by(
                    name,
                    &Topic {
                        name: &topic.name,
                        aliases: &topic.aliases,
                        holds: &[],
                    },
                )
            })
            .map_or_else(
                || Placement::Start {
                    name: name.clone(),
                    aliases: aliases.clone(),
                },
                |topic| Placement::Node {
                    node: topic.id.clone(),
                },
            ),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SortReadInput {
    /// The model's answer, as it came.
    pub output: String,
    /// [`SortPrepared::offered`], as it was.
    #[serde(default)]
    pub offered: Vec<String>,
    /// The ids of the facts that went in, in order (the first `taken`).
    pub facts: Vec<String>,
    /// Every topic the host keeps in the audience.
    #[serde(default)]
    pub known: Vec<Known>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sorted {
    pub fact: String,
    #[serde(flatten)]
    pub placement: Placement,
    /// When it happened and where it stands, where she said and it reads.
    #[serde(flatten)]
    pub standing: Standing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SortRead {
    /// In the order she answered; a fact not here is still to sort.
    pub facts: Vec<Sorted>,
    /// No answer could be read at all (no JSON object with `facts` in it):
    /// the batch failed and is asked again, which is not the same as an
    /// answer that puts none of them anywhere.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unreadable: bool,
}

pub fn sort_read(input: SortReadInput) -> Result<SortRead, NodeError> {
    if input.facts.is_empty() {
        return Err(NodeError::new("no facts to sort"));
    }
    check_unique("fact id", input.facts.iter().map(String::as_str))?;
    check_unique("topic id", input.offered.iter().map(String::as_str))?;
    check_unique(
        "known id",
        input.known.iter().map(|topic| topic.id.as_str()),
    )?;
    let unreadable = myriad_merope::answer::parse::<Value>(&input.output)
        .and_then(|answer| answer.get("facts").map(Value::is_array))
        != Some(true);
    Ok(SortRead {
        unreadable,
        facts: read_sorted_standing(&input.output, input.offered.len(), input.facts.len())
            .into_iter()
            .map(|(at, under, standing)| Sorted {
                fact: input.facts[at].clone(),
                placement: placement(&under, &input.offered, &input.known),
                standing,
            })
            .collect(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnderReadInput {
    /// Where one fact goes in her answer (the settling answer's `topic`),
    /// as it came; absent when she gave none.
    #[serde(default)]
    pub value: Option<Value>,
    /// The ids of the topics shown, `t1` first.
    #[serde(default)]
    pub offered: Vec<String>,
    #[serde(default)]
    pub known: Vec<Known>,
}

pub fn under_read(input: UnderReadInput) -> Result<Placement, NodeError> {
    check_unique("topic id", input.offered.iter().map(String::as_str))?;
    check_unique(
        "known id",
        input.known.iter().map(|topic| topic.id.as_str()),
    )?;
    let under = read_under(input.value.as_ref(), input.offered.len());
    Ok(placement(&under, &input.offered, &input.known))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StandingReadInput {
    /// One fact of the settling answer (one of `lasting`), as it came: its
    /// `happened`, `state` and `open` are read.
    pub item: Value,
}

/// When one fact of the settling answer happened and where it stands
/// ([`read_standing`]): each none when absent or not readable.
pub fn standing_read(input: StandingReadInput) -> Standing {
    read_standing(&input.item)
}

/// The id of a row made from something else: of `kind`, on `node`, from
/// `source` (a memory row `memory:<id>`, a topic's key…) at its `revision`
/// (empty for none). The same four always make the same id, the `u…` form
/// her upkeep's rows have, from another seed so the two never meet.
pub fn row_id_from(kind: &str, node: &str, source: &str, revision: &str) -> String {
    let seed = format!("from\u{1f}{kind}\u{1f}{node}\u{1f}{source}\u{1f}{revision}");
    format!("u{}", &sha256_hex(seed.as_bytes())[..24])
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowIdInput {
    pub kind: String,
    pub node: String,
    pub source: String,
    #[serde(default)]
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowId {
    pub id: String,
}

/// [`row_id_from`], checked: a node kind the core reads, and tokens.
pub fn row_id(input: RowIdInput) -> Result<RowId, NodeError> {
    if !crate::body::READ.contains(&input.kind.as_str()) {
        return Err(NodeError::new("kind is not one the core reads"));
    }
    check_token("node", &input.node)?;
    check_token("source", &input.source)?;
    if input.revision.chars().any(char::is_control) {
        return Err(NodeError::new("revision holds a control character"));
    }
    Ok(RowId {
        id: row_id_from(&input.kind, &input.node, &input.source, &input.revision),
    })
}

/// The key of a topic she starts under `name` in `audience`: `minted:t` and
/// 24 hex of what the name's words ([`name_words`]) and the audience make,
/// so one name in one audience is one node on every side. The partner's
/// private talk makes it under its one fixed name, as a node id is made.
pub fn topic_key(audience: &str, name: &str) -> Result<String, NodeError> {
    check_audience(audience)?;
    let words = name_words(&clean(name, true));
    if words.is_empty() {
        return Err(NodeError::new("name is empty"));
    }
    let seed = format!("topic\u{1f}{}\u{1f}{words}", id_audience(audience));
    Ok(format!("minted:t{}", &sha256_hex(seed.as_bytes())[..24]))
}

fn check_audience(audience: &str) -> Result<(), NodeError> {
    check_token("audience", audience)?;
    myriad_persona_log::row::Audience::parse(audience)
        .map(|_| ())
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))
}

/// Other names as a `node.revised` of `aliases` writes them: each cleaned
/// and cut to a name, none empty or the name itself or another of them
/// again ([`same_name`]), the first [`limits::ALIASES`], sorted.
pub fn aliases_of(name: &str, aliases: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for alias in aliases {
        let alias = cut(&clean(alias, true), limits::NAME_CHARS);
        if alias.is_empty()
            || same_name(&alias, name)
            || kept.iter().any(|seen| same_name(seen, &alias))
        {
            continue;
        }
        kept.push(alias);
    }
    kept.truncate(limits::ALIASES);
    kept.sort();
    kept
}

/// A node to start: a topic she starts (no `key`: one is made from its
/// name, [`topic_key`]; a `minted:` key: one a host already keeps under it),
/// or a person's own node (`person:<id>`, `group:<id>`, `her`), once.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Start {
    #[serde(default)]
    pub key: Option<String>,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// A fact to put on the node: the memory row it is the place of, its text,
/// who said it, the originals found.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fact {
    /// The memory row's id.
    pub memory: String,
    pub text: String,
    pub said_by: Vec<SaidBy>,
    /// Originals found (`message:<id>`…); none and it is quote-only.
    #[serde(default)]
    pub refs: Vec<String>,
    /// The memory row's revision, for the entry's id: a revised memory is a
    /// new entry.
    #[serde(default)]
    pub revision: String,
    /// Where it was said, when not in this audience: only the partner's own
    /// words in a group, kept in their private audience.
    #[serde(default)]
    pub audience: Option<String>,
    /// When what it tells of happened, as her answer gave it
    /// ([`read_standing`]); one that does not hold is left out.
    #[serde(default)]
    pub happened: Option<Happened>,
    /// Where it stands; one that is not a word or two is left out, and
    /// `open` with it.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub open: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactRowsInput {
    pub audience: String,
    /// The rows' sync tag; shared when not given.
    #[serde(default = "shared")]
    pub sync: Sync,
    /// A node to start, the facts going on it.
    #[serde(default)]
    pub start: Option<Start>,
    /// Or the key of a node there already.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub facts: Vec<Fact>,
}

fn shared() -> Sync {
    Sync::Shared
}

/// One row to write: its id, kind and body. The host writes the envelope
/// (`node:<node>`, her as author, its clock), in this order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowOut {
    pub id: String,
    pub kind: String,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRows {
    pub node: String,
    pub key: String,
    pub rows: Vec<RowOut>,
}

fn row(node: &str, source: &str, revision: &str, body: Body) -> Result<RowOut, NodeError> {
    let kind = match &body {
        Body::Created(_) => "node.created",
        Body::Revised(_) => "node.revised",
        Body::Entry(_) => "node.entry",
        _ => unreachable!("only these are made here"),
    };
    // Checked as every side reads it.
    let body = Body::read(kind, &body.to_value())?;
    Ok(RowOut {
        id: row_id_from(kind, node, source, revision),
        kind: kind.to_string(),
        body: body.to_value(),
    })
}

/// The rows that start a node (with `start`) and put facts on it. A topic
/// is started only when the host keeps none it goes by ([`goes_by`]); a
/// person's own node only once: the created row's id comes from its key,
/// so starting one again with another name makes two rows of one id.
pub fn fact_rows(input: FactRowsInput) -> Result<FactRows, NodeError> {
    check_audience(&input.audience)?;
    input.sync.check()?;
    let audience = input.audience.as_str();
    let mut rows = Vec::new();
    let key = match (&input.start, &input.key) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(NodeError::new("give start or key, one of them"));
        }
        (None, Some(key)) => key.clone(),
        (Some(start), None) => {
            let name = cut(&clean(&start.name, true), limits::NAME_CHARS);
            if name.is_empty() {
                return Err(NodeError::new("name is empty"));
            }
            let key = match &start.key {
                Some(key) => key.clone(),
                None => topic_key(audience, &name)?,
            };
            let kind = match Key::parse(&key)? {
                Key::Minted(_) => Kind::Topic,
                Key::Person(_) | Key::Group(_) | Key::Her => Kind::Person,
                Key::Repo(_) => {
                    return Err(NodeError::new(
                        "a repository's node is started with its object",
                    ));
                }
            };
            let node = node_id(audience, &key)?;
            rows.push(row(
                &node,
                &key,
                "",
                Body::Created(Created {
                    version: BODY_VERSION,
                    sync: input.sync.clone(),
                    said_by: vec![SaidBy::Her],
                    kind,
                    key: key.clone(),
                    name: name.clone(),
                    object: None,
                }),
            )?);
            let aliases = aliases_of(&name, &start.aliases);
            if !aliases.is_empty() {
                rows.push(row(
                    &node,
                    &key,
                    "aliases",
                    Body::Revised(Revised {
                        version: BODY_VERSION,
                        sync: input.sync.clone(),
                        said_by: vec![SaidBy::Her],
                        base: Vec::new(),
                        field: Field::Aliases,
                        person: None,
                        value: Value::from(aliases),
                    }),
                )?);
            }
            key
        }
    };
    let node = node_id(audience, &key)?;
    check_unique(
        "memory",
        input.facts.iter().map(|fact| fact.memory.as_str()),
    )?;
    for fact in &input.facts {
        let mut said_by = fact.said_by.clone();
        said_by.sort();
        said_by.dedup();
        let said_in = fact.audience.as_deref().unwrap_or(audience);
        if !audience_fits(said_in, audience, &said_by) {
            return Err(NodeError::new(
                "a fact said elsewhere is only the partner's own words from a group",
            ));
        }
        let text = cut(&clean(&fact.text, false), limits::TEXT_CHARS);
        if text.is_empty() {
            return Err(NodeError::new("a fact's text is empty"));
        }
        let memory = format!("{MEMORY_REF}{}", fact.memory);
        let mut originals: Vec<String> = fact
            .refs
            .iter()
            .filter(|reference| is_original(reference))
            .cloned()
            .collect();
        originals.sort();
        originals.dedup();
        originals.truncate(limits::REFS - 1);
        let quote_only = originals.is_empty();
        let mut refs = originals;
        refs.push(memory.clone());
        refs.sort();
        let happened = fact
            .happened
            .clone()
            .filter(|happened| happened.check().is_ok());
        let state = fact.state.as_deref().and_then(state_of);
        let open = fact.open.filter(|_| state.is_some());
        rows.push(row(
            &node,
            &memory,
            &fact.revision,
            Body::Entry(Entry {
                version: BODY_VERSION,
                sync: input.sync.clone(),
                said_by,
                audience: said_in.to_string(),
                text,
                refs,
                quote_only,
                happened,
                state,
                open,
            }),
        )?);
    }
    Ok(FactRows { node, key, rows })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn one_name_in_one_audience_is_one_topic() {
        let key = topic_key("personal:7", "Seeing  Doctors").unwrap();
        assert_eq!(key, topic_key("personal:7", "seeing doctors").unwrap());
        assert_ne!(key, topic_key("personal:8", "seeing doctors").unwrap());
        assert!(key.starts_with("minted:t") && key.len() == "minted:t".len() + 24);
        assert!(Key::parse(&key).is_ok());
        // The partner's talk, whatever id the partner goes by.
        assert_eq!(
            topic_key("personal:local-owner", "plants").unwrap(),
            topic_key(
                "personal:partner_0199d5c0900033333333333333333333",
                "plants"
            )
            .unwrap()
        );
        assert!(topic_key("personal:7", " \n ").is_err());
    }

    #[test]
    fn a_row_made_from_the_same_thing_has_the_same_id() {
        let id = row_id_from("node.entry", "n", "memory:m1", "");
        assert_eq!(id, row_id_from("node.entry", "n", "memory:m1", ""));
        assert_ne!(id, row_id_from("node.entry", "n", "memory:m1", "2"));
        assert!(id.starts_with('u') && id.len() == 25);
    }

    #[test]
    fn a_fact_with_only_its_memory_is_quote_only() {
        let rows = fact_rows(
            serde_json::from_value(json!({
                "audience": "personal:7",
                "start": {"name": " houseplants ", "aliases": ["Plants", "houseplants", "plants", "植物"]},
                "facts": [
                    {"memory": "m1", "text": "has a snake plant", "saidBy": ["partner"]},
                    {"memory": "m2", "text": "waters on Sundays", "saidBy": ["partner", "partner"], "refs": ["message:b", "message:a", "memory:x"]}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let kinds: Vec<&str> = rows.rows.iter().map(|row| row.kind.as_str()).collect();
        assert_eq!(
            kinds,
            ["node.created", "node.revised", "node.entry", "node.entry"]
        );
        assert_eq!(rows.rows[0].body["name"], "houseplants");
        assert_eq!(rows.rows[1].body["value"], json!(["Plants", "植物"]));
        assert_eq!(rows.rows[2].body["refs"], json!(["memory:m1"]));
        assert_eq!(rows.rows[2].body["quoteOnly"], true);
        assert_eq!(
            rows.rows[3].body["refs"],
            json!(["memory:m2", "message:a", "message:b"])
        );
        assert!(rows.rows[3].body.get("quoteOnly").is_none());
        assert_eq!(rows.node, node_id("personal:7", &rows.key).unwrap());
    }
}
