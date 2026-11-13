//! What a coding-agent session's change (`now.session` on her link) puts on
//! her nodes, by rules and without asking her: the repository is a whole (a
//! topic of its `repo:` key), each session a task under it (`part_of`).
//!
//! | change | rows |
//! | --- | --- |
//! | started | the task, under the repository |
//! | asked | an open item: what it asked; where it stands: waiting on the owner |
//! | finished | open items closed; the task resting (a turn ended, not the task); one line of its result on the repository |
//! | failed | where it stands: stuck, and why |
//! | quiet | where it stands only |
//!
//! Every frame carries the session's whole picture. What the owner asked of
//! it, what it said last, the files it changed and its checks go on the
//! task when they say something new: their row's id comes from what they
//! hold, so the same words again are the same row. Where it stands, its
//! open items and its status are written for each change: their row's id
//! comes from when it changed, so the same frame sent again is the same
//! rows, and a later change is new ones even when it says what an earlier
//! one did. What the device told is not her view: every row is said by a
//! third party (a coding agent's output), or by the partner for what they
//! said to it, and her own judgment is never written here.
//!
//! A task's key comes from the device, the grant and the session
//! ([`task_key`]), so one session is one task on every host, and a grant
//! dropped is gone for good: a later one makes other keys. A host forgets a
//! grant by forgetting each task it made (the whole node: its created row
//! and status rows name nothing) and every other row that names the grant
//! ([`grant_ref`]): the only ones elsewhere are the results folded back onto
//! a repository, and her line on a repository (`look_progress`), which
//! names every grant whose sessions of it she was told of. Every row that
//! can name something names the grant and the session ([`session_ref`]), in
//! `because` or `refs`.
//!
//! The host gives what it keeps of the task as it stands after the rows of
//! the last change it took were written: two changes of one session are
//! taken one after the other, never from the same reading.
//!
//! Pure: nothing here reads a clock, a file or the network.

use myriad_her_link::now::{Change, CheckKind, CheckState, Session, WaitingOn};
use myriad_persona_sync::canonical::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{
        BODY_VERSION, Body, Created, Entry, Field, Object, ObjectKind, OpenItem, Related, Relation,
        Revised, Since, Standing, StandingState, Status,
    },
    entries::{RowOut, row_id_from},
    key::{Kind, node_id, repo_key},
    limits::{self, cut},
    place::clean,
    tags::{SaidBy, Sync},
};

/// The one thing on a task its standing (`node.standing`) is about: where
/// the session is now.
pub const NOW: &str = "now";

/// The key of the task one session is: `minted:s` and 24 hex of the
/// device, the grant and the session.
pub fn task_key(device: &str, grant: &str, session: &str) -> String {
    format!("minted:s{}", session_token(device, grant, session))
}

fn session_token(device: &str, grant: &str, session: &str) -> String {
    let seed = format!("session\u{1f}{device}\u{1f}{grant}\u{1f}{session}");
    sha256_hex(seed.as_bytes())[..24].to_string()
}

/// What a row written from one session names: `session:` and the same 24
/// hex as its task's key.
pub fn session_ref(device: &str, grant: &str, session: &str) -> String {
    format!("session:s{}", session_token(device, grant, session))
}

/// What every row written under one grant of a device names: `grant:g` and
/// 24 hex of the device and the grant. Forgetting a grant is forgetting
/// what names it, and the tasks it made.
pub fn grant_ref(device: &str, grant: &str) -> String {
    let seed = format!("grant\u{1f}{device}\u{1f}{grant}");
    format!("grant:g{}", &sha256_hex(seed.as_bytes())[..24])
}

/// One of the task's open items, closed or not, as the host keeps it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemKnown {
    pub item: String,
    pub text: String,
    #[serde(default)]
    pub closed: bool,
    /// The rows it stands at (the fold's `base`): what the next write of
    /// it is written against.
    pub base: Vec<String>,
}

/// The task as the host keeps it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskKnown {
    pub status: Status,
    /// The rows its status stands at; empty while it was never revised.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_base: Vec<String>,
    /// Its open items, closed ones too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemKnown>,
    /// The rows where it stands now ([`NOW`]) stands at.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standing_base: Vec<String>,
    /// The sync of the row that made it ([`created_sync`]), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<Sync>,
}

/// The sync of the row that made a node, from the node as the fold gives
/// it: its kind's first version is that row's. A fold keeps the latest 20
/// versions of a field, so a node whose kind changed more often reads the
/// sync of its oldest kept change; the nodes the rules write never change
/// kind.
pub fn created_sync(node: &Value) -> Option<Sync> {
    let kind = node.get("kind")?;
    let first = kind
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.first())
        .unwrap_or(kind);
    serde_json::from_value(first.get("sync")?.clone()).ok()
}

/// What the host keeps of a task, from its node as the fold gives it
/// (`Folded.nodes`, serialized): its status, its open items and where it
/// stands. A node the fold has no created row for yet is none.
pub fn task_known(node: &Value) -> Option<TaskKnown> {
    node.get("key")?;
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let status = node
        .get("status")
        .and_then(|status| serde_json::from_value(status["value"].clone()).ok())
        .unwrap_or(Status::Active);
    let status_base = node
        .get("status")
        .map(|status| strings(&status["base"]))
        .unwrap_or_default();
    let items = node
        .get("open")
        .and_then(Value::as_object)
        .map(|open| {
            open.iter()
                .map(|(item, held)| ItemKnown {
                    item: item.clone(),
                    text: held["value"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    closed: held["value"]["closed"] == true,
                    base: strings(&held["base"]),
                })
                .collect()
        })
        .unwrap_or_default();
    let standing_base = node
        .get("standing")
        .and_then(|standing| standing.get(NOW))
        .map(|now| strings(&now["base"]))
        .unwrap_or_default();
    Some(TaskKnown {
        status,
        status_base,
        items,
        standing_base,
        sync: created_sync(node),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionInput {
    /// The partner's private audience: what a device of theirs tells is
    /// theirs.
    pub audience: String,
    /// The device that told it.
    pub device: String,
    /// The change, as `now::read_session` read it.
    pub session: Session,
    /// Whether the host keeps a node of the repository's key.
    #[serde(default)]
    pub repo_known: bool,
    /// The sync of the row that made that node ([`created_sync`]), when
    /// known: what is written on a node keeps the sync it was made with.
    #[serde(default)]
    pub repo_sync: Option<Sync>,
    /// The task, when the host keeps it.
    #[serde(default)]
    pub task: Option<TaskKnown>,
    /// The sync tag of rows on nodes made now: shared when not given. Local
    /// (the host's own side) until every side it syncs with reads
    /// node.standing and rows a rule wrote (her link's `log3`). Rows on a
    /// node that is there already keep the sync it was made with
    /// ([`Self::repo_sync`], [`TaskKnown::sync`]), so a result entry on a
    /// shared repository is shared; a side without `log3` is not sent rows
    /// a rule wrote either way.
    #[serde(default = "shared")]
    pub sync: Sync,
}

fn shared() -> Sync {
    Sync::Shared
}

/// The key of the repository a session with no remote is in: `minted:l`
/// and 24 hex of the device and its folder, so it is that device's alone,
/// never one with another device's folder of the same name.
pub fn folder_key(device: &str, folder: &str) -> String {
    let seed = format!("folder\u{1f}{device}\u{1f}{folder}");
    format!("minted:l{}", &sha256_hex(seed.as_bytes())[..24])
}

/// One row to write and the node it is about. Rows of one change may carry
/// different syncs: each row's envelope takes its replicate from its own
/// body's sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub node: String,
    #[serde(flatten)]
    pub row: RowOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRows {
    pub repo: String,
    pub repo_key: String,
    pub task: String,
    pub task_key: String,
    /// In the order they are written. A row whose id the host has is
    /// written already.
    pub rows: Vec<Placed>,
}

/// Where a session stands, as a word an end says in its own language, and
/// the line that says it.
pub fn standing(session: &Session) -> (StandingState, String) {
    let failed = session
        .checks
        .iter()
        .find(|check| check.state == CheckState::Failed);
    match session.change {
        Change::Started => (StandingState::Working, "started working".to_string()),
        Change::Asked => (
            StandingState::WaitingOwner,
            match &session.from_agent {
                Some(_) => "waiting on the owner's answer to what it asked".to_string(),
                None => "waiting on the owner".to_string(),
            },
        ),
        Change::Finished => (StandingState::Finished, "finished a turn".to_string()),
        Change::Failed => (
            StandingState::Stuck,
            match failed {
                Some(check) => format!("stuck: its {} failed", check_word(check.kind)),
                None => "stuck: it kept failing".to_string(),
            },
        ),
        Change::Quiet => (
            match session.waiting_on {
                Some(WaitingOn::Checks) => StandingState::WaitingChecks,
                Some(WaitingOn::Program) => StandingState::WaitingProgram,
                Some(WaitingOn::Owner) => StandingState::WaitingOwner,
                Some(WaitingOn::Session) => StandingState::WaitingSession,
                _ => StandingState::Quiet,
            },
            match session.waiting_on {
                Some(WaitingOn::Checks) => "quiet for long, its tests or builds still running",
                Some(WaitingOn::Program) => "quiet for long, waiting on another program",
                Some(WaitingOn::Owner) => "quiet for long, waiting on the owner",
                Some(WaitingOn::Session) => {
                    "quiet for long, to report back to the session that sent it on"
                }
                _ => "quiet for long, neither asking nor moving",
            }
            .to_string(),
        ),
    }
}

fn check_word(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Test => "tests",
        CheckKind::Build => "build",
        CheckKind::Other => "check",
    }
}

fn state_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Running => "running",
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Ended => "ended",
    }
}

/// The checks in a line: the latest of each kind, `tests passed; build
/// failed`.
fn checks_line(session: &Session) -> Option<String> {
    let mut seen = Vec::new();
    let mut parts = Vec::new();
    for check in &session.checks {
        if seen.contains(&check.kind) {
            continue;
        }
        seen.push(check.kind);
        parts.push(format!(
            "{} {}",
            check_word(check.kind),
            state_word(check.state)
        ));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The repository's name: the last part of its path.
fn repo_name(remote: &str) -> String {
    remote.rsplit('/').next().unwrap_or(remote).to_string()
}

fn text(raw: &str) -> Option<String> {
    let text = cut(&clean(raw, false), limits::TEXT_CHARS);
    (!text.is_empty()).then_some(text)
}

fn digest(text: &str) -> String {
    sha256_hex(text.as_bytes())[..16].to_string()
}

struct Writer<'a> {
    rows: Vec<Placed>,
    audience: &'a str,
    reference: String,
    /// The grant's and the session's references, sorted.
    names: Vec<String>,
    /// The task's sync.
    sync: Sync,
    /// The repository's node and its sync.
    repo: (String, Sync),
}

impl Writer<'_> {
    fn push(
        &mut self,
        node: &str,
        source: &str,
        revision: &str,
        body: Body,
    ) -> Result<(), NodeError> {
        let kind = match &body {
            Body::Created(_) => "node.created",
            Body::Revised(_) => "node.revised",
            Body::OpenItem(_) => "node.open_item",
            Body::Entry(_) => "node.entry",
            Body::Related(_) => "node.related",
            Body::Standing(_) => "node.standing",
            _ => unreachable!("only these are made here"),
        };
        let body = Body::read(kind, &body.to_value())?;
        self.rows.push(Placed {
            node: node.to_string(),
            row: RowOut {
                id: row_id_from(kind, node, source, revision),
                kind: kind.to_string(),
                body: body.to_value(),
            },
        });
        Ok(())
    }

    fn entry(
        &mut self,
        node: &str,
        what: &str,
        said_by: SaidBy,
        text: String,
        state: Option<String>,
        revision: String,
    ) -> Result<(), NodeError> {
        self.push(
            node,
            &format!("{}/{what}", self.reference),
            &revision,
            Body::Entry(Entry {
                version: BODY_VERSION,
                sync: if node == self.repo.0 {
                    self.repo.1.clone()
                } else {
                    self.sync.clone()
                },
                said_by: vec![said_by],
                audience: self.audience.to_string(),
                text,
                refs: self.names.clone(),
                quote_only: false,
                happened: None,
                state,
                open: None,
            }),
        )
    }
}

/// The rows one change of a session writes.
pub fn session_rows(input: SessionInput) -> Result<SessionRows, NodeError> {
    let SessionInput {
        audience,
        device,
        session,
        repo_known,
        repo_sync,
        task,
        sync,
    } = input;
    crate::check_token("device", &device)?;
    sync.check()?;
    // The repository: by its remote on every device, or this device's own
    // folder.
    let (repo_key, name, object) = match (&session.remote, &session.folder) {
        (Some(remote), _) => {
            let key = repo_key(remote)?;
            if key != format!("repo:{remote}") {
                return Err(NodeError::new("remote is not as repoKey writes it"));
            }
            let object = Object {
                kind: ObjectKind::Repo,
                name: cut(remote, limits::NAME_CHARS),
                aliases: Vec::new(),
            };
            (key, repo_name(remote), Some(object))
        }
        (None, Some(folder)) => (folder_key(&device, folder), folder.clone(), None),
        (None, None) => {
            return Err(NodeError::new("a session with no remote names its folder"));
        }
    };
    let repo = node_id(&audience, &repo_key)?;
    // What is written on a node keeps the sync it was made with; a task
    // under a repository kept on one side is kept there too.
    let repo_sync = repo_sync
        .filter(|_| repo_known)
        .unwrap_or_else(|| sync.clone());
    let sync = match task.as_ref().and_then(|task| task.sync.clone()) {
        Some(made) => made,
        None if matches!(repo_sync, Sync::Local(_)) => repo_sync.clone(),
        None => sync,
    };
    sync.check()?;
    repo_sync.check()?;
    let task_key = task_key(&device, &session.grant, &session.session);
    let task_node = node_id(&audience, &task_key)?;
    let reference = session_ref(&device, &session.grant, &session.session);
    let mut because = vec![grant_ref(&device, &session.grant), reference.clone()];
    because.sort();
    let mut writer = Writer {
        rows: Vec::new(),
        audience: &audience,
        reference: reference.clone(),
        names: because.clone(),
        sync: sync.clone(),
        repo: (repo.clone(), repo_sync.clone()),
    };
    // A change's own rows: the same frame again is the same rows.
    let changed = session.occurred_at.to_string();

    if !repo_known {
        writer.push(
            &repo,
            &repo_key,
            "",
            Body::Created(Created {
                version: BODY_VERSION,
                sync: repo_sync.clone(),
                said_by: vec![SaidBy::ThirdParty],
                kind: Kind::Topic,
                key: repo_key.clone(),
                name: cut(&name, limits::NAME_CHARS),
                object,
            }),
        )?;
    }
    if task.is_none() {
        let title = session
            .title
            .as_deref()
            .map(|title| cut(&clean(title, true), limits::NAME_CHARS))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| cut(&format!("{} in {name}", session.tool), limits::NAME_CHARS));
        writer.push(
            &task_node,
            &task_key,
            "",
            Body::Created(Created {
                version: BODY_VERSION,
                sync: sync.clone(),
                said_by: vec![SaidBy::ThirdParty],
                kind: Kind::Task,
                key: task_key.clone(),
                name: title,
                object: None,
            }),
        )?;
        writer.push(
            &task_node,
            &format!("{reference}/part_of"),
            "",
            Body::Related(Related {
                version: BODY_VERSION,
                sync: sync.clone(),
                said_by: vec![SaidBy::ThirdParty],
                relation: Relation::PartOf,
                to: repo_key.clone(),
                reason: "a coding-agent session in this repository".to_string(),
                because: because.clone(),
            }),
        )?;
    }

    // What it holds, as it stands.
    if let Some(asked) = session.from_owner.as_deref().and_then(text) {
        let revision = digest(&asked);
        writer.entry(&task_node, "owner", SaidBy::Partner, asked, None, revision)?;
    }
    // What another session sent it on with: that session's, not theirs.
    if let Some(sent) = session.from_peer.as_deref().and_then(text) {
        let sent = match session
            .asked_by_title
            .as_deref()
            .map(|title| clean(title, true))
        {
            Some(title) if !title.is_empty() => {
                cut(&format!("sent on by «{title}»: {sent}"), limits::TEXT_CHARS)
            }
            _ => sent,
        };
        let revision = digest(&sent);
        writer.entry(&task_node, "peer", SaidBy::ThirdParty, sent, None, revision)?;
    }
    if let Some(said) = session.from_agent.as_deref().and_then(text) {
        let revision = digest(&said);
        writer.entry(
            &task_node,
            "agent",
            SaidBy::ThirdParty,
            said,
            None,
            revision,
        )?;
    }
    if !session.touched.is_empty() {
        let files = cut(
            &format!("changed {}", session.touched.join(", ")),
            limits::TEXT_CHARS,
        );
        let revision = digest(&files);
        writer.entry(
            &task_node,
            "touched",
            SaidBy::ThirdParty,
            files,
            None,
            revision,
        )?;
    }
    if let Some(checks) = checks_line(&session) {
        let revision = digest(&checks);
        writer.entry(
            &task_node,
            "checks",
            SaidBy::ThirdParty,
            checks,
            None,
            revision,
        )?;
    }
    // Where it stands now (现在), one line rewritten with each change.
    let (state, line) = standing(&session);
    writer.push(
        &task_node,
        &format!("{reference}/{NOW}"),
        &changed,
        Body::Standing(Standing {
            version: BODY_VERSION,
            sync: sync.clone(),
            said_by: vec![SaidBy::ThirdParty],
            base: task
                .as_ref()
                .map(|task| task.standing_base.clone())
                .unwrap_or_default(),
            item: NOW.to_string(),
            text: line,
            since: Some(Since::At(session.occurred_at.saturating_mul(1000))),
            state: Some(state),
            because: because.clone(),
            ended: false,
        }),
    )?;

    // What waits on the owner: one item per question, opened when it is
    // asked (again, when it was closed), closed once the session goes on.
    let items = task
        .as_ref()
        .map(|task| task.items.as_slice())
        .unwrap_or(&[]);
    // Waiting on them, whatever the change: it asked, or says so.
    let waits_on_owner =
        session.change == Change::Asked || session.waiting_on == Some(WaitingOn::Owner);
    let asking = waits_on_owner.then(|| {
        let question = session
            .from_agent
            .as_deref()
            .and_then(text)
            .unwrap_or_else(|| "it waits on the owner's answer".to_string());
        (format!("ask-{}", digest(&question)), question)
    });
    let keeps_open = matches!(session.change, Change::Failed | Change::Quiet);
    let mut item_row = |item: &str, text: &str, base: Vec<String>, closed: bool| {
        writer.push(
            &task_node,
            &format!("{reference}/{item}"),
            &changed,
            Body::OpenItem(OpenItem {
                version: BODY_VERSION,
                sync: sync.clone(),
                said_by: vec![SaidBy::ThirdParty],
                base,
                item: item.to_string(),
                text: text.to_string(),
                because: because.clone(),
                closed,
            }),
        )
    };
    // A new question replaces the ones before it; going on closes them.
    if asking.is_some() || !keeps_open {
        for known in items.iter().filter(|known| !known.closed) {
            if asking.as_ref().is_some_and(|(item, _)| item == &known.item) {
                continue;
            }
            item_row(&known.item, &known.text, known.base.clone(), true)?;
        }
    }
    if let Some((item, question)) = &asking {
        match items.iter().find(|known| &known.item == item) {
            None => item_row(item, question, Vec::new(), false)?,
            Some(known) if known.closed => item_row(item, question, known.base.clone(), false)?,
            Some(_) => {}
        }
    }

    // Resting or under way. A finished turn is only that this turn stopped
    // (its last words may still ask the owner something): the task rests
    // until the session goes on, and is done only when she marks it so.
    let status = if session.change == Change::Finished {
        Status::Resting
    } else {
        Status::Active
    };
    let was = task.as_ref().map_or(Status::Active, |task| task.status);
    if status != was {
        let base: Vec<String> = task
            .as_ref()
            .map(|task| task.status_base.clone())
            .unwrap_or_default();
        writer.push(
            &task_node,
            &format!("{reference}/status"),
            &format!("{changed}-{}", status_word(status)),
            Body::Revised(Revised {
                version: BODY_VERSION,
                sync: sync.clone(),
                said_by: vec![SaidBy::ThirdParty],
                base,
                field: Field::Status,
                person: None,
                value: serde_json::to_value(status).map_err(|error| {
                    NodeError::new(format!("status does not serialize: {error}"))
                })?,
            }),
        )?;
    }
    if session.change == Change::Finished {
        let title = session
            .title
            .as_deref()
            .map(|title| clean(title, true))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| format!("a {} session", session.tool));
        let mut result = format!("{title}: finished a turn");
        if let Some(checks) = checks_line(&session) {
            result.push_str(&format!("; {checks}"));
        }
        if !session.touched.is_empty() {
            result.push_str(&format!("; {} files changed", session.touched.len()));
        }
        writer.entry(
            &repo,
            "result",
            SaidBy::ThirdParty,
            cut(&result, limits::TEXT_CHARS),
            None,
            changed.clone(),
        )?;
    }

    Ok(SessionRows {
        repo,
        repo_key,
        task: task_node,
        task_key,
        rows: writer.rows,
    })
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Ongoing => "ongoing",
        Status::Resting => "resting",
        Status::Done => "done",
        Status::Dropped => "dropped",
    }
}

/// Whether a row's body names `reference` (in `because` or `refs`): it was
/// written from that session, and goes when its grant is forgotten.
pub fn names_session(body: &Value, reference: &str) -> bool {
    ["because", "refs"].iter().any(|field| {
        body.get(field)
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(reference)))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session(change: &str) -> Session {
        myriad_her_link::now::read_session(&json!({
            "grant": "g_0123456789",
            "session": "s-1",
            "change": change,
            "occurredAt": 1_760_000_000_000_i64,
            "tool": "coding-agent",
            "remote": "github.com/me/repo",
            "title": "Split the store",
            "fromOwner": "split the store and commit it",
            "fromAgent": "Committed locally. Push now? (push / wait)",
            "touched": ["src/a.rs", "src/b.rs"],
            "checks": [{"kind": "test", "state": "passed", "at": 1}],
        }))
        .unwrap()
    }

    fn input(change: &str, repo_known: bool, task: Option<TaskKnown>) -> SessionInput {
        SessionInput {
            audience: "personal:7".into(),
            device: "dev_mac".into(),
            session: session(change),
            repo_known,
            repo_sync: None,
            task,
            sync: Sync::Shared,
        }
    }

    fn kinds(rows: &SessionRows) -> Vec<(&str, &str)> {
        rows.rows
            .iter()
            .map(|placed| {
                (
                    if placed.node == rows.repo {
                        "repo"
                    } else {
                        "task"
                    },
                    placed.row.kind.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn a_new_session_is_a_task_under_its_repository() {
        let rows = session_rows(input("started", false, None)).unwrap();
        assert_eq!(rows.repo_key, "repo:github.com/me/repo");
        assert!(rows.task_key.starts_with("minted:s"));
        assert_eq!(
            kinds(&rows),
            [
                ("repo", "node.created"),
                ("task", "node.created"),
                ("task", "node.related"),
                ("task", "node.entry"),
                ("task", "node.entry"),
                ("task", "node.entry"),
                ("task", "node.entry"),
                ("task", "node.standing"),
            ]
        );
        assert_eq!(rows.rows[0].row.body["object"]["kind"], "repo");
        assert_eq!(rows.rows[1].row.body["kind"], "task");
        assert_eq!(rows.rows[1].row.body["name"], "Split the store");
        assert_eq!(rows.rows[2].row.body["relation"], "part_of");
        assert_eq!(rows.rows[2].row.body["to"], "repo:github.com/me/repo");
        // What the owner said to it is theirs; the rest a third party's.
        assert_eq!(rows.rows[3].row.body["saidBy"], json!(["partner"]));
        for placed in &rows.rows[4..] {
            assert_eq!(placed.row.body["saidBy"], json!(["third_party"]));
        }
        assert_eq!(rows.rows[7].row.kind, "node.standing");
        assert_eq!(rows.rows[7].row.body["text"], "started working");
        assert_eq!(rows.rows[7].row.body["since"], 1_760_000_000_000_000_i64);
        // Never her own.
        assert!(
            rows.rows
                .iter()
                .all(|placed| placed.row.body["saidBy"] != json!(["her"]))
        );
    }

    #[test]
    fn the_same_picture_again_is_the_same_rows() {
        let once = session_rows(input("started", false, None)).unwrap();
        let again = session_rows(input("started", true, None)).unwrap();
        let ids = |rows: &SessionRows| {
            rows.rows
                .iter()
                .map(|placed| placed.row.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&once)[1..], ids(&again)[..]);
    }

    #[test]
    fn asking_opens_an_item_and_finishing_closes_it_and_folds_back() {
        let task = TaskKnown {
            status: Status::Active,
            status_base: Vec::new(),
            items: Vec::new(),
            standing_base: Vec::new(),
            sync: None,
        };
        let asked = session_rows(input("asked", true, Some(task))).unwrap();
        let item = asked
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.open_item")
            .unwrap();
        assert_eq!(
            item.row.body["text"],
            "Committed locally. Push now? (push / wait)"
        );
        let standing = asked
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.standing")
            .unwrap();
        assert_eq!(
            standing.row.body["text"],
            "waiting on the owner's answer to what it asked"
        );
        assert!(
            !asked
                .rows
                .iter()
                .any(|placed| placed.row.kind == "node.revised")
        );

        let open = ItemKnown {
            item: item.row.body["item"].as_str().unwrap().into(),
            text: item.row.body["text"].as_str().unwrap().into(),
            closed: false,
            base: vec![item.row.id.clone()],
        };
        // Asked again with the same question: nothing new opens.
        let again = session_rows(input(
            "asked",
            true,
            Some(TaskKnown {
                status: Status::Active,
                status_base: Vec::new(),
                items: vec![open.clone()],
                standing_base: Vec::new(),
                sync: None,
            }),
        ))
        .unwrap();
        assert!(
            !again
                .rows
                .iter()
                .any(|placed| placed.row.kind == "node.open_item")
        );
        // Quiet keeps it open.
        let quiet = session_rows(input(
            "quiet",
            true,
            Some(TaskKnown {
                status: Status::Active,
                status_base: Vec::new(),
                items: vec![open.clone()],
                standing_base: Vec::new(),
                sync: None,
            }),
        ))
        .unwrap();
        assert!(
            !quiet
                .rows
                .iter()
                .any(|placed| placed.row.kind == "node.open_item")
        );

        let finished = session_rows(input(
            "finished",
            true,
            Some(TaskKnown {
                status: Status::Active,
                status_base: Vec::new(),
                items: vec![open.clone()],
                standing_base: Vec::new(),
                sync: None,
            }),
        ))
        .unwrap();
        let closed = finished
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.open_item")
            .unwrap();
        assert_eq!(closed.row.body["closed"], true);
        assert_eq!(closed.row.body["base"], json!(open.base));
        let status = finished
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.revised")
            .unwrap();
        assert_eq!(status.row.body["value"], "resting");
        let result = finished
            .rows
            .iter()
            .find(|placed| placed.node == finished.repo)
            .unwrap();
        assert_eq!(
            result.row.body["text"],
            "Split the store: finished a turn; tests passed; 2 files changed"
        );
    }

    #[test]
    fn a_dropped_grant_is_gone_and_a_new_one_starts_afresh() {
        let one = session_rows(input("started", false, None)).unwrap();
        let mut later = input("started", false, None);
        later.session.grant = "g_9876543210".into();
        let two = session_rows(later).unwrap();
        assert_ne!(one.task_key, two.task_key);
        assert_eq!(one.repo_key, two.repo_key);
        let reference = session_ref("dev_mac", "g_0123456789", "s-1");
        let grant = grant_ref("dev_mac", "g_0123456789");
        // Off the task (forgotten whole), only what names the grant and
        // the session; on it, every row that can name something.
        for placed in &one.rows[1..] {
            let names = names_session(&placed.row.body, &reference)
                && names_session(&placed.row.body, &grant);
            let can_name = !matches!(placed.row.kind.as_str(), "node.created" | "node.revised");
            assert!(placed.node == one.task, "{placed:?}");
            assert_eq!(names, can_name, "{placed:?}");
        }
        let finished = session_rows(input("finished", true, None)).unwrap();
        assert!(
            finished
                .rows
                .iter()
                .all(|placed| placed.node == finished.task
                    || names_session(&placed.row.body, &grant))
        );
        let result = finished
            .rows
            .iter()
            .find(|placed| placed.node == finished.repo)
            .unwrap();
        assert!(names_session(&result.row.body, &reference));
        assert!(!names_session(
            &result.row.body,
            &session_ref("dev_mac", "g_9876543210", "s-1")
        ));
    }

    #[test]
    fn a_stuck_session_says_why() {
        let mut failed = input("failed", true, None);
        failed.session.checks = vec![myriad_her_link::now::Check {
            kind: CheckKind::Build,
            state: CheckState::Failed,
            at: 2,
        }];
        let (state, line) = standing(&failed.session);
        assert_eq!(state, StandingState::Stuck);
        assert_eq!(line, "stuck: its build failed");
        let mut quiet = session("quiet");
        quiet.waiting_on = Some(WaitingOn::Checks);
        assert_eq!(standing(&quiet).0, StandingState::WaitingChecks);
        for change in ["started", "asked", "finished", "failed", "quiet"] {
            assert!(!standing(&session(change)).1.is_empty());
        }
    }

    /// What the link's reader takes, the nodes take as a repository's key.
    #[test]
    fn every_remote_the_link_takes_is_a_repository_key() {
        for remote in [
            "github.com/me/repo",
            "gitlab.example.co.jp/group/sub/repo",
            "a-b.com/x_y/z.w",
            "git.sr.ht/~me/repo",
            &format!("github.com/{}", "a".repeat(489)),
        ] {
            assert!(myriad_her_link::now::valid_remote(remote), "{remote}");
            assert_eq!(repo_key(remote).unwrap(), format!("repo:{remote}"));
        }
    }

    /// The task as the fold keeps it, from every row written so far, each
    /// written by the rule on the site.
    fn known(rows: &[&SessionRows]) -> TaskKnown {
        let folded = fold_of(rows, &[]);
        let task = rows[0].task.clone();
        let node = folded.nodes.iter().find(|node| node.id == task).unwrap();
        task_known(&serde_json::to_value(node).unwrap()).unwrap()
    }

    /// The rows folded as a host holds them, with `hers` (node, kind, body)
    /// written after them by her.
    fn fold_of(rows: &[&SessionRows], hers: &[(String, &str, Value)]) -> crate::Folded {
        use myriad_persona_log::Row;
        let mut log: Vec<Row> = Vec::new();
        let mut bodies = std::collections::BTreeMap::new();
        let mut seen = std::collections::BTreeSet::new();
        for placed in rows.iter().flat_map(|rows| rows.rows.iter()) {
            // A row the host has is written already.
            if !seen.insert(placed.row.id.clone()) {
                continue;
            }
            let at = 1_760_000_000_000_u64 + log.len() as u64;
            // A row kept on one side has no place in the shared order.
            let shared = placed.row.body["sync"] == json!("shared");
            let mut row = json!({
                "version": 1,
                "id": placed.row.id,
                "origin": "site",
                "author": "rule",
                "hlc": format!("{at:015}-000000-site"),
                "audience": "personal:7",
                "object": format!("node:{}", placed.node),
                "kind": placed.row.kind,
                "replicate": if shared { "shared" } else { "local" },
                "payload": {"ref": format!("body:{}", placed.row.id)},
            });
            if shared {
                row["dseq"] = json!(log.len() + 1);
            }
            log.push(serde_json::from_value(row).unwrap());
            bodies.insert(placed.row.id.clone(), placed.row.body.clone());
        }
        for (n, (node, kind, body)) in hers.iter().enumerate() {
            let at = 1_760_000_000_000_u64 + log.len() as u64;
            let id = format!("her_{n}");
            log.push(
                serde_json::from_value(json!({
                    "version": 1,
                    "id": id,
                    "origin": "site",
                    "author": "her",
                    "epoch": 1,
                    "hlc": format!("{at:015}-000000-site"),
                    "audience": "personal:7",
                    "object": format!("node:{node}"),
                    "kind": kind,
                    "replicate": "shared",
                    "dseq": log.len() + 1,
                    "payload": {"ref": format!("body:{id}")},
                }))
                .unwrap(),
            );
            bodies.insert(id, body.clone());
        }
        let folded = crate::fold(crate::FoldInput {
            rows: log,
            bodies,
            ledger: Vec::new(),
        })
        .unwrap();
        assert!(folded.skipped.is_empty(), "{:?}", folded.skipped);
        folded
    }

    /// A finished turn rests its task, and the repository does not count it
    /// done: only she marks it done, and then it counts.
    #[test]
    fn a_finished_turn_rests_and_is_done_only_when_she_marks_it() {
        let finished = session_rows(at("finished", 1, None)).unwrap();
        let parts = |folded: &crate::Folded| {
            folded
                .nodes
                .iter()
                .find(|node| node.id == finished.repo)
                .and_then(|node| node.parts.clone())
                .map(|parts| (parts.total, parts.done))
        };
        assert_eq!(parts(&fold_of(&[&finished], &[])), Some((1, 0)));
        let done = Body::Revised(Revised {
            version: BODY_VERSION,
            sync: crate::Sync::Shared,
            said_by: vec![SaidBy::Her],
            base: known(&[&finished]).status_base,
            field: Field::Status,
            person: None,
            value: serde_json::to_value(Status::Done).unwrap(),
        });
        let marked = fold_of(
            &[&finished],
            &[(finished.task.clone(), "node.revised", done.to_value())],
        );
        assert_eq!(parts(&marked), Some((1, 1)));
    }

    fn at(change: &str, when: i64, task: Option<TaskKnown>) -> SessionInput {
        let mut input = input(change, true, task);
        input.session.occurred_at = when;
        input
    }

    /// Asked, finished, asked the same again, finished again: each change
    /// writes its own rows, and the question opens again on the row it
    /// was closed at.
    #[test]
    fn a_question_asked_again_opens_again_and_each_change_is_new() {
        let one = session_rows(at("asked", 1, None)).unwrap();
        let two = session_rows(at("finished", 2, Some(known(&[&one])))).unwrap();
        let three = session_rows(at("asked", 3, Some(known(&[&one, &two])))).unwrap();
        let four = session_rows(at("finished", 4, Some(known(&[&one, &two, &three])))).unwrap();
        let ids: Vec<&str> = [&one, &two, &three, &four]
            .iter()
            .flat_map(|rows| rows.rows.iter().map(|placed| placed.row.id.as_str()))
            .collect();
        let state_rows = [&one, &two, &three, &four]
            .iter()
            .flat_map(|rows| rows.rows.iter())
            .filter(|placed| placed.row.kind == "node.standing")
            .count();
        assert_eq!(state_rows, 4, "where it stands, for each change");
        // Only the content entries repeat (the same words are one row).
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        let content = 4 * 3;
        assert_eq!(ids.len() - unique.len(), content);
        let reopened = three
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.open_item")
            .unwrap();
        assert_eq!(reopened.row.body["closed"], json!(null));
        let closed_at = two
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.open_item")
            .unwrap();
        assert_eq!(reopened.row.body["base"], json!([closed_at.row.id]));
        let after = known(&[&one, &two, &three, &four]);
        assert_eq!(after.status, Status::Resting);
        // Going on wakes it; the next finished turn rests it again.
        assert_eq!(known(&[&one, &two]).status, Status::Resting);
        assert_eq!(known(&[&one, &two, &three]).status, Status::Active);
        assert!(after.items.iter().all(|item| item.closed));
        assert_eq!(after.items.len(), 1, "one question, one item");
        // A session that starts again after a finished turn is under way.
        let five = session_rows(at("started", 5, Some(after.clone()))).unwrap();
        assert_eq!(
            known(&[&one, &two, &three, &four, &five]).status,
            Status::Active
        );
        // Where it stands moved on with each change, one version current.
        let now = four
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.standing")
            .unwrap();
        assert_eq!(after.standing_base, std::slice::from_ref(&now.row.id));
        // The same frame again: the same rows.
        let again = session_rows(at("asked", 3, Some(known(&[&one, &two])))).unwrap();
        assert_eq!(again, three);
    }

    /// No remote: the repository is this device's folder, never another
    /// device's of the same name; the rows stay on the host while it says so.
    #[test]
    fn a_repository_with_no_remote_is_its_devices_own() {
        let mut here = input("started", false, None);
        here.session.remote = None;
        here.session.folder = Some("notes".into());
        here.sync = Sync::Local("site".into());
        let mut there = here.clone();
        there.device = "dev_pc".into();
        let here = session_rows(here).unwrap();
        let there = session_rows(there).unwrap();
        assert!(here.repo_key.starts_with("minted:l"));
        assert_ne!(here.repo_key, there.repo_key);
        assert_eq!(here.rows[0].row.body["name"], "notes");
        assert!(here.rows[0].row.body.get("object").is_none());
        assert!(
            here.rows
                .iter()
                .all(|placed| placed.row.body["sync"] == json!({"local": "site"}))
        );
        let mut neither = input("started", false, None);
        neither.session.remote = None;
        assert!(session_rows(neither).is_err());
    }

    /// Whatever the change, waiting on the owner keeps a question open;
    /// what another session sent it on with is that session's.
    #[test]
    fn waiting_on_the_owner_always_has_its_item() {
        let mut quiet = input("quiet", true, None);
        quiet.session.waiting_on = Some(WaitingOn::Owner);
        let rows = session_rows(quiet).unwrap();
        assert!(
            rows.rows
                .iter()
                .any(|placed| placed.row.kind == "node.open_item")
        );
        let mut sent = input("started", true, None);
        sent.session.asked_by = Some(myriad_her_link::now::AskedBy::Session);
        sent.session.asked_by_title = Some("plan the release".into());
        sent.session.from_peer = Some("split it, then report back".into());
        let rows = session_rows(sent).unwrap();
        let peer = rows
            .rows
            .iter()
            .find(|placed| {
                placed.row.body["text"]
                    == "sent on by «plan the release»: split it, then report back"
            })
            .unwrap();
        assert_eq!(peer.row.body["saidBy"], json!(["third_party"]));
        let mut back = input("quiet", true, None);
        back.session.waiting_on = Some(WaitingOn::Session);
        let rows = session_rows(back).unwrap();
        assert!(
            !rows
                .rows
                .iter()
                .any(|placed| placed.row.kind == "node.open_item")
        );
    }

    /// What is written on a node keeps the sync it was made with: a
    /// repository and a task made on the host's side alone stay there when
    /// later changes come shared, and a task under such a repository too.
    #[test]
    fn a_node_keeps_the_sync_it_was_made_with() {
        let site = Sync::Local("site".into());
        let mut later = input("finished", true, None);
        later.repo_sync = Some(site.clone());
        let rows = session_rows(later).unwrap();
        assert!(
            rows.rows
                .iter()
                .all(|placed| placed.row.body["sync"] == json!({"local": "site"}))
        );
        let mut task_local = input("asked", true, None);
        task_local.task = Some(TaskKnown {
            status: Status::Active,
            status_base: Vec::new(),
            items: Vec::new(),
            standing_base: Vec::new(),
            sync: Some(site.clone()),
        });
        let rows = session_rows(task_local).unwrap();
        assert!(
            rows.rows
                .iter()
                .all(|placed| placed.row.body["sync"] == json!({"local": "site"}))
        );
        // The fold says what made a node.
        let made = session_rows(SessionInput {
            sync: site.clone(),
            ..input("started", false, None)
        })
        .unwrap();
        let folded = known(&[&made]);
        assert_eq!(folded.sync, Some(site));
    }
}
