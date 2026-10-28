//! Which node a thing is: its id, made from the owner, the audience and
//! the node's key, and the node a moment a side told her files under.
//!
//! The id is the native app's (`AgentNode.id`): the first 16 bytes of
//! sha256 of `agent_node␟<owner>␟<key>` in hex, with the audience between
//! owner and key outside the owner's private one, and a together node's
//! own older formula. Whoever writes a node's first row, the server or a
//! computer, writes it under the same id.

use std::collections::BTreeMap;

use myriad_persona_sync::canonical::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{NodeError, body::Form, check_token};

/// A moment about another coding agent's session (her link's `moment`).
pub const PEER_AGENT: &str = "agent.merope.peer_agent";
/// A moment about the owner's conversation with another AI.
pub const AI_CHAT: &str = "agent.merope.ai_chat";
/// A child's own reading of what the owner is doing: no node.
pub const SITUATION: &str = "agent.merope.situation";

/// The key of the node of the owner's talks with other AIs.
pub const CHATS_KEY: &str = "together:chats";

/// The together topics (`TogetherTopic`): what was watched, what was read,
/// the talks with other AIs. Their nodes keep the ids they had as their own
/// rows.
const TOGETHER_TOPICS: [&str; 3] = ["watched", "reading", "chats"];

/// Where a thing files: the node's form, its key and the name she sees.
/// The title may be empty (a together node the reading side names in its
/// own language).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Place {
    pub form: Form,
    pub key: String,
    #[serde(default)]
    pub title: String,
}

/// What the native digest trims at both ends (`MemoryRules.isWhitespace`).
fn blank(ch: char) -> bool {
    matches!(
        u32::from(ch),
        0x09..=0x0D | 0x20 | 0x85 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000
    )
}

/// The native digest (`SemanticMemoryRules.digest`): the text trimmed,
/// sha256 of its UTF-8, the first 16 bytes in hex.
pub fn digest(text: &str) -> String {
    sha256_hex(text.trim_matches(blank).as_bytes())[..32].to_string()
}

/// The owner's private audience: `personal:<owner>`.
pub fn private(owner: &str) -> String {
    format!("personal:{owner}")
}

/// The topic a together key names, if it is one.
pub fn together_topic(key: &str) -> Option<&str> {
    key.strip_prefix("together:")
        .filter(|topic| TOGETHER_TOPICS.contains(topic))
}

pub(crate) fn check_owner(owner: &str) -> Result<(), NodeError> {
    check_token("owner", owner)?;
    if owner.contains(',') || owner.starts_with(blank) || owner.ends_with(blank) {
        return Err(NodeError::new(
            "owner holds a comma or starts or ends with a space",
        ));
    }
    Ok(())
}

pub(crate) fn check_key(key: &str) -> Result<(), NodeError> {
    check_token("key", key)?;
    // The digest trims: a key with a space at either end would share an id.
    if key.starts_with(blank) || key.ends_with(blank) {
        return Err(NodeError::new("key starts or ends with a space"));
    }
    Ok(())
}

/// The node of `key` in `audience`, of `owner`. In the owner's private
/// audience a key names one node; another audience never shares its id.
pub fn node_id(owner: &str, audience: &str, key: &str) -> Result<String, NodeError> {
    check_owner(owner)?;
    check_key(key)?;
    check_token("audience", audience)?;
    if audience != private(owner) {
        return Ok(digest(&format!(
            "agent_node\u{1f}{owner}\u{1f}{audience}\u{1f}{key}"
        )));
    }
    Ok(match together_topic(key) {
        Some(topic) => digest(&format!("together_node\u{1f}{owner}\u{1f}{topic}")),
        None => digest(&format!("agent_node\u{1f}{owner}\u{1f}{key}")),
    })
}

/// Whether `place` may be a node in `audience`: a together key only in the
/// owner's private audience, and only as a together node; a together node
/// only under a together key.
pub(crate) fn check_place(place: &Place, owner: &str, audience: &str) -> Result<(), NodeError> {
    check_key(&place.key)?;
    match (together_topic(&place.key), place.form) {
        (Some(_), Form::Together) if audience == private(owner) => Ok(()),
        (Some(_), Form::Together) => Err(NodeError::new(
            "a together node is only in the owner's private audience",
        )),
        (Some(_), _) => Err(NodeError::new("a together key is a together node")),
        (None, Form::Together) => Err(NodeError::new("a together node needs a together key")),
        (None, _) => Ok(()),
    }
}

/// Where a moment of `kind` with these facts files: another agent's session
/// under its repository's node (`safeFacts.repo`, trimmed), a talk with
/// another AI under the node of those talks, a situation under none (and a
/// peer moment without a repository under none). A kind her link does not
/// carry is refused.
pub fn place(
    kind: &str,
    safe_facts: &BTreeMap<String, String>,
) -> Result<Option<Place>, NodeError> {
    match kind {
        PEER_AGENT => {
            let repo = safe_facts
                .get("repo")
                .map(|repo| repo.trim_matches(blank))
                .unwrap_or("");
            if repo.is_empty() {
                return Ok(None);
            }
            check_key(repo)?;
            Ok(Some(Place {
                form: Form::Repo,
                key: repo.to_string(),
                title: repo.to_string(),
            }))
        }
        // The side that shows it names it in the owner's language.
        AI_CHAT => Ok(Some(Place {
            form: Form::Together,
            key: CHATS_KEY.to_string(),
            title: String::new(),
        })),
        SITUATION => Ok(None),
        _ => Err(NodeError::new("kind is not a moment her link carries")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_the_native_one() {
        // sha256("abc") starts ba7816bf…; the native digest trims first.
        assert_eq!(digest(" abc\u{3000}"), "ba7816bf8f01cfea414140de5dae2223");
        assert_eq!(digest("abc"), digest("\nabc "));
    }

    #[test]
    fn a_key_names_one_node_per_owner_and_audience() {
        let mine = node_id("7", "personal:7", "sample-repo").unwrap();
        assert_eq!(mine, digest("agent_node\u{1f}7\u{1f}sample-repo"));
        assert_ne!(mine, node_id("8", "personal:8", "sample-repo").unwrap());
        assert_ne!(mine, node_id("7", "personal:7,9", "sample-repo").unwrap());
        assert_eq!(
            node_id("7", "personal:7", CHATS_KEY).unwrap(),
            digest("together_node\u{1f}7\u{1f}chats")
        );
        assert!(node_id("7", "personal:7", "repo ").is_err());
        assert!(node_id("7,8", "personal:7,8", "repo").is_err());
    }
}
