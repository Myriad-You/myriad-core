//! The `now` family's bodies, read by the contract's `sessions` rules: what
//! a device may say of one of the owner's coding-agent sessions when it
//! changes, and which grant a drop ends. The envelope is [`crate::check`]'s;
//! this reads what is inside, so both ends refuse the same bodies.
//!
//! Pure: nothing here reads a clock, a file or the network.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CONTRACT_JSON, Refusal, refuse};

/// The contract's `sessions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    pub changes: Vec<String>,
    pub waiting_on: Vec<String>,
    pub asked_by: Vec<String>,
    pub check_kinds: Vec<String>,
    pub check_states: Vec<String>,
    pub remote_bytes: usize,
    pub folder_chars: usize,
    pub title_chars: usize,
    pub words_chars: usize,
    pub path_bytes: usize,
    pub touched_at_most: usize,
    pub checks_at_most: usize,
    pub keep_ms: u64,
    pub quiet_after_ms: u64,
    pub sig_pattern: String,
    pub errors_at_most: usize,
    pub overwrote_at_most: usize,
    pub error_span_ms: i64,
    pub repeated_error_at: u32,
    pub look: Look,
}

/// The contract's `sessions.look`: how her host looks at what it is told.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub verdicts: Vec<String>,
    pub late_after_ms: i64,
    pub quiet_ms: i64,
    pub at_most_ms: i64,
}

#[derive(Deserialize)]
struct WithSessions {
    sessions: Rules,
}

static RULES: LazyLock<Rules> = LazyLock::new(|| {
    serde_json::from_str::<WithSessions>(CONTRACT_JSON)
        .expect("her_link_contract.json sessions")
        .sessions
});

/// The contract's `sessions` rules.
pub fn rules() -> &'static Rules {
    &RULES
}

/// What changed about a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Started,
    Asked,
    Finished,
    Failed,
    Quiet,
}

/// Who a session's next step waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitingOn {
    Owner,
    /// The session that sent it on, to report back to.
    Session,
    Checks,
    Program,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Test,
    Build,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Running,
    Passed,
    Failed,
    /// It ended and whether it passed is not known, or it was stopped.
    Ended,
}

/// Who gave a session its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskedBy {
    Owner,
    /// Another of the owner's sessions sent it on.
    Session,
    None,
}

/// One test or build of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Check {
    pub kind: CheckKind,
    pub state: CheckState,
    pub at: i64,
}

/// Failures of one kind in a session, as the device groups them (`sessions`
/// says how `sig` is made: no one can read the failure back from it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failures {
    pub sig: String,
    pub count: u32,
    pub first_at: i64,
    pub last_at: i64,
}

/// Lines another session on the device had just written, that this one
/// lately changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Overwrote {
    /// The other session's id on the device.
    pub session: String,
    pub files: u32,
    /// It put back what the other had replaced, not only rewrote it.
    pub undid: bool,
    pub at: i64,
}

/// One `now.session` body, read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub grant: String,
    pub session: String,
    pub change: Change,
    pub occurred_at: i64,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_by: Option<AskedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_by_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_peer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<String>,
    /// When the owner last spoke to the session themselves. This, `errors`
    /// and `overwrote` come only from a device whose welcome listed
    /// `now.sessions2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_said_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<Failures>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overwrote: Vec<Overwrote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touched: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<WaitingOn>,
}

/// One `now.drop` body, read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drop {
    pub grant: String,
}

fn bad(detail: impl Into<String>) -> Refusal {
    refuse("bad_frame", detail)
}

/// `sessions.grantPattern`: `^[A-Za-z0-9_-]{8,64}$`.
pub fn valid_grant(grant: &str) -> bool {
    (8..=64).contains(&grant.len())
        && grant
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// `sessions.sessionPattern`: `^[A-Za-z0-9_.:-]{1,128}$`.
pub fn valid_session(session: &str) -> bool {
    (1..=128).contains(&session.len())
        && session
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
}

/// `sessions.toolPattern`: `^[a-z0-9][a-z0-9_.-]{0,63}$`.
fn valid_tool(tool: &str) -> bool {
    (1..=64).contains(&tool.len())
        && tool
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && tool.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}

/// A remote as repoKey writes it, without `repo:` (at most
/// `sessions.remoteBytes`, so the key fits a token): `host/path`, lowercase,
/// no scheme, no user or password (`@`), no port (`:`), no `.git`, no
/// trailing or doubled slash, no space. Whether it is exactly repoKey's
/// form is the nodes' to check; this keeps anything that could carry a
/// credential or a local path off the link.
pub fn valid_remote(remote: &str) -> bool {
    let rules = rules();
    if remote.is_empty() || remote.len() > rules.remote_bytes {
        return false;
    }
    let Some((host, path)) = remote.split_once('/') else {
        return false;
    };
    // A host name: dot-separated labels of a-z 0-9 and inner hyphens.
    let host_name = host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    });
    host_name
        && !path.is_empty()
        && !remote.ends_with('/')
        && !remote.ends_with(".git")
        && !remote.contains("//")
        && remote.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._~/-".contains(&byte)
        })
        && !remote.split('/').any(|part| part == "." || part == "..")
}

/// `sessions.sigPattern`: 16 lowercase hex digits.
pub fn valid_sig(sig: &str) -> bool {
    sig.len() == 16
        && sig
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A path relative to the repository's root: not empty, not absolute (no
/// leading `/`, `\`, `~` or drive), no `.` or `..` part, no control.
pub fn valid_relative_path(path: &str) -> bool {
    let rules = rules();
    if path.is_empty() || path.len() > rules.path_bytes {
        return false;
    }
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    !path.starts_with(['/', '\\', '~'])
        && !drive
        && !path.chars().any(char::is_control)
        && !path
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn words(field: &str, text: &Option<String>, most: usize) -> Result<(), Refusal> {
    match text {
        Some(text) if text.trim().is_empty() => Err(bad(format!("{field} is empty"))),
        Some(text) if text.chars().count() > most => {
            Err(bad(format!("{field} is over {most} characters")))
        }
        Some(text)
            if text
                .chars()
                .any(|ch| ch.is_control() && ch != '\n' && ch != '\t') =>
        {
            Err(bad(format!("{field} holds a control character")))
        }
        _ => Ok(()),
    }
}

/// A `now.session` body by the contract's `sessions` rules.
pub fn read_session(body: &Value) -> Result<Session, Refusal> {
    let session: Session = serde_json::from_value(body.clone())
        .map_err(|error| bad(format!("now.session: {error}")))?;
    let rules = rules();
    if !valid_grant(&session.grant) {
        return Err(bad("grant does not match sessions.grantPattern"));
    }
    if !valid_session(&session.session) {
        return Err(bad("session does not match sessions.sessionPattern"));
    }
    if session.occurred_at <= 0 {
        return Err(bad("occurredAt is not a time"));
    }
    if !valid_tool(&session.tool) {
        return Err(bad("tool does not match sessions.toolPattern"));
    }
    if session
        .remote
        .as_deref()
        .is_some_and(|remote| !valid_remote(remote))
    {
        return Err(bad("remote is not a normalized remote"));
    }
    if session.remote.is_none() && session.folder.is_none() {
        return Err(bad("a session with no remote names its folder"));
    }
    if let Some(folder) = &session.folder
        && (folder.is_empty()
            || folder.trim() != folder
            || folder.chars().count() > rules.folder_chars
            || folder.contains(['/', '\\'])
            || folder == "."
            || folder == ".."
            || folder.chars().any(char::is_control))
    {
        return Err(bad("folder is not one folder's name"));
    }
    words("title", &session.title, rules.title_chars)?;
    words("askedByTitle", &session.asked_by_title, rules.title_chars)?;
    if session.asked_by_title.is_some() && session.asked_by != Some(AskedBy::Session) {
        return Err(bad("askedByTitle goes only with askedBy session"));
    }
    words("fromOwner", &session.from_owner, rules.words_chars)?;
    words("fromPeer", &session.from_peer, rules.words_chars)?;
    words("fromAgent", &session.from_agent, rules.words_chars)?;
    if session.owner_said_at.is_some_and(|at| at <= 0) {
        return Err(bad("ownerSaidAt is not a time"));
    }
    if session.errors.len() > rules.errors_at_most {
        return Err(refuse(
            "too_many",
            format!("errors holds more than {}", rules.errors_at_most),
        ));
    }
    for failures in &session.errors {
        if !valid_sig(&failures.sig) {
            return Err(bad("an error's sig does not match sessions.sigPattern"));
        }
        if failures.count == 0 || failures.first_at <= 0 || failures.last_at < failures.first_at {
            return Err(bad("an error is not counted between two times"));
        }
        if failures.last_at - failures.first_at > rules.error_span_ms {
            return Err(bad("an error spans more than sessions.errorSpanMs"));
        }
    }
    let mut sigs: Vec<&str> = session
        .errors
        .iter()
        .map(|failures| failures.sig.as_str())
        .collect();
    sigs.sort_unstable();
    if sigs.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(bad("errors holds one sig twice"));
    }
    if session.overwrote.len() > rules.overwrote_at_most {
        return Err(refuse(
            "too_many",
            format!("overwrote holds more than {}", rules.overwrote_at_most),
        ));
    }
    for overwrote in &session.overwrote {
        if !valid_session(&overwrote.session) || overwrote.session == session.session {
            return Err(bad("overwrote names no other session"));
        }
        if overwrote.files == 0 || overwrote.at <= 0 {
            return Err(bad("overwrote counts no file, or no time"));
        }
    }
    if session.touched.len() > rules.touched_at_most {
        return Err(refuse(
            "too_many",
            format!("touched holds more than {}", rules.touched_at_most),
        ));
    }
    if !session.touched.iter().all(|path| valid_relative_path(path)) {
        return Err(bad("touched holds a path that is not relative"));
    }
    if session.checks.len() > rules.checks_at_most {
        return Err(refuse(
            "too_many",
            format!("checks holds more than {}", rules.checks_at_most),
        ));
    }
    if session.checks.iter().any(|check| check.at <= 0) {
        return Err(bad("a check's at is not a time"));
    }
    if session.change == Change::Asked
        && session
            .waiting_on
            .is_some_and(|waiting| waiting != WaitingOn::Owner)
    {
        return Err(bad("an asked session waits on the owner"));
    }
    if session.change == Change::Finished && session.waiting_on == Some(WaitingOn::Owner) {
        return Err(bad("a finished session waits on nobody's answer"));
    }
    Ok(session)
}

/// A `now.session` body as a device told it on a link: with `sessions2`
/// false (the host's welcome did not list `now.sessions2`), one that holds
/// `ownerSaidAt`, `errors` or `overwrote` is refused.
pub fn read_told(body: &Value, sessions2: bool) -> Result<Session, Refusal> {
    let session = read_session(body)?;
    if !sessions2 && session.needs_sessions2() {
        return Err(bad("ownerSaidAt, errors and overwrote need now.sessions2"));
    }
    Ok(session)
}

impl Session {
    /// It holds what only a host that listed `now.sessions2` takes.
    pub fn needs_sessions2(&self) -> bool {
        self.owner_said_at.is_some() || !self.errors.is_empty() || !self.overwrote.is_empty()
    }
}

/// A `now.drop` body.
pub fn read_drop(body: &Value) -> Result<Drop, Refusal> {
    let drop: Drop =
        serde_json::from_value(body.clone()).map_err(|error| bad(format!("now.drop: {error}")))?;
    if !valid_grant(&drop.grant) {
        return Err(bad("grant does not match sessions.grantPattern"));
    }
    Ok(drop)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body() -> Value {
        json!({
            "grant": "g_0123456789",
            "session": "0199d5c0-9000-7333-8333-333333333333",
            "change": "asked",
            "occurredAt": 1_760_140_881_000_i64,
            "tool": "coding-agent",
            "remote": "github.com/me/repo",
            "folder": "repo",
            "title": "split the store",
            "askedBy": "session",
            "askedByTitle": "plan the release",
            "fromOwner": "commit the split",
            "fromPeer": "split the store, then report back",
            "fromAgent": "Committed locally. Push now? (yes / later)",
            "ownerSaidAt": 1_760_140_000_000_i64,
            "errors": [{"sig": "0123456789abcdef", "count": 3, "firstAt": 1_760_140_000_000_i64, "lastAt": 1_760_140_800_000_i64}],
            "overwrote": [{"session": "s-other", "files": 2, "undid": true, "at": 1_760_140_700_000_i64}],
            "touched": ["src/store.rs", "src/lib.rs"],
            "checks": [{"kind": "test", "state": "passed", "at": 1_760_140_880_000_i64}],
            "waitingOn": "owner"
        })
    }

    #[test]
    fn the_contract_names_what_the_reader_reads() {
        let rules = rules();
        let names = |values: &[&str]| values.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert_eq!(
            rules.changes,
            names(&["started", "asked", "finished", "failed", "quiet"])
        );
        for change in &rules.changes {
            serde_json::from_value::<Change>(json!(change)).unwrap();
        }
        for waiting in &rules.waiting_on {
            serde_json::from_value::<WaitingOn>(json!(waiting)).unwrap();
        }
        for asked_by in &rules.asked_by {
            serde_json::from_value::<AskedBy>(json!(asked_by)).unwrap();
        }
        assert_eq!(rules.quiet_after_ms, 20 * 60 * 1000);
        assert_eq!(rules.look.verdicts, names(&["no_need", "nod", "decide"]));
        assert_eq!(rules.look.late_after_ms, 10 * 60 * 1000);
        assert_eq!(
            (rules.look.quiet_ms, rules.look.at_most_ms),
            (20_000, 60_000)
        );
        for kind in &rules.check_kinds {
            serde_json::from_value::<CheckKind>(json!(kind)).unwrap();
        }
        for state in &rules.check_states {
            serde_json::from_value::<CheckState>(json!(state)).unwrap();
        }
        let contract: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
        let frame = &contract["frames"]["now.session"];
        assert_eq!(frame["maxItems"]["touched"], json!(rules.touched_at_most));
        assert_eq!(frame["maxItems"]["checks"], json!(rules.checks_at_most));
        assert_eq!(frame["maxItems"]["errors"], json!(rules.errors_at_most));
        assert_eq!(
            frame["maxItems"]["overwrote"],
            json!(rules.overwrote_at_most)
        );
        assert_eq!(rules.sig_pattern, "^[0-9a-f]{16}$");
        assert!(rules.repeated_error_at >= 2 && rules.error_span_ms > 0);
        let mut fields: Vec<String> = frame["required"]
            .as_array()
            .unwrap()
            .iter()
            .chain(frame["optional"].as_array().unwrap())
            .map(|field| field.as_str().unwrap().to_string())
            .collect();
        fields.sort();
        let mut read: Vec<String> = serde_json::to_value(read_session(&body()).unwrap())
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        read.sort();
        assert_eq!(fields, read);
    }

    #[test]
    fn a_session_is_read_by_the_rules() {
        let session = read_session(&body()).unwrap();
        assert_eq!(session.change, Change::Asked);
        assert_eq!(session.touched.len(), 2);
        let minimal = json!({
            "grant": "g_0123456789", "session": "s1", "change": "started",
            "occurredAt": 1, "tool": "codex", "remote": "gitlab.com/a/b/c"
        });
        assert!(read_session(&minimal).is_ok());
    }

    #[test]
    fn what_could_carry_a_secret_or_a_path_is_refused() {
        for (field, value) in [
            ("remote", json!("https://github.com/me/repo")),
            ("remote", json!("token@github.com/me/repo")),
            ("remote", json!("github.com:22/me/repo")),
            ("remote", json!("github.com/me/repo.git")),
            ("remote", json!("/Users/me/repo")),
            ("remote", json!("github.com")),
            ("remote", json!("GitHub.com/me/repo")),
            ("remote", json!("-x.com/me/repo")),
            ("remote", json!("a~b.com/me/repo")),
            ("remote", json!(format!("github.com/{}", "a".repeat(500)))),
            ("touched", json!(["/etc/passwd"])),
            ("touched", json!(["~/notes.md"])),
            ("touched", json!(["C:/x.rs"])),
            ("touched", json!(["src/../../x"])),
            ("folder", json!("me/repo")),
            ("folder", json!("notes ")),
            ("folder", json!("   ")),
            ("grant", json!("short")),
            ("session", json!("has space")),
            ("tool", json!("Tool")),
            ("change", json!("waiting")),
            ("waitingOn", json!("checks")),
            ("fromAgent", json!(" ")),
            ("fromAgent", json!("x".repeat(2001))),
            ("title", json!("x".repeat(201))),
            ("output", json!("ok")),
            ("askedBy", json!("someone")),
            ("fromPeer", json!(" ")),
            ("ownerSaidAt", json!(0)),
            ("ownerSaidAt", json!("soon")),
            (
                "errors",
                json!([{"sig": "0123456789ABCDEF", "count": 1, "firstAt": 1, "lastAt": 1}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcde", "count": 1, "firstAt": 1, "lastAt": 1}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcdef", "count": 0, "firstAt": 1, "lastAt": 1}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcdef", "count": 1, "firstAt": 2, "lastAt": 1}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcdef", "count": 1, "firstAt": 1, "lastAt": 1, "text": "x"}]),
            ),
            (
                "overwrote",
                json!([{"session": "s/x", "files": 1, "undid": false, "at": 1}]),
            ),
            (
                "overwrote",
                json!([{"session": "s-other", "files": 0, "undid": false, "at": 1}]),
            ),
            (
                "overwrote",
                json!([{"session": "s-other", "files": 1, "undid": false, "at": 0}]),
            ),
            (
                "overwrote",
                json!([{"session": "0199d5c0-9000-7333-8333-333333333333", "files": 1, "undid": false, "at": 1}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcdef", "count": 1, "firstAt": 1, "lastAt": 3_600_002}]),
            ),
            (
                "errors",
                json!([{"sig": "0123456789abcdef", "count": 1, "firstAt": 1, "lastAt": 1},
                       {"sig": "0123456789abcdef", "count": 2, "firstAt": 1, "lastAt": 2}]),
            ),
        ] {
            let mut bad = body();
            bad[field] = value.clone();
            let refused = read_session(&bad).unwrap_err();
            assert_eq!(refused.code, "bad_frame", "{field} {value}");
        }
        // A title of who sent it on goes only with askedBy session.
        let mut owner = body();
        owner["askedBy"] = json!("owner");
        assert_eq!(read_session(&owner).unwrap_err().code, "bad_frame");
        // No remote: the folder names it; neither is refused.
        let mut local = body();
        local.as_object_mut().unwrap().remove("remote");
        assert!(read_session(&local).is_ok());
        local.as_object_mut().unwrap().remove("folder");
        assert_eq!(read_session(&local).unwrap_err().code, "bad_frame");
        let mut ended = body();
        ended["checks"] = json!([{"kind": "test", "state": "ended", "at": 1}]);
        ended["change"] = json!("quiet");
        ended["waitingOn"] = json!("session");
        assert!(read_session(&ended).is_ok());
        let mut finished = body();
        finished["change"] = json!("finished");
        assert_eq!(read_session(&finished).unwrap_err().code, "bad_frame");
        let mut many = body();
        many["touched"] = json!(vec!["a.rs"; 65]);
        assert_eq!(read_session(&many).unwrap_err().code, "too_many");
        let mut many = body();
        many["errors"] = json!(
            (0..17_u64)
                .map(
                    |at| json!({"sig": format!("{at:016x}"), "count": 1, "firstAt": 1, "lastAt": 1})
                )
                .collect::<Vec<_>>()
        );
        assert_eq!(read_session(&many).unwrap_err().code, "too_many");
        let mut many = body();
        many["overwrote"] = json!(
            (0..17)
                .map(
                    |at| json!({"session": format!("s-{at}"), "files": 1, "undid": false, "at": 1})
                )
                .collect::<Vec<_>>()
        );
        assert_eq!(read_session(&many).unwrap_err().code, "too_many");
        // Only to a host that listed now.sessions2.
        assert!(read_told(&body(), true).is_ok());
        assert_eq!(read_told(&body(), false).unwrap_err().code, "bad_frame");
        let mut plain = body();
        for field in ["ownerSaidAt", "errors", "overwrote"] {
            plain.as_object_mut().unwrap().remove(field);
        }
        assert!(read_told(&plain, false).is_ok());
        // A quiet session may wait on anything; 2000 characters are kept.
        let mut quiet = body();
        quiet["change"] = json!("quiet");
        quiet["waitingOn"] = json!("checks");
        quiet["fromAgent"] = json!("字".repeat(2000));
        assert!(read_session(&quiet).is_ok());
    }

    /// The contract's examples read by these rules.
    #[test]
    fn the_examples_read() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../../../shared/her_link_fixtures.json")).unwrap();
        let frames = fixtures["frames"].as_array().unwrap();
        let body = |t: &str| frames.iter().find(|frame| frame["t"] == t).unwrap()["body"].clone();
        assert_eq!(
            read_session(&body("now.session")).unwrap().change,
            Change::Asked
        );
        read_drop(&body("now.drop")).unwrap();
    }

    #[test]
    fn a_drop_names_a_grant() {
        assert!(read_drop(&json!({"grant": "g_0123456789"})).is_ok());
        assert!(read_drop(&json!({"grant": "g"})).is_err());
        assert!(read_drop(&json!({"grant": "g_0123456789", "all": true})).is_err());
    }
}
