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
//! - **Two ids meet** ([`meet`]): sides that minted on their own (a Mac
//!   that first links to the site, an export from another side) keep the
//!   smaller in byte order, which is the one minted first. Every alias
//!   either side had, and the other id, become aliases. The rule only looks
//!   at the two ids, so any order of meetings ends on the same id.
//! - **Moving** ([`row`]): a side whose id changed rewrites its rows once:
//!   `personal:<alias>` becomes `personal:<id>`, a subject that was the
//!   alias becomes the id, and so does an object about it (`subject:<alias>`,
//!   `affect:<alias>`). Ids, sides, counts and clocks stay, so a row is the
//!   same row on every side that moved it, and rows that come later from a
//!   side that has not moved yet are moved as they arrive. What nodes add
//!   (bodies, node ids made from the audience) is `myriad_nodes::migrate`.

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
/// Aliases one partner may carry.
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
    Ok(())
}

/// A new partner id: minted at `at_ms` (Unix milliseconds) with the
/// caller's random bytes.
pub fn mint(at_ms: i64, random: &[u8]) -> Result<String, LogError> {
    if !(0..=MAX_MS).contains(&at_ms) {
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

pub fn meet(mine: &Partner, theirs: &Partner) -> Result<Met, LogError> {
    mine.check()?;
    theirs.check()?;
    let id = mine.id.as_str().min(theirs.id.as_str()).to_string();
    let mut aliases: Vec<String> = mine
        .aliases
        .iter()
        .chain(&theirs.aliases)
        .chain([&mine.id, &theirs.id])
        .filter(|alias| **alias != id)
        .cloned()
        .collect();
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
        let one = meet(&meet(&mac, &site).unwrap().partner, &other).unwrap();
        let two = meet(&other, &meet(&site, &mac).unwrap().partner).unwrap();
        assert_eq!(one.partner, two.partner);
        assert_eq!(one.partner.id, site.id);
        assert_eq!(
            one.partner.aliases,
            [
                "local-owner".to_string(),
                "o41".into(),
                mac.id.clone(),
                other.id.clone()
            ]
        );
        let met = meet(&mac, &site).unwrap();
        assert!(met.changed, "the Mac's id lost");
        assert!(!meet(&site, &mac).unwrap().changed);
    }
}
