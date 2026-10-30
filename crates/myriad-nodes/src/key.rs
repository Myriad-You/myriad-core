//! Which node a thing is: the three kinds, the stable keys, and the id.
//!
//! A node's id is the first 16 bytes, in hex, of sha256 of
//! `node␟<audience>␟<key>`. Nothing else goes in: not the kind (it can
//! change), not the device (two computers that see the same repository get
//! the same node), not a local path. The key is one of:
//!
//! | key | what | kind |
//! | --- | --- | --- |
//! | `repo:<host>/<path>` | a repository, by its remote ([`repo_key`]) | topic, always |
//! | `person:<id>` | a person: the partner (「关于你」 in their own private audience) or someone else | person, always |
//! | `group:<id>` | a group | person, always |
//! | `her` | her (「她自己」) in this audience | person, always |
//! | `minted:<token>` | a thing she made a node for; the writer mints the token once and it travels in the created row | task or topic, and may change |
//!
//! A minted token that starts `together-` is what she watched or read with
//! the partner (一起看过的): it only ever takes others in, and never goes
//! into another node itself ([`Key::is_together`]).
//!
//! A person or group lives in its own audience (the person's private talk,
//! the group), never in hers (`own:`): what she keeps of someone else stays
//! with them, so it never reaches the partner's computers, which hold her
//! own audience ([`Key::fits`]). Hers holds `her`, repositories and minted
//! things.

use std::borrow::Cow;

use myriad_persona_log::{partner, row::Audience};
use myriad_persona_sync::canonical::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{NodeError, check_token, limits::MINTED_BYTES};

/// What a node is. Kinds exist only where the rules differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// 进行中的事: it has a "done" (or, ongoing, it never ends but is
    /// always being done).
    Task,
    /// 话题: no "done", it keeps coming back; rests, is never folded away.
    Topic,
    /// 人: someone who speaks and has their own privacy, a group too.
    Person,
}

/// A key, read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key<'a> {
    Repo(&'a str),
    Person(&'a str),
    Group(&'a str),
    Her,
    Minted(&'a str),
}

impl<'a> Key<'a> {
    pub fn parse(key: &'a str) -> Result<Self, NodeError> {
        check_token("key", key)?;
        if key == "her" {
            return Ok(Self::Her);
        }
        let (space, rest) = key
            .split_once(':')
            .ok_or_else(|| NodeError::new("key is <space>:<value> or her"))?;
        match space {
            "repo" => {
                if repo_key(rest).ok().as_deref() != Some(key) {
                    return Err(NodeError::new("a repo key is a normalized remote"));
                }
                Ok(Self::Repo(rest))
            }
            "person" | "group" => {
                if rest.is_empty() || rest.contains(',') || rest.chars().any(char::is_whitespace) {
                    return Err(NodeError::new(
                        "a person or group id is not empty and holds no comma or space",
                    ));
                }
                Ok(if space == "person" {
                    Self::Person(rest)
                } else {
                    Self::Group(rest)
                })
            }
            "minted" => {
                if rest.is_empty()
                    || rest.len() > MINTED_BYTES
                    || !rest
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                {
                    return Err(NodeError::new(format!(
                        "a minted token is 1 to {MINTED_BYTES} of A-Z a-z 0-9 - _"
                    )));
                }
                Ok(Self::Minted(rest))
            }
            _ => Err(NodeError::new(
                "key space is not repo, person, group or minted",
            )),
        }
    }

    /// The kinds a node of this key may be. Only a minted key's kind changes.
    pub fn kinds(&self) -> &'static [Kind] {
        match self {
            Self::Repo(_) => &[Kind::Topic],
            Self::Person(_) | Self::Group(_) | Self::Her => &[Kind::Person],
            Self::Minted(_) => &[Kind::Task, Kind::Topic],
        }
    }

    /// Whether this is something she watched or read with the partner
    /// (`minted:together-…`): it takes others in, and is never merged away
    /// or folded back.
    pub fn is_together(&self) -> bool {
        matches!(self, Self::Minted(token) if token.starts_with("together-"))
    }

    /// Whether a node of this key may be in `audience`: a person or a group
    /// never in hers (`own:`).
    pub fn fits(&self, audience: &str) -> bool {
        !(matches!(self, Self::Person(_) | Self::Group(_))
            && matches!(Audience::parse(audience), Ok(Audience::Own)))
    }

    /// Whether this is the partner's own node in their private audience
    /// (「关于你」): what is only about them, with no experience hung on it.
    pub fn is_about_you(&self, audience: &str) -> bool {
        match (self, Audience::parse(audience)) {
            (Self::Person(person), Ok(Audience::Personal(members))) => members == *person,
            _ => false,
        }
    }
}

/// The node of `key` in `audience`.
pub fn node_id(audience: &str, key: &str) -> Result<String, NodeError> {
    check_token("audience", audience)?;
    Audience::parse(audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    if !Key::parse(key)?.fits(audience) {
        return Err(NodeError::new(
            "a person or group lives in its own audience, not hers",
        ));
    }
    Ok(made_id(audience, key))
}

/// The id `audience` and `key` make, unchecked: also a person's or a
/// group's in her own audience, which no side may make but one may meet.
///
/// The partner's private talk is made under one fixed name whatever id the
/// partner goes by (`local-owner`, or any `partner_…`;
/// `myriad_persona_log::partner`), and so is their own node in it
/// (「关于你」, `person:<that id>`): a node keeps its id when the partner's
/// id moves, on every side, whatever bodies it holds. The fixed name is the
/// one every Mac made its nodes under, so their ids stay as they were.
pub(crate) fn made_id(audience: &str, key: &str) -> String {
    let (audience, key) = fixed(audience, key);
    sha256_hex(format!("node\u{1f}{audience}\u{1f}{key}").as_bytes())[..32].to_string()
}

/// `audience` and `key` as a node id is made from them: the partner's
/// private talk, and their own node in it, under the fixed name.
fn fixed<'a>(audience: &'a str, key: &'a str) -> (Cow<'a, str>, Cow<'a, str>) {
    let Ok(Audience::Personal(member)) = Audience::parse(audience) else {
        return (Cow::Borrowed(audience), Cow::Borrowed(key));
    };
    if !partner::is_partners(member) {
        return (Cow::Borrowed(audience), Cow::Borrowed(key));
    }
    let key = match key.strip_prefix("person:") {
        Some(person) if person == member => Cow::Owned(format!("person:{}", partner::LEGACY)),
        _ => Cow::Borrowed(key),
    };
    (Cow::Owned(format!("personal:{}", partner::LEGACY)), key)
}

/// The repository key of a remote: `repo:<host>/<path>`, lowercase, with
/// no scheme, user, password, port, query, `.git` or trailing slash.
/// `git@github.com:Me/Repo.git`, `https://token@github.com/me/repo/` and
/// `ssh://git@github.com:22/me/repo` are all `repo:github.com/me/repo`. A
/// local path or a `file:` remote is refused: a key is the same on every
/// device, and a path is not.
pub fn repo_key(remote: &str) -> Result<String, NodeError> {
    let remote = remote.trim();
    if remote.is_empty() || remote.len() > myriad_persona_log::row::MAX_TOKEN_BYTES {
        return Err(NodeError::new("remote must be 1 to 512 bytes"));
    }
    if remote
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(NodeError::new("remote holds a space or control character"));
    }
    let local = remote.starts_with(['/', '~', '.', '\\'])
        || remote.as_bytes().get(1).is_some_and(|second| {
            *second == b':'
                && remote.as_bytes()[0].is_ascii_alphabetic()
                && remote
                    .as_bytes()
                    .get(2)
                    .is_none_or(|third| *third == b'\\' || *third == b'/')
        });
    if local {
        return Err(NodeError::new("a local path is not a remote"));
    }
    let (authority, path) = if let Some((scheme, rest)) = remote.split_once("://") {
        match scheme.to_ascii_lowercase().as_str() {
            "https" | "http" | "ssh" | "git" | "git+ssh" | "ssh+git" => {}
            "file" => return Err(NodeError::new("a local path is not a remote")),
            _ => {
                return Err(NodeError::new(
                    "remote scheme is not https, http, ssh or git",
                ));
            }
        }
        rest.split_once('/')
            .ok_or_else(|| NodeError::new("remote has no path"))?
    } else {
        // `user@host:path` (scp-like), or `host/path` (already normal).
        match (remote.find(':'), remote.find('/')) {
            (Some(colon), slash) if slash.is_none_or(|slash| colon < slash) => {
                (&remote[..colon], &remote[colon + 1..])
            }
            _ => remote
                .split_once('/')
                .ok_or_else(|| NodeError::new("remote has no path"))?,
        }
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split_once(':').map_or(host, |(host, _)| host);
    let host = host.to_ascii_lowercase();
    if host.is_empty()
        || host.starts_with(['.', '-'])
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(NodeError::new("remote host is not a host name"));
    }
    // Lowercase first, then strip to a fixed point, so a key made from a
    // key is the same key (`Repo.GIT`, `repo.git.git/`: `repo`).
    let path = path.split(['?', '#']).next().unwrap_or("").to_lowercase();
    let mut path = path.trim_matches('/');
    while let Some(bare) = path.strip_suffix(".git") {
        path = bare.trim_end_matches('/');
    }
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| *segment == "." || *segment == "..")
    {
        return Err(NodeError::new("remote path is empty or climbs"));
    }
    Ok(format!("repo:{host}/{}", segments.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_repository_one_key_from_every_remote() {
        for remote in [
            "git@github.com:Me/Repo.git",
            "https://github.com/me/repo",
            "https://token@GitHub.com/me/repo.git/",
            "ssh://git@github.com:22/me/repo",
            "github.com/me/repo",
            "  https://github.com/me/repo?tab=readme#top ",
        ] {
            assert_eq!(
                repo_key(remote).unwrap(),
                "repo:github.com/me/repo",
                "{remote}"
            );
        }
        for local in [
            "/Users/demo/repo",
            "~/repo",
            "./repo",
            "C:\\repo",
            "file:///Users/demo/repo",
            "",
            "github.com",
            "https://github.com/../x",
        ] {
            assert!(repo_key(local).is_err(), "{local}");
        }
    }

    #[test]
    fn a_key_made_from_a_key_is_the_same_key() {
        for remote in [
            "https://github.com/demo/Example.GIT",
            "https://github.com/demo/repo.git.git/",
            "https://github.com/demo/repo.git/.git",
            "git@GitHub.com:Me/Ünïcode-Repo.Git",
            "ssh://git@host.example:2222/a//b/c/",
            "https://host/a@b/c:d",
            "repo:github.com/x",
            "https://github.com/demo/repo?x=1#.git",
            "http://h/a\\b",
            "https://.hidden/x",
            "https://-host/x",
            "https://github.com/.git",
        ] {
            let Ok(key) = repo_key(remote) else {
                continue;
            };
            let rest = key.strip_prefix("repo:").unwrap();
            assert_eq!(repo_key(rest).unwrap(), key, "{remote}");
            assert!(node_id("personal:7", &key).is_ok(), "{remote}: {key}");
        }
        assert_eq!(
            repo_key("https://github.com/demo/Example.GIT").unwrap(),
            "repo:github.com/demo/example"
        );
        assert!(repo_key("https://.hidden/x").is_err());
    }

    #[test]
    fn the_id_is_the_audience_and_the_key_only() {
        let id = node_id("personal:7", "repo:github.com/me/repo").unwrap();
        assert_eq!(
            id,
            sha256_hex("node\u{1f}personal:7\u{1f}repo:github.com/me/repo".as_bytes())[..32]
        );
        assert_ne!(id, node_id("group:g1", "repo:github.com/me/repo").unwrap());
        assert!(node_id("personal:7", "repo:github.com/Me/repo").is_err());
        assert!(node_id("personal:7", "minted:a b").is_err());
        assert!(node_id("nowhere", "her").is_err());
        assert!(node_id("own:", "her").is_ok());
        assert!(node_id("own:", "minted:a").is_ok());
        assert!(node_id("own:", "person:7").is_err());
        assert!(node_id("own:", "group:g1").is_err());
        assert!(node_id("personal:7", "person:9").is_ok());
        assert!(Key::parse("person:7").unwrap().is_about_you("personal:7"));
        assert!(!Key::parse("person:9").unwrap().is_about_you("personal:7"));
        assert!(!Key::parse("person:7").unwrap().is_about_you("personal:7,9"));
        assert!(Key::parse("minted:together-ab12").unwrap().is_together());
        assert!(!Key::parse("minted:ab12-together-").unwrap().is_together());
        assert!(!Key::parse("person:together-ab12").unwrap().is_together());
    }
}
