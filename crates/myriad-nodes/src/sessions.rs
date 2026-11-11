//! What a coding-agent session's change (`now.session` on her link) puts on
//! her nodes, by rules and without asking her: the repository is a whole (a
//! topic of its `repo:` key), each session a task under it (`part_of`).
//!
//! | change | rows |
//! | --- | --- |
//! | started | the task, under the repository |
//! | asked | an open item: what it asked; where it stands: waiting on the owner |
//! | finished | open items closed; the task done; one line of its result on the repository |
//! | failed | where it stands: stuck, and why |
//! | quiet | where it stands only |
//!
//! Every frame carries the session's whole picture, so what the owner asked
//! of it, what it said last, the files it changed and its checks go on the
//! task each time they change: a row's id comes from what it holds, and the
//! same again is the same row. What the device told is not her view: every
//! row is said by a third party (a coding agent's output), or by the partner
//! for what they said to it, and her own judgment is never written here.
//!
//! A task's key comes from the device, the grant and the session
//! ([`task_key`]), so one session is one task on every host, and a grant
//! dropped is gone for good: a later one makes other keys. Everything
//! written from one session names it ([`session_ref`]), in `because` or
//! `refs`, so a host forgets a grant by forgetting its tasks and every row
//! that names one of them.
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
        Revised, Status,
    },
    entries::{RowOut, row_id_from},
    key::{Kind, node_id, repo_key},
    limits::{self, cut},
    place::clean,
    tags::{SaidBy, Sync},
};

/// The key of the task one session is: `minted:s` and 24 hex of the
/// device, the grant and the session.
pub fn task_key(device: &str, grant: &str, session: &str) -> String {
    format!("minted:s{}", session_token(device, grant, session))
}

fn session_token(device: &str, grant: &str, session: &str) -> String {
    let seed = format!("session\u{1f}{device}\u{1f}{grant}\u{1f}{session}");
    sha256_hex(seed.as_bytes())[..24].to_string()
}

/// What every row written from one session names: `session:` and the same
/// 24 hex as its task's key.
pub fn session_ref(device: &str, grant: &str, session: &str) -> String {
    format!("session:s{}", session_token(device, grant, session))
}

/// One of the task's open items, as the host keeps it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenKnown {
    pub item: String,
    pub text: String,
    /// The row it stands at.
    pub row: String,
}

/// The task as the host keeps it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskKnown {
    pub status: Status,
    /// The row its status stands at; none while it was never revised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_row: Option<String>,
    /// Its open items not closed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<OpenKnown>,
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
    /// The task, when the host keeps it.
    #[serde(default)]
    pub task: Option<TaskKnown>,
}

/// One row to write and the node it is about.
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

/// Where a session stands, a word or two, and the line that says it.
pub fn standing(session: &Session) -> (&'static str, String) {
    let failed = session
        .checks
        .iter()
        .find(|check| check.state == CheckState::Failed);
    match session.change {
        Change::Started => ("working", "started working".to_string()),
        Change::Asked => (
            "waiting owner",
            match &session.from_agent {
                Some(_) => "waiting on the owner's answer to what it asked".to_string(),
                None => "waiting on the owner".to_string(),
            },
        ),
        Change::Finished => (
            "finished",
            "finished, nothing waits on the owner".to_string(),
        ),
        Change::Failed => (
            "stuck",
            match failed {
                Some(check) => format!("stuck: its {} failed", check_word(check.kind)),
                None => "stuck: it kept failing".to_string(),
            },
        ),
        Change::Quiet => (
            match session.waiting_on {
                Some(WaitingOn::Checks) => "waiting checks",
                Some(WaitingOn::Program) => "waiting program",
                Some(WaitingOn::Owner) => "waiting owner",
                _ => "quiet",
            },
            match session.waiting_on {
                Some(WaitingOn::Checks) => "quiet for long, its tests or builds still running",
                Some(WaitingOn::Program) => "quiet for long, waiting on another program",
                Some(WaitingOn::Owner) => "quiet for long, waiting on the owner",
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
    ) -> Result<(), NodeError> {
        let revision = digest(&format!("{}\u{1f}{text}", state.as_deref().unwrap_or("")));
        self.push(
            node,
            &format!("{}/{what}", self.reference),
            &revision,
            Body::Entry(Entry {
                version: BODY_VERSION,
                sync: Sync::Shared,
                said_by: vec![said_by],
                audience: self.audience.to_string(),
                text,
                refs: vec![self.reference.clone()],
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
        task,
    } = input;
    crate::check_token("device", &device)?;
    let repo_key = repo_key(&session.remote)?;
    if repo_key != format!("repo:{}", session.remote) {
        return Err(NodeError::new("remote is not as repoKey writes it"));
    }
    let repo = node_id(&audience, &repo_key)?;
    let task_key = task_key(&device, &session.grant, &session.session);
    let task_node = node_id(&audience, &task_key)?;
    let reference = session_ref(&device, &session.grant, &session.session);
    let mut writer = Writer {
        rows: Vec::new(),
        audience: &audience,
        reference: reference.clone(),
    };
    let because = vec![reference.clone()];
    let name = repo_name(&session.remote);

    if !repo_known {
        writer.push(
            &repo,
            &repo_key,
            "",
            Body::Created(Created {
                version: BODY_VERSION,
                sync: Sync::Shared,
                said_by: vec![SaidBy::ThirdParty],
                kind: Kind::Topic,
                key: repo_key.clone(),
                name: cut(&name, limits::NAME_CHARS),
                object: Some(Object {
                    kind: ObjectKind::Repo,
                    name: cut(&session.remote, limits::NAME_CHARS),
                    aliases: Vec::new(),
                }),
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
                sync: Sync::Shared,
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
                sync: Sync::Shared,
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
        writer.entry(&task_node, "owner", SaidBy::Partner, asked, None)?;
    }
    if let Some(said) = session.from_agent.as_deref().and_then(text) {
        writer.entry(&task_node, "agent", SaidBy::ThirdParty, said, None)?;
    }
    if !session.touched.is_empty() {
        let files = cut(
            &format!("changed {}", session.touched.join(", ")),
            limits::TEXT_CHARS,
        );
        writer.entry(&task_node, "touched", SaidBy::ThirdParty, files, None)?;
    }
    if let Some(checks) = checks_line(&session) {
        writer.entry(&task_node, "checks", SaidBy::ThirdParty, checks, None)?;
    }
    let (state, line) = standing(&session);
    writer.entry(
        &task_node,
        "standing",
        SaidBy::ThirdParty,
        line,
        Some(state.to_string()),
    )?;

    // What waits on the owner.
    let open = task
        .as_ref()
        .map(|task| task.open.as_slice())
        .unwrap_or(&[]);
    let asking = (session.change == Change::Asked).then(|| {
        let question = session
            .from_agent
            .as_deref()
            .and_then(text)
            .unwrap_or_else(|| "it waits on the owner's answer".to_string());
        (format!("ask-{}", digest(&question)), question)
    });
    let keeps_open = matches!(session.change, Change::Failed | Change::Quiet);
    if !keeps_open {
        for known in open {
            if asking.as_ref().is_some_and(|(item, _)| item == &known.item) {
                continue;
            }
            writer.push(
                &task_node,
                &format!("{reference}/{}", known.item),
                "closed",
                Body::OpenItem(OpenItem {
                    version: BODY_VERSION,
                    sync: Sync::Shared,
                    said_by: vec![SaidBy::ThirdParty],
                    base: vec![known.row.clone()],
                    item: known.item.clone(),
                    text: known.text.clone(),
                    because: because.clone(),
                    closed: true,
                }),
            )?;
        }
    }
    if let Some((item, question)) = &asking
        && !open.iter().any(|known| &known.item == item)
    {
        writer.push(
            &task_node,
            &format!("{reference}/{item}"),
            "",
            Body::OpenItem(OpenItem {
                version: BODY_VERSION,
                sync: Sync::Shared,
                said_by: vec![SaidBy::ThirdParty],
                base: Vec::new(),
                item: item.clone(),
                text: question.clone(),
                because: because.clone(),
                closed: false,
            }),
        )?;
    }

    // Done or not.
    let status = if session.change == Change::Finished {
        Status::Done
    } else {
        Status::Active
    };
    let was = task.as_ref().map_or(Status::Active, |task| task.status);
    if status != was {
        let base: Vec<String> = task
            .as_ref()
            .and_then(|task| task.status_row.clone())
            .into_iter()
            .collect();
        writer.push(
            &task_node,
            &format!("{reference}/status"),
            &format!("{}\u{1f}{}", session.occurred_at, status_word(status)),
            Body::Revised(Revised {
                version: BODY_VERSION,
                sync: Sync::Shared,
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
        let mut result = format!("{title}: finished");
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
            task,
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
                ("task", "node.entry"),
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
        assert_eq!(rows.rows[7].row.body["state"], "working");
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
            status_row: None,
            open: Vec::new(),
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
            .find(|placed| placed.row.body.get("state").is_some())
            .unwrap();
        assert_eq!(standing.row.body["state"], "waiting owner");
        assert!(
            !asked
                .rows
                .iter()
                .any(|placed| placed.row.kind == "node.revised")
        );

        let open = OpenKnown {
            item: item.row.body["item"].as_str().unwrap().into(),
            text: item.row.body["text"].as_str().unwrap().into(),
            row: item.row.id.clone(),
        };
        // Asked again with the same question: nothing new opens.
        let again = session_rows(input(
            "asked",
            true,
            Some(TaskKnown {
                status: Status::Active,
                status_row: None,
                open: vec![open.clone()],
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
                status_row: None,
                open: vec![open.clone()],
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
                status_row: None,
                open: vec![open.clone()],
            }),
        ))
        .unwrap();
        let closed = finished
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.open_item")
            .unwrap();
        assert_eq!(closed.row.body["closed"], true);
        assert_eq!(closed.row.body["base"], json!([open.row]));
        let status = finished
            .rows
            .iter()
            .find(|placed| placed.row.kind == "node.revised")
            .unwrap();
        assert_eq!(status.row.body["value"], "done");
        let result = finished
            .rows
            .iter()
            .find(|placed| placed.node == finished.repo)
            .unwrap();
        assert_eq!(
            result.row.body["text"],
            "Split the store: finished; tests passed; 2 files changed"
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
        // Every row written from the session names it, but the
        // repository's own start.
        for placed in &one.rows[1..] {
            assert!(
                placed.node == one.task || names_session(&placed.row.body, &reference),
                "{placed:?}"
            );
        }
        let finished = session_rows(input("finished", true, None)).unwrap();
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
        assert_eq!(state, "stuck");
        assert_eq!(line, "stuck: its build failed");
        let mut quiet = session("quiet");
        quiet.waiting_on = Some(WaitingOn::Checks);
        assert_eq!(standing(&quiet).0, "waiting checks");
        // Each state is one a node entry keeps.
        for change in ["started", "asked", "finished", "failed", "quiet"] {
            let state = standing(&session(change)).0;
            assert_eq!(
                myriad_merope::happened::state_of(state).as_deref(),
                Some(state)
            );
        }
    }
}
