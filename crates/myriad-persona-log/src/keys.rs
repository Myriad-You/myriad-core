//! Keys for what a side keeps sealed at rest, written once for every side.
//!
//! No key ring ever leaves a side. Rows and their payloads go between
//! sides in the clear inside the link's TLS (the server has to read her
//! log, problem 5 of the sync plan); each holder seals what it keeps with
//! keys of its own, and forgetting reaches every side as `forget` rows,
//! which each side turns into deleting its own keys. What the core fixes is
//! that every side seals the same way and that one forget reaches the same
//! keys everywhere.
//!
//! - [`seal`] and [`open`]: AES-256-GCM, the caller's 12-byte nonce first,
//!   then the ciphertext and its tag. What a sealed value is bound to
//!   ([`Bound`]) goes in as associated data, so it cannot be moved to
//!   another row, object, account or file. The bytes are the ones the
//!   site's server log has sealed since S6 (`her_log/keys.rs`), so what it
//!   already keeps opens unchanged.
//! - [`forget_reach`]: which object keys a ledger entry deletes.
//! - The store keys of a computer (one whole-database key per file, kept
//!   wrapped by a key of its own) are in [`crate::store_keys`].
//!
//! Pure: keys and nonces come from the caller, never from here. A refusal
//! never names a key, a nonce, sealed bytes or what they hold.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use serde::{Deserialize, Serialize};

use crate::{LogError, kinds};

/// Bytes of every key here.
pub const KEY_BYTES: usize = 32;
/// Bytes of a nonce, the first bytes of every sealed value.
pub const NONCE_BYTES: usize = 12;
/// Bytes the tag adds.
pub const TAG_BYTES: usize = 16;

/// What a sealed value is bound to: its associated data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Bound {
    /// An object's key, sealed by its audience's key. `holder` names whose
    /// log it is on the side that keeps it (the site: the account's id).
    ObjectKey {
        holder: String,
        audience: String,
        object: String,
    },
    /// A row's payload, sealed by its object's key.
    Payload { holder: String, row: String },
    /// One generation of a computer's key for one of its database files,
    /// sealed by the key that wraps them ([`crate::store_keys`]).
    StoreKey { file: String, generation: u64 },
}

impl Bound {
    /// The associated data: a label, then each part on a line of its own.
    pub fn aad(&self) -> Result<Vec<u8>, LogError> {
        let (label, parts): (&str, Vec<String>) = match self {
            Self::ObjectKey {
                holder,
                audience,
                object,
            } => (
                "myriad-her-log/object-key",
                vec![holder.clone(), audience.clone(), object.clone()],
            ),
            Self::Payload { holder, row } => {
                ("myriad-her-log/payload", vec![holder.clone(), row.clone()])
            }
            Self::StoreKey { file, generation } => {
                if *generation == 0 {
                    return Err(LogError::new("a store key's generation starts at 1"));
                }
                (
                    "myriad-store-key",
                    vec![file.clone(), generation.to_string()],
                )
            }
        };
        // A part with a line break in it would read as two: refused.
        if parts
            .iter()
            .any(|part| part.is_empty() || part.contains('\n'))
        {
            return Err(LogError::new(
                "every part a value is bound to is one non-empty line",
            ));
        }
        let mut aad = label.to_string();
        for part in parts {
            aad.push('\n');
            aad.push_str(&part);
        }
        Ok(aad.into_bytes())
    }
}

fn cipher(key: &[u8; KEY_BYTES]) -> Result<aes_gcm::Aes256Gcm, LogError> {
    aes_gcm::Aes256Gcm::new_from_slice(key).map_err(|_| LogError::new("the key does not fit"))
}

/// `plain` sealed with `key` under the caller's fresh `nonce`, bound to
/// `bound`: the nonce, then ciphertext and tag. A nonce is never used twice
/// with one key; the caller draws it at random each time.
pub fn seal(
    key: &[u8; KEY_BYTES],
    nonce: &[u8; NONCE_BYTES],
    bound: &Bound,
    plain: &[u8],
) -> Result<Vec<u8>, LogError> {
    let aad = bound.aad()?;
    let sealed = cipher(key)?
        .encrypt(
            &aes_gcm::Nonce::from(*nonce),
            Payload {
                msg: plain,
                aad: &aad,
            },
        )
        .map_err(|_| LogError::new("could not seal"))?;
    let mut out = nonce.to_vec();
    out.extend(sealed);
    Ok(out)
}

/// What [`seal`] made, opened: refused when the key is another, the value
/// was bound to something else, or a byte changed.
pub fn open(key: &[u8; KEY_BYTES], sealed: &[u8], bound: &Bound) -> Result<Vec<u8>, LogError> {
    let aad = bound.aad()?;
    if sealed.len() < NONCE_BYTES + TAG_BYTES {
        return Err(LogError::new("too short to be sealed"));
    }
    let (nonce, body) = sealed.split_at(NONCE_BYTES);
    let nonce: [u8; NONCE_BYTES] = nonce
        .try_into()
        .map_err(|_| LogError::new("too short to be sealed"))?;
    cipher(key)?
        .decrypt(
            &aes_gcm::Nonce::from(nonce),
            Payload {
                msg: body,
                aad: &aad,
            },
        )
        .map_err(|_| LogError::new("does not open with this key for this place"))
}

/// The object keys a forget deletes: in `audience`, every key of an object
/// about the subject, or the one object's key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reach {
    Subject { audience: String, subject: String },
    Object { audience: String, object: String },
}

/// What a ledger entry `target` in `audience` reaches among the object keys:
/// `subject:<id>` reaches the keys of every object about that subject in
/// the audience (an object key is kept with the subject it is about); any
/// other target reaches its own object's key.
pub fn forget_reach(audience: &str, target: &str) -> Result<Reach, LogError> {
    if audience.is_empty() || target.is_empty() {
        return Err(LogError::new("a forget names an audience and a target"));
    }
    match target
        .strip_prefix(kinds::SUBJECT)
        .and_then(|rest| rest.strip_prefix(':'))
    {
        Some("") => Err(LogError::new("a subject forget names the subject")),
        Some(subject) => Ok(Reach::Subject {
            audience: audience.to_string(),
            subject: subject.to_string(),
        }),
        None => Ok(Reach::Object {
            audience: audience.to_string(),
            object: target.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_BYTES] = [7; KEY_BYTES];
    const NONCE: [u8; NONCE_BYTES] = [1; NONCE_BYTES];

    fn payload(row: &str) -> Bound {
        Bound::Payload {
            holder: "41".into(),
            row: row.into(),
        }
    }

    #[test]
    fn a_payload_opens_only_for_its_own_row_holder_and_key() {
        let sealed = seal(&KEY, &NONCE, &payload("row_1"), b"what she thought").unwrap();
        assert!(!sealed.windows(4).any(|w| w == b"what"), "not in the clear");
        assert_eq!(&sealed[..NONCE_BYTES], &NONCE);
        assert_eq!(sealed.len(), NONCE_BYTES + 16 + TAG_BYTES);
        assert_eq!(
            open(&KEY, &sealed, &payload("row_1")).unwrap(),
            b"what she thought"
        );
        assert!(open(&KEY, &sealed, &payload("row_2")).is_err());
        let other_holder = Bound::Payload {
            holder: "42".into(),
            row: "row_1".into(),
        };
        assert!(open(&KEY, &sealed, &other_holder).is_err());
        assert!(open(&[8; KEY_BYTES], &sealed, &payload("row_1")).is_err());
        let mut changed = sealed.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(open(&KEY, &changed, &payload("row_1")).is_err());
        assert!(open(&KEY, &sealed[..20], &payload("row_1")).is_err());
    }

    #[test]
    fn the_associated_data_is_the_servers_since_s6() {
        // `her_log/keys.rs` `object_aad`, `payload_aad`.
        let object = Bound::ObjectKey {
            holder: "41".into(),
            audience: "personal:p_1".into(),
            object: "node:n_1".into(),
        };
        assert_eq!(
            object.aad().unwrap(),
            b"myriad-her-log/object-key\n41\npersonal:p_1\nnode:n_1"
        );
        assert_eq!(
            payload("row_1").aad().unwrap(),
            b"myriad-her-log/payload\n41\nrow_1"
        );
        let store = Bound::StoreKey {
            file: "myriad".into(),
            generation: 3,
        };
        assert_eq!(store.aad().unwrap(), b"myriad-store-key\nmyriad\n3");
    }

    #[test]
    fn a_part_that_would_read_as_two_is_refused() {
        for bound in [
            payload("row\n1"),
            payload(""),
            Bound::StoreKey {
                file: "myriad".into(),
                generation: 0,
            },
        ] {
            assert!(seal(&KEY, &NONCE, &bound, b"x").is_err(), "{bound:?}");
        }
    }

    #[test]
    fn no_refusal_names_what_it_was_given() {
        let sealed = seal(&KEY, &NONCE, &payload("row_1"), b"secret words").unwrap();
        for error in [
            open(&[9; KEY_BYTES], &sealed, &payload("row_1")).unwrap_err(),
            open(&KEY, &sealed, &payload("row_2")).unwrap_err(),
            open(&KEY, &sealed[..5], &payload("row_1")).unwrap_err(),
        ] {
            let message = error.to_string();
            assert!(!message.contains("row_"), "{message}");
            assert!(!message.contains("secret"), "{message}");
        }
    }

    #[test]
    fn a_forget_reaches_its_object_or_everything_about_its_subject() {
        assert_eq!(
            forget_reach("personal:p_1", "node:n_1").unwrap(),
            Reach::Object {
                audience: "personal:p_1".into(),
                object: "node:n_1".into()
            }
        );
        assert_eq!(
            forget_reach("personal:p_1", "subject:s_9").unwrap(),
            Reach::Subject {
                audience: "personal:p_1".into(),
                subject: "s_9".into()
            }
        );
        assert!(forget_reach("personal:p_1", "subject:").is_err());
        assert!(forget_reach("", "node:n_1").is_err());
        // `subjects:x` is an object, not a subject forget.
        assert!(matches!(
            forget_reach("own:", "subjects:x").unwrap(),
            Reach::Object { .. }
        ));
    }
}
