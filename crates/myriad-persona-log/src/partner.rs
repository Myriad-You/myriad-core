//! One id for the partner on every side.
//!
//! The partner's private audience is `personal:<id>`, and node ids are made
//! from the audience, so two sides that call the partner by different ids
//! keep two logs of the same talk. Until now the Mac said `local-owner` and
//! the site its own account id. From here every side uses one id that
//! belongs to neither a device nor the site:
//!
//! - **Its form** ([`check`]): `partner_` and 32 lowercase hex digits: 12
//!   of the time it was minted (Unix milliseconds) then 20 random (80
//!   bits). It is an id, not a credential: it opens nothing, and it may
//!   stand in a file's clear header.
//! - **Who mints it** ([`mint`]): whichever side first needs it (the first
//!   time it writes a row of the partner's private talk under this core, or
//!   moves an old one). The caller gives the time and ten random bytes; the
//!   core reads neither. The id the side used before stays as an alias.
//! - **Two ids meet** ([`meet`]): over an authenticated link to the
//!   partner's own account two sides keep the smaller id in byte order,
//!   which is the one minted first, and the other becomes an alias. The
//!   rule only looks at the two ids, so any order of links ends on the same
//!   id. A file proves less: the id in its clear header can be copied by
//!   anyone who saw it, so a file never changes a side's id. A file that
//!   shares a partner id with this side adds its id as an alias, nothing
//!   more; a side that holds nothing of the partner yet and never met
//!   anyone takes a file's id as it is (the partner's decision, 2026-10-30:
//!   空的 Mac 直接认文件). A legacy id such as `local-owner` is every Mac's,
//!   so it shows nothing. A link takes none of the other side's aliases
//!   but the ones this side vouches for (`ours`): an alias would move every
//!   row under it into the partner's private talk, so a claim alone never
//!   does. An id the other side brings that this side knows is someone
//!   else's (`others`) is refused. Aliases past [`MAX_ALIASES`] are let
//!   go, never the two ids that meet, then the newest partner ids first: a
//!   row under one let go is moved by its own side when that side meets.
//! - **Moving** ([`row`]): a side whose id changed rewrites its rows once:
//!   `personal:<alias>` becomes `personal:<id>`, a subject that was the
//!   alias becomes the id, and so does an object about it (`subject:<alias>`,
//!   `affect:<alias>`). Ids, sides, counts and clocks stay, so a row is the
//!   same row on every side that moved it, and rows that come later from a
//!   side that has not moved yet are moved as they arrive. Node ids do not
//!   move: the partner's private talk makes them under one fixed name,
//!   [`LEGACY`] (`myriad_nodes::key`). What nodes add (bodies) is
//!   `myriad_nodes::migrate`.

use serde::{Deserialize, Serialize};

use crate::{LogError, Row, row::Audience, row::MAX_TOKEN_BYTES};

/// What every partner id starts with.
pub const PREFIX: &str = "partner_";
/// Hex digits of the mint time.
const TIME_DIGITS: usize = 12;
/// Random bytes the caller gives a mint.
pub const RANDOM_BYTES: usize = 10;
/// The latest mint time the form holds: 16^12 - 1 milliseconds.
const MAX_MS: i64 = (1 << 48) - 1;
/// The earliest mint time: 2025-01-01, before any side minted one. An id
/// that says it is older is made up, to win a meeting.
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
/// Aliases one partner may carry; a meeting keeps this many.
pub const MAX_ALIASES: usize = 32;

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

/// The partner as a side knows them: the id in use, and every id that
/// stood for them before (a legacy one such as `local-owner`, or a partner
/// id that lost a meeting), sorted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Partner {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// An alias is any one id a private audience could have held.
fn check_alias(alias: &str) -> Result<(), LogError> {
    let one = !alias.is_empty()
        && alias.len() <= MAX_TOKEN_BYTES
        && !alias
            .chars()
            .any(|ch| ch == ',' || ch.is_control() || ch.is_whitespace());
    if !one {
        return Err(LogError::new(
            "an alias is one id: no comma, space or control character",
        ));
    }
    Ok(())
}

impl Partner {
    pub fn check(&self) -> Result<(), LogError> {
        check(&self.id)?;
        if self.aliases.len() > MAX_ALIASES {
            return Err(LogError::new(format!(
                "a partner carries at most {MAX_ALIASES} aliases"
            )));
        }
        for alias in &self.aliases {
            check_alias(alias)?;
        }
        if self.aliases.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(LogError::new("aliases are sorted and do not repeat"));
        }
        if self.aliases.contains(&self.id) {
            return Err(LogError::new("the id in use is not its own alias"));
        }
        Ok(())
    }

    /// Whether `id` is this partner, by the id in use or an alias.
    pub fn is(&self, id: &str) -> bool {
        self.id == id || self.aliases.iter().any(|alias| alias == id)
    }

    /// `audience` as it reads now: `personal:<alias>`, the partner's alone,
    /// is `personal:<id>`; anything else is itself.
    pub fn audience(&self, audience: &str) -> String {
        match Audience::parse(audience) {
            Ok(Audience::Personal(member)) if member != self.id && self.is(member) => {
                format!("personal:{}", self.id)
            }
            _ => audience.to_string(),
        }
    }

    /// `id` as it reads now: an alias is the id in use.
    pub fn person<'a>(&'a self, id: &'a str) -> &'a str {
        if self.is(id) { &self.id } else { id }
    }
}

/// A side's partner, minted: `was`, the id it used until now, becomes an
/// alias.
pub fn minted(at_ms: i64, random: &[u8], was: Option<&str>) -> Result<Partner, LogError> {
    let id = mint(at_ms, random)?;
    let aliases: Vec<String> = was
        .filter(|was| *was != id)
        .map(str::to_string)
        .into_iter()
        .collect();
    let partner = Partner { id, aliases };
    partner.check()?;
    Ok(partner)
}

/// What two sides call the partner once they meet: the smaller id (the one
/// minted first), every other id an alias. `changed` says this side's id
/// moved, so it moves its rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Met {
    pub partner: Partner,
    #[serde(skip_serializing_if = "is_false")]
    pub changed: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// How a side knows the other side it meets is the partner's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    /// An authenticated link to the partner's own account: the site's
    /// welcome to a device it linked, a hello the site took for this
    /// account.
    Link,
    /// A file: it already shares a partner id with this side (a legacy
    /// alias does not count), or this side holds nothing of the partner yet
    /// (`empty`). It never changes the id of a side that holds anything.
    File,
}

/// Two ids meet, `by` a proof this side holds.
///
/// - `link`: the smaller id wins. Of the other side's aliases only those
///   this side vouches for (`ours`) are taken.
/// - `file`: a file that shares a partner id with this side adds its id as
///   an alias and changes nothing else. A side that holds nothing of the
///   partner yet (`empty`: no row in a `personal:` audience or in `own:`)
///   and never met anyone (no partner id among its aliases) takes the
///   file's id as it is (the partner's decision, 2026-10-30: 空的 Mac 直接认
///   文件). Any other file is refused.
///
/// The other side's id, and each alias this side vouches for, that this
/// side knows is someone else's (`others`: every person it holds a private
/// audience or a person node of, every other partner it serves) is refused;
/// legacy ids are not partner ids and are not checked. An `others` that
/// names the partner is refused too.
pub fn meet(
    mine: &Partner,
    theirs: &Partner,
    by: By,
    ours: &[String],
    others: &[String],
    empty: bool,
) -> Result<Met, LogError> {
    mine.check()?;
    theirs.check()?;
    if others.iter().any(|other| mine.is(other)) {
        return Err(LogError::new("others names the partner"));
    }
    let vouched: Vec<&String> = match by {
        By::Link => theirs
            .aliases
            .iter()
            .filter(|alias| ours.contains(alias))
            .collect(),
        By::File => Vec::new(),
    };
    let brought = std::iter::once(&theirs.id).chain(vouched.iter().copied());
    if brought
        .filter(|id| check(id).is_ok())
        .any(|id| others.contains(id))
    {
        return Err(LogError::new(
            "an id the other side brings is someone else's here",
        ));
    }
    // Only a partner id counts: a legacy one (`local-owner`) is every
    // Mac's, so it shows nothing about whose file this is.
    let shared = mine.is(&theirs.id)
        || theirs
            .aliases
            .iter()
            .any(|alias| check(alias).is_ok() && mine.is(alias));
    let never_met = !mine.aliases.iter().any(|alias| check(alias).is_ok());
    let id = match by {
        By::Link => mine.id.as_str().min(theirs.id.as_str()).to_string(),
        By::File if empty && never_met => theirs.id.clone(),
        By::File if shared => mine.id.clone(),
        By::File => {
            return Err(LogError::new(
                "a file shares no partner id with this side, which is not empty",
            ));
        }
    };
    // The two ids that meet always stay; then this side's aliases, and the
    // other side's it vouches for.
    let meeting: Vec<String> = [&mine.id, &theirs.id]
        .into_iter()
        .filter(|one| **one != id)
        .cloned()
        .collect();
    let mut rest: Vec<String> = mine
        .aliases
        .iter()
        .chain(vouched.iter().copied())
        .filter(|alias| **alias != id && !meeting.contains(alias))
        .cloned()
        .collect();
    // Past the cap: legacy ids first, then partner ids oldest first.
    rest.sort_by(|a, b| (check(a).is_ok(), a).cmp(&(check(b).is_ok(), b)));
    rest.dedup();
    rest.truncate(MAX_ALIASES - meeting.len());
    let mut aliases: Vec<String> = meeting.into_iter().chain(rest).collect();
    aliases.sort();
    aliases.dedup();
    let partner = Partner { id, aliases };
    partner.check()?;
    Ok(Met {
        changed: partner.id != mine.id,
        partner,
    })
}

/// Object types whose id is a person: `subject:<id>` (a forget of all
/// about them), `affect:<id>` (her mood towards them).
const PERSON_OBJECTS: [&str; 2] = [crate::kinds::SUBJECT, "affect"];

/// `row` as it reads under `partner`: the partner's private audience under
/// the id in use, and the partner as subject and as an object's id. Rows of
/// her own audience change only in such objects; other audiences not at
/// all. The same row moves to the same bytes on every side.
pub fn row(row: &Row, partner: &Partner) -> Row {
    let audience = partner.audience(&row.audience);
    let ours = audience != row.audience || row.audience == format!("personal:{}", partner.id);
    let own = matches!(Audience::parse(&row.audience), Ok(Audience::Own));
    if !ours && !own {
        return row.clone();
    }
    let object = match row.object.split_once(':') {
        Some((kind, id)) if PERSON_OBJECTS.contains(&kind) && partner.is(id) => {
            format!("{kind}:{}", partner.id)
        }
        _ => row.object.clone(),
    };
    let subject = if ours {
        row.subject
            .as_deref()
            .map(|subject| partner.person(subject).to_string())
    } else {
        row.subject.clone()
    };
    Row {
        audience,
        subject,
        object,
        ..row.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "partner_0199d5c0a00011111111111111111111";

    #[test]
    fn a_mint_is_its_time_then_its_random_and_the_first_one_wins() {
        let early = mint(0x0199_d5c0_a000, &[0x11; 10]).unwrap();
        assert_eq!(early, A);
        assert!(check(&early).is_ok());
        let late = mint(0x0199_d5c0_a001, &[0x00; 10]).unwrap();
        assert!(early < late, "the earlier mint sorts first");
        assert!(mint(-1, &[0; 10]).is_err());
        assert!(mint(MIN_MS - 1, &[0; 10]).is_err(), "before 2025");
        assert!(check("partner_00000000000011111111111111111111").is_err());
        assert!(mint(MAX_MS + 1, &[0; 10]).is_err());
        assert!(mint(1, &[0; 9]).is_err());
        for bad in [
            "local-owner",
            "partner_",
            "partner_0199D5C0A00011111111111111111111",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn meetings_in_any_order_end_on_the_same_id() {
        let mac = minted(0x0199_d5c0_a000, &[0x22; 10], Some("local-owner")).unwrap();
        let site = minted(0x0199_d5c0_9000, &[0x33; 10], Some("o41")).unwrap();
        let other = minted(0x0199_d5c0_b000, &[0x01; 10], Some("local-owner")).unwrap();
        let link =
            |a: &Partner, b: &Partner| meet(a, b, By::Link, &[], &[], false).unwrap().partner;
        let one = link(&link(&mac, &site), &other);
        let two = link(&other, &link(&site, &mac));
        assert_eq!(one.id, site.id);
        assert_eq!(two.id, site.id);
        // Each side keeps its own former ids and the ids it met; none of
        // the other side's aliases.
        assert_eq!(
            link(&site, &mac).aliases,
            ["o41".to_string(), mac.id.clone()]
        );
        assert_eq!(
            link(&mac, &site).aliases,
            ["local-owner".to_string(), mac.id.clone()]
        );
        let met = meet(&mac, &site, By::Link, &[], &[], false).unwrap();
        assert!(met.changed, "the Mac's id lost");
        assert!(
            !meet(&site, &mac, By::Link, &[], &[], false)
                .unwrap()
                .changed
        );
        // An alias the side vouches for is taken.
        let vouched = meet(&site, &mac, By::Link, &["local-owner".into()], &[], false).unwrap();
        assert_eq!(
            vouched.partner.aliases,
            ["local-owner".to_string(), "o41".into(), mac.id.clone()]
        );
    }

    #[test]
    fn no_claim_alone_moves_anyone_else_into_the_partner() {
        let site = minted(0x0199_d5c0_a000, &[0x33; 10], Some("o41")).unwrap();
        // A hello that claims another person of the site as an alias, and
        // one the site does not hold yet: neither alias is taken.
        let claim = Partner {
            id: mint(0x0199_d5c0_8000, &[0; 10]).unwrap(),
            aliases: vec!["o100".into(), "o99".into()],
        };
        let met = meet(&site, &claim, By::Link, &[], &[], false).unwrap();
        assert_eq!(met.partner.aliases, ["o41".to_string(), site.id.clone()]);
        // Its id, if this side knows it is someone else's, is refused; a
        // legacy alias it brings is not looked at, for it is not taken.
        assert!(meet(&site, &claim, By::Link, &[], &["o99".into()], false).is_ok());
        assert!(
            meet(
                &site,
                &claim,
                By::Link,
                &[],
                std::slice::from_ref(&claim.id),
                false
            )
            .is_err()
        );
        assert!(
            meet(&site, &claim, By::Link, &[], &["o41".into()], false).is_err(),
            "others names the partner"
        );
        // A file meets only when it already shares a partner id.
        assert!(meet(&site, &claim, By::File, &[], &[], false).is_err());
        // A friend's export carries `local-owner` too, as every Mac's
        // partner does: that is no shared id.
        let mac = minted(0x0199_d5c0_a000, &[0x22; 10], Some(LEGACY)).unwrap();
        let friend = Partner {
            id: minted(0x0199_d5c0_b000, &[0x44; 10], None).unwrap().id,
            aliases: vec![LEGACY.into()],
        };
        assert!(meet(&mac, &friend, By::File, &[], &[], false).is_err());
        // A legacy id it shares does not count either; a partner id does.
        let file = Partner {
            id: claim.id.clone(),
            aliases: vec!["o41".into()],
        };
        assert!(meet(&site, &file, By::File, &[], &[], false).is_err());
    }

    #[test]
    fn a_file_never_changes_the_id_of_a_side_that_holds_anything() {
        let site = minted(0x0199_d5c0_a000, &[0x33; 10], Some("o41")).unwrap();
        // A file that shares the site's id, under an earlier id anyone who
        // saw a header could have made: only that id becomes an alias.
        let file = Partner {
            id: mint(0x0199_d5c0_8000, &[0x99; 10]).unwrap(),
            aliases: vec!["o7".into(), site.id.clone()],
        };
        let met = meet(&site, &file, By::File, &["o7".into()], &[], false).unwrap();
        assert!(!met.changed);
        assert_eq!(met.partner.id, site.id);
        assert_eq!(met.partner.aliases, ["o41".to_string(), file.id.clone()]);
    }

    #[test]
    fn an_empty_side_takes_a_files_partner_id_as_it_is() {
        // A Mac set up just now: its id was minted before the file's, and
        // it holds nothing of the partner.
        let fresh = minted(0x0199_d5c0_9000, &[0x55; 10], Some(LEGACY)).unwrap();
        let file = minted(0x0199_d5c0_a000, &[0x66; 10], Some("o41")).unwrap();
        assert!(meet(&fresh, &file, By::File, &[], &[], false).is_err());
        let met = meet(&fresh, &file, By::File, &[], &[], true).unwrap();
        assert!(met.changed);
        assert_eq!(met.partner.id, file.id, "the file's id, not the smaller");
        assert!(!met.partner.is("o41"), "none of the file's aliases");
        // A side that met someone before is not empty, whatever it says.
        let other = minted(0x0199_d5c0_b000, &[0x77; 10], None).unwrap();
        let linked = meet(&fresh, &other, By::Link, &[], &[], false)
            .unwrap()
            .partner;
        assert!(meet(&linked, &file, By::File, &[], &[], true).is_err());
        // A link still keeps the smaller.
        let linked = meet(&fresh, &file, By::Link, &[], &[], true).unwrap();
        assert_eq!(linked.partner.id, fresh.id);
    }

    #[test]
    fn past_the_cap_the_ids_that_meet_always_stay() {
        let side = |n: u8| minted(0x0199_d5c0_a000 + i64::from(n), &[n; 10], None).unwrap();
        let sides: Vec<Partner> = (0..40).map(side).collect();
        let mut held = minted(0x0199_d5c0_9000, &[0x77; 10], Some("local-owner")).unwrap();
        for next in sides.iter().rev() {
            let met = meet(&held, next, By::Link, &[], &[], false)
                .unwrap()
                .partner;
            assert!(met.check().is_ok());
            assert!(met.aliases.len() <= MAX_ALIASES);
            assert!(met.is(&next.id), "the id it met stays");
            assert!(met.is(&held.id), "its own id stays");
            assert!(met.is("local-owner"), "a legacy id goes last");
            held = met;
        }
        assert_eq!(held.aliases.len(), MAX_ALIASES);
    }
}
