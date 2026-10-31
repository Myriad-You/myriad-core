//! One id for the partner on every side.
//!
//! The partner's private audience is `personal:<id>`. Until now the Mac
//! said `local-owner` and the site its own account id. From here every side
//! uses one id, which the site issues (the partner's decision, 2026-10-30:
//! one new id on both ends):
//!
//! - **Its form** ([`check`]): `partner_` and 32 lowercase hex digits: 12
//!   of the time it was minted (Unix milliseconds) then 20 random (80
//!   bits). It is an id, not a credential: it opens nothing, and it may
//!   stand in a file's clear header.
//! - **Who mints it** ([`mint`]): the site, once per account (not the
//!   account id itself). The caller gives the time and ten random bytes;
//!   the core reads neither. A Mac never mints one.
//! - **How a Mac gets it** ([`link`]): only over the authenticated link to
//!   the partner's account (the site's welcome). Before its first sync on
//!   that link a Mac still on `local-owner` moves what it held to it: rows,
//!   bodies and its ledger (`myriad_nodes::migrate`). A move is only ever
//!   from `local-owner`. A Mac that goes by another id, linked or taken
//!   from a file, is refused: what it holds may be another account's
//!   partner's (a file of theirs taken onto an empty Mac), and no link moves
//!   it. The host offers to start that Mac empty before linking. A Mac
//!   that took a file from before (no partner in its header) while empty
//!   holds talk that may be someone else's under `local-owner` too: its
//!   link asks first ([`Linked::confirm`]; the coordinator's decision,
//!   2026-10-31). A move cut off half way is taken up again: a side that
//!   still holds `personal:local-owner`, or says it is moving
//!   ([`Here::moving`]: set before the first write of a move, cleared after
//!   the last), moves from it, whatever id it goes by already. A side that
//!   holds the talk of another partner id than the link's is refused: it
//!   would go by two ids. A Mac that never linked stays `local-owner`.
//! - **Files** ([`file`]) never change or add an id. A file is taken when
//!   the partner id in its header is this side's. A side with no id yet
//!   (still `local-owner`) that holds no row of a `personal:` audience or of
//!   `own:` and never linked may take the file's id as its own (the
//!   partner's decision, 2026-10-30: 空的 Mac 直接认文件); a file with no
//!   partner in its header (its pages under `local-owner`) only onto such a
//!   side, as it is. Anything else is refused: a side that took an id never
//!   takes another file's, and a side with an id never takes pages under
//!   `local-owner`, nor a side that holds another partner id's talk the
//!   file of its own id. So a Mac that holds anything and never linked
//!   cannot take another unlinked Mac's file: link both, or import onto an
//!   empty Mac. The header's id is in the clear: it guards against a mix-up (the
//!   wrong file, the wrong Mac), not against anyone; it authenticates
//!   nothing.
//! - **Nobody else's partner talk** (`crate::Holder::holds`): no side, the
//!   server included, holds a private audience of `local-owner` or of a
//!   partner id but its own partner's.
//! - **Moving** ([`row`], [`entry`]): `personal:<from>` becomes
//!   `personal:<to>`, a subject that was `from` becomes `to`, and so does an
//!   object or a ledger target about them (`subject:<from>`,
//!   `affect:<from>`), in the partner's talk and in her own. Ids, sides,
//!   counts and clocks stay, so a row is the same row on every side. Node
//!   ids do not move: the partner's private talk makes them under one fixed
//!   name, [`LEGACY`] (`myriad_nodes::key`).

use serde::{Deserialize, Serialize};

use crate::{LogError, Row, forget::Entry, row::Audience};

/// What every partner id starts with.
pub const PREFIX: &str = "partner_";
/// Hex digits of the mint time.
const TIME_DIGITS: usize = 12;
/// Random bytes the caller gives a mint.
pub const RANDOM_BYTES: usize = 10;
/// The latest mint time the form holds: 16^12 - 1 milliseconds.
const MAX_MS: i64 = (1 << 48) - 1;
/// The earliest mint time: 2025-01-01, before any side minted one.
const MIN_MS: i64 = 1_735_689_600_000;
/// The id every Mac's partner went by before the partner's one id, and the
/// fixed name node ids of the partner's private talk are made under.
pub const LEGACY: &str = "local-owner";

/// Whether the one member of a private audience is the partner, under an id
/// a partner may go by: a partner id or [`LEGACY`]. Another id (another
/// person, a site's account id) is not.
pub fn is_partners(member: &str) -> bool {
    member == LEGACY || check(member).is_ok()
}

/// Whether `id` is a partner id in the form this core mints.
pub fn check(id: &str) -> Result<(), LogError> {
    let digits = id.strip_prefix(PREFIX).unwrap_or("");
    let well = digits.len() == TIME_DIGITS + 2 * RANDOM_BYTES
        && digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !well {
        return Err(LogError::new(
            "a partner id is partner_ and 32 lowercase hex digits",
        ));
    }
    let at = i64::from_str_radix(&digits[..TIME_DIGITS], 16).unwrap_or(0);
    if at < MIN_MS {
        return Err(LogError::new("a partner id is minted no earlier than 2025"));
    }
    Ok(())
}

/// A new partner id: minted at `at_ms` (Unix milliseconds) with the
/// caller's random bytes.
pub fn mint(at_ms: i64, random: &[u8]) -> Result<String, LogError> {
    if !(MIN_MS..=MAX_MS).contains(&at_ms) {
        return Err(LogError::new(
            "the mint time is Unix milliseconds the id can hold",
        ));
    }
    if random.len() != RANDOM_BYTES {
        return Err(LogError::new(format!(
            "a mint takes {RANDOM_BYTES} random bytes"
        )));
    }
    let mut id = format!("{PREFIX}{at_ms:012x}");
    for byte in random {
        id.push_str(&format!("{byte:02x}"));
    }
    Ok(id)
}

/// What a side knows of its partner, for [`link`] and [`file`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Here {
    /// The partner id this side goes by; none while it is `local-owner`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Whether the id came over a link to the partner's account (the site
    /// is always linked).
    #[serde(default, skip_serializing_if = "is_false")]
    pub linked: bool,
    /// Every audience this side holds a row of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audiences: Vec<String>,
    /// This side took a file from before (no partner in its header) while
    /// it was empty ([`Filed::filed`]): what it holds under `local-owner`
    /// may be another account's partner's. Kept until its move from
    /// `local-owner` has finished, or the side is started empty.
    #[serde(default, skip_serializing_if = "is_false")]
    pub filed: bool,
    /// A move from `local-owner` is under way: set before its first write,
    /// cleared after its last. While it is set, [`link`] moves again, so a
    /// move cut off after the partner's talk but before her own rows or the
    /// ledger is finished.
    #[serde(default, skip_serializing_if = "is_false")]
    pub moving: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Here {
    fn check(&self) -> Result<(), LogError> {
        if let Some(id) = &self.id {
            check(id)?;
        }
        if self.linked && self.id.is_none() {
            return Err(LogError::new("a linked side has a partner id"));
        }
        Ok(())
    }

    /// Whether this side still holds rows of the partner's talk under
    /// `local-owner`: it has not moved, or a move was cut off.
    fn unmoved(&self) -> bool {
        self.audiences
            .iter()
            .any(|audience| audience.strip_prefix("personal:") == Some(LEGACY))
    }

    /// Whether this side holds the private talk of a partner id other than
    /// `id`: it would go by two ids.
    fn holds_another(&self, id: &str) -> bool {
        self.audiences.iter().any(|audience| {
            audience
                .strip_prefix("personal:")
                .is_some_and(|member| member != id && check(member).is_ok())
        })
    }

    /// Whether this side holds nothing of the partner's or hers: no row of
    /// a `personal:` audience or of `own:`.
    fn empty(&self) -> bool {
        !self.audiences.iter().any(|audience| {
            matches!(
                Audience::parse(audience),
                Ok(Audience::Personal(_) | Audience::Own)
            )
        })
    }
}

/// What a link's partner id means for this side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    /// The id this side goes by from now on.
    pub id: String,
    /// The id to move from first (`myriad_nodes::migrate`): `local-owner`,
    /// while this side has not moved or holds rows of it still.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Ask the partner before moving: this side took a file from before
    /// while empty, so what it holds under `local-owner` may be another
    /// account's partner's. On yes, move as `from` says; on no, start this
    /// side empty and link again.
    #[serde(skip_serializing_if = "is_false")]
    pub confirm: bool,
}

/// The site's id for the partner, as it comes over the authenticated link:
/// a side still `local-owner` takes it and moves (asking first when it took
/// a file from before while empty); a side that already goes by it has
/// nothing to do, but a move it did not finish (rows left under
/// `local-owner`, or [`Here::moving`] still set); a side that goes by
/// another (linked, or taken from a file), or holds another partner id's
/// talk, is refused, and may be started empty to link.
pub fn link(here: &Here, theirs: &str) -> Result<Linked, LogError> {
    here.check()?;
    check(theirs)?;
    if here.holds_another(theirs) {
        return Err(LogError::new(
            "this side holds another partner id's talk: start it empty to link",
        ));
    }
    let from = match &here.id {
        None => Some(LEGACY.to_string()),
        Some(id) if id == theirs => (here.unmoved() || here.moving).then(|| LEGACY.to_string()),
        Some(_) => {
            return Err(LogError::new(
                "this side goes by another partner id: start it empty to link",
            ));
        }
    };
    Ok(Linked {
        id: theirs.to_string(),
        confirm: here.filed && from.is_some(),
        from,
    })
}

/// What a file means for this side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filed {
    /// The id its pages are under, which is this side's from now on: the
    /// holder's `owner` to take them with (`nodes.wire_take`).
    pub id: String,
    /// This side takes it as its own id (it was empty and never linked).
    #[serde(skip_serializing_if = "is_false")]
    pub adopt: bool,
    /// A file from before taken onto an empty side, as it is: keep
    /// [`Here::filed`] set until this side's move from `local-owner` has
    /// finished, or it is started empty.
    #[serde(skip_serializing_if = "is_false")]
    pub filed: bool,
}

/// Whether a file of the partner id `header` (none: a file from before,
/// its pages under `local-owner`) may come in here, and under which id.
pub fn file(here: &Here, header: Option<&str>) -> Result<Filed, LogError> {
    here.check()?;
    let open = here.empty() && !here.linked && here.id.is_none();
    let id = header.unwrap_or(LEGACY);
    if let Some(header) = header {
        check(header)?;
    }
    let mine = here.id.as_deref().unwrap_or(LEGACY);
    if here.holds_another(mine) {
        return Err(LogError::new(
            "this side holds another partner id's talk than its own",
        ));
    }
    if header.is_some() && id == mine {
        return Ok(Filed {
            id: id.to_string(),
            adopt: false,
            filed: false,
        });
    }
    if !open {
        return Err(LogError::new(
            "the file's partner is not this side's, and this side is not empty",
        ));
    }
    Ok(Filed {
        id: id.to_string(),
        adopt: id != mine,
        filed: header.is_none(),
    })
}

/// One move of a side's partner id: from `local-owner` to the id the link
/// gave. Never from one partner id to another: what a side holds under a
/// partner id is that partner's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Move {
    pub from: String,
    pub to: String,
}

impl Move {
    pub fn check(&self) -> Result<(), LogError> {
        if self.from != LEGACY {
            return Err(LogError::new("a move is from local-owner"));
        }
        check(&self.to)?;
        if self.from == self.to {
            return Err(LogError::new("a move goes to another id"));
        }
        Ok(())
    }

    /// Whether `audience` is the partner's private talk, before or after.
    pub fn is_ours(&self, audience: &str) -> bool {
        audience
            .strip_prefix("personal:")
            .is_some_and(|member| member == self.from || member == self.to)
    }

    /// `audience` moved: `personal:<from>` is `personal:<to>`.
    pub fn audience(&self, audience: &str) -> String {
        match audience.strip_prefix("personal:") {
            Some(member) if member == self.from => format!("personal:{}", self.to),
            _ => audience.to_string(),
        }
    }

    /// A person's id moved: `from` is `to`.
    pub fn person<'a>(&'a self, id: &'a str) -> &'a str {
        if id == self.from { &self.to } else { id }
    }

    /// An object or a ledger target about a person moved.
    fn about(&self, object: &str) -> String {
        match object.split_once(':') {
            Some((kind, id)) if PERSON_OBJECTS.contains(&kind) && id == self.from => {
                format!("{kind}:{}", self.to)
            }
            _ => object.to_string(),
        }
    }

    /// Whether rows of `audience` may name the partner: theirs, and hers.
    fn touches(&self, audience: &str) -> bool {
        self.is_ours(audience) || matches!(Audience::parse(audience), Ok(Audience::Own))
    }
}

/// Object types whose id is a person: `subject:<id>` (a forget of all
/// about them), `affect:<id>` (her mood towards them).
pub(crate) const PERSON_OBJECTS: [&str; 2] = [crate::kinds::SUBJECT, "affect"];

/// `row` moved: the partner's private audience under the new id, and the
/// partner as subject and as an object's id. Rows of her own audience
/// change only in such objects; other audiences not at all. The same row
/// moves to the same bytes on every side.
pub fn row(row: &Row, by: &Move) -> Row {
    if !by.touches(&row.audience) {
        return row.clone();
    }
    let subject = match by.is_ours(&row.audience) {
        true => row
            .subject
            .as_deref()
            .map(|subject| by.person(subject).to_string()),
        false => row.subject.clone(),
    };
    Row {
        audience: by.audience(&row.audience),
        subject,
        object: by.about(&row.object),
        ..row.clone()
    }
}

/// A ledger line moved as [`row`] moves rows: its audience, and a target
/// about the partner (`subject:<from>`, `affect:<from>`), so what was
/// forgotten under the old id stays forgotten under the new. Node ids,
/// homes and names stay.
pub fn entry(entry: &Entry, by: &Move) -> Entry {
    if !by.touches(&entry.audience) {
        return entry.clone();
    }
    Entry {
        audience: by.audience(&entry.audience),
        target: by.about(&entry.target),
        ..entry.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P0: &str = "partner_0199d5c0900033333333333333333333";
    const PF: &str = "partner_0199d5c0a00011111111111111111111";

    fn here(id: Option<&str>, linked: bool, audiences: &[&str]) -> Here {
        Here {
            id: id.map(str::to_string),
            linked,
            audiences: audiences.iter().map(|one| one.to_string()).collect(),
            filed: false,
            moving: false,
        }
    }

    #[test]
    fn a_mint_is_its_time_then_its_random() {
        let id = mint(0x0199_d5c0_a000, &[0x11; 10]).unwrap();
        assert_eq!(id, PF);
        assert!(check(&id).is_ok());
        assert!(mint(-1, &[0; 10]).is_err());
        assert!(mint(MIN_MS - 1, &[0; 10]).is_err(), "before 2025");
        assert!(check("partner_00000000000011111111111111111111").is_err());
        assert!(mint(MAX_MS + 1, &[0; 10]).is_err());
        assert!(mint(0x0199_d5c0_a000, &[0; 9]).is_err());
        for bad in [
            "local-owner",
            "partner_",
            "partner_0199D5C0A00011111111111111111111",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_link_gives_its_id_once_and_a_side_moves_to_it() {
        // A Mac still local-owner takes the site's id and moves.
        let fresh = link(&here(None, false, &["personal:local-owner"]), P0).unwrap();
        assert_eq!(
            (fresh.id.as_str(), fresh.from.as_deref()),
            (P0, Some(LEGACY))
        );
        // One that took another id from a file is refused: what it holds may
        // be another account's partner's. One that took this id is linked.
        assert!(link(&here(Some(PF), false, &[&format!("personal:{PF}")]), P0).is_err());
        assert_eq!(link(&here(Some(P0), false, &[]), P0).unwrap().from, None);
        // Linked before: the same id is nothing to do, another is refused.
        assert_eq!(link(&here(Some(P0), true, &[]), P0).unwrap().from, None);
        assert!(link(&here(Some(PF), true, &[]), P0).is_err());
        assert!(link(&here(None, false, &[]), "acct_7").is_err());
        // A move cut off half way: the id is set, rows under local-owner
        // remain, so it moves again.
        let half = here(
            Some(P0),
            true,
            &["personal:local-owner", &format!("personal:{P0}")],
        );
        assert_eq!(link(&half, P0).unwrap().from.as_deref(), Some(LEGACY));
    }

    #[test]
    fn a_side_that_took_a_file_from_before_asks_before_it_moves() {
        let mut filed = here(None, false, &["personal:local-owner", "own:"]);
        filed.filed = true;
        let linked = link(&filed, P0).unwrap();
        assert_eq!(
            linked,
            Linked {
                id: P0.into(),
                from: Some(LEGACY.into()),
                confirm: true
            }
        );
        // On no, the side starts empty: nothing to ask, nothing to move.
        let cleared = here(None, false, &[]);
        assert!(!link(&cleared, P0).unwrap().confirm);
        // On yes, it moved: once done, nothing is left to ask about.
        let mut done = here(Some(P0), true, &[&format!("personal:{P0}")]);
        done.filed = true;
        assert_eq!(
            link(&done, P0).unwrap(),
            Linked {
                id: P0.into(),
                from: None,
                confirm: false
            }
        );
        // Half way through the move it still asks: `filed` stays until the
        // move has finished.
        let mut half = here(
            Some(P0),
            true,
            &["personal:local-owner", &format!("personal:{P0}")],
        );
        half.filed = true;
        assert!(link(&half, P0).unwrap().confirm);
        // The file says so when it comes in.
        let empty = here(None, false, &[]);
        assert!(file(&empty, None).unwrap().filed);
        assert!(!file(&empty, Some(PF)).unwrap().filed);
        // A filed side takes no other file.
        let filed = here(None, false, &["personal:local-owner", "own:"]);
        assert!(file(&filed, None).is_err());
        assert!(file(&filed, Some(PF)).is_err());
    }

    #[test]
    fn a_move_cut_off_is_taken_up_again_until_the_side_says_it_is_done() {
        // The partner's talk moved and the id was saved, but not her own
        // rows or the ledger: no row of local-owner is left to say so, but
        // the side says it is moving.
        let mut cut = here(Some(P0), true, &[&format!("personal:{P0}"), "own:"]);
        cut.moving = true;
        assert_eq!(link(&cut, P0).unwrap().from.as_deref(), Some(LEGACY));
        cut.moving = false;
        assert_eq!(link(&cut, P0).unwrap().from, None);
        // Rows moved, the id not saved yet: it moves again.
        let unsaved = here(None, false, &[&format!("personal:{P0}"), "own:"]);
        assert_eq!(link(&unsaved, P0).unwrap().from.as_deref(), Some(LEGACY));
    }

    #[test]
    fn a_side_never_goes_by_two_ids() {
        // It holds another partner id's talk: no link, and no file of its
        // own id either.
        let two = here(
            Some(P0),
            true,
            &[&format!("personal:{P0}"), &format!("personal:{PF}")],
        );
        assert!(link(&two, P0).is_err());
        assert!(file(&two, Some(P0)).is_err());
        let unsaved = here(None, false, &[&format!("personal:{PF}")]);
        assert!(link(&unsaved, P0).is_err());
    }

    #[test]
    fn a_file_never_changes_the_id_of_a_side_that_holds_anything() {
        let full = |id, linked| here(id, linked, &["own:"]);
        // Its own partner's file comes in, as it is.
        let same = file(&full(Some(P0), true), Some(P0)).unwrap();
        assert_eq!((same.id.as_str(), same.adopt), (P0, false));
        // Another partner id, a file from before (local-owner), or a
        // made-up header: refused, on a Mac and on the site alike.
        for (side, header) in [
            (full(Some(P0), true), Some(PF)),
            (full(Some(P0), true), None),
            (full(None, false), Some(PF)),
            (full(None, false), None),
            (
                full(None, false),
                Some("partner_00000000000011111111111111111111"),
            ),
        ] {
            assert!(file(&side, header).is_err(), "{side:?} {header:?}");
        }
        // An empty Mac that never linked takes the file's id, or a file
        // from before as it is; an empty linked one does not.
        let empty = here(None, false, &["group:g1"]);
        assert_eq!(
            file(&empty, Some(PF)).unwrap(),
            Filed {
                id: PF.into(),
                adopt: true,
                filed: false
            }
        );
        assert_eq!(
            file(&empty, None).unwrap(),
            Filed {
                id: LEGACY.into(),
                adopt: false,
                filed: true
            }
        );
        assert!(file(&here(Some(P0), true, &[]), Some(PF)).is_err());
        assert!(file(&here(Some(P0), true, &[]), None).is_err());
        // A side that took an id, even with nothing under it yet, never takes
        // another file's, nor pages under local-owner.
        let adopted = here(Some(PF), false, &[]);
        assert!(file(&adopted, Some(P0)).is_err());
        assert!(file(&adopted, None).is_err());
        assert!(!file(&adopted, Some(PF)).unwrap().adopt);
    }

    #[test]
    fn a_move_takes_rows_and_the_ledger_to_the_new_id() {
        let by = Move {
            from: LEGACY.into(),
            to: P0.into(),
        };
        assert!(by.check().is_ok());
        for bad in [
            ("acct_7", P0),
            (LEGACY, LEGACY),
            (P0, P0),
            (LEGACY, "x"),
            (PF, P0),
        ] {
            let bad = Move {
                from: bad.0.into(),
                to: bad.1.into(),
            };
            assert!(bad.check().is_err(), "{bad:?}");
        }
        let line = |audience: &str, target: &str| Entry {
            audience: audience.into(),
            target: target.into(),
            at: serde_json::from_value(serde_json::json!("001760000000001-000000-dev_mac"))
                .unwrap(),
            kind: crate::forget::Kind::Forget,
            homes: Vec::new(),
            to: None,
            takes: Vec::new(),
        };
        let moved = entry(&line("personal:local-owner", "subject:local-owner"), &by);
        assert_eq!(
            moved,
            line(&format!("personal:{P0}"), &format!("subject:{P0}"))
        );
        let moved = entry(&line("own:", "affect:local-owner"), &by);
        assert_eq!(moved, line("own:", &format!("affect:{P0}")));
        // Someone else's talk, and her mood towards someone else, stay.
        let other = line("personal:9", "subject:local-owner");
        assert_eq!(entry(&other, &by), other);
        let other = line("own:", "affect:9");
        assert_eq!(entry(&other, &by), other);
    }
}
