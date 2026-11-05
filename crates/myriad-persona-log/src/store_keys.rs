//! A computer's keys for its own database files, and how a file moves from
//! the clear to sealed and from one key to the next without ever losing
//! what is in it.
//!
//! Each database file of hers on a computer is sealed whole by a key of its
//! own. That key is kept in a small file beside the data (the store keys),
//! sealed by one key that wraps them all (kept by the host, e.g. its
//! keychain), each generation bound to its file and number
//! ([`crate::keys::Bound::StoreKey`]). The store keys stay out of every
//! backup the host makes or the system makes, so when a forget moves a file
//! to its next generation, every older copy of that file, in any backup,
//! no longer opens: the key it needs is nowhere.
//!
//! The host draws keys and nonces, seals and opens with [`crate::keys`],
//! reads and moves files and writes the store keys whole and at once. This
//! module says what the store keys hold ([`StoreKeys`]), how one step
//! changes them ([`step`]) and, from what the host finds on disk, what to do
//! next ([`settle`]); a host that stops anywhere and starts again asks
//! [`settle`] and goes on from there.
//!
//! Moving a file from the clear (`migrate`): a key is made and recorded
//! (`migrating`), the file is copied sealed beside itself (`<file>.encrypting`)
//! and checked, the clear file is moved aside (`<file>.plain-aside`) and
//! the sealed copy takes its name (`swapped`); only once the host has opened
//! it as it always does is the clear copy deleted (`ready`). Until then the
//! clear copy is there to go back to.
//!
//! Moving to the next key (a forget): the next generation is recorded
//! beside the current one, the file is rekeyed in one transaction, and the
//! next becomes the current one (`rotated`). A file opens with the current
//! key, or with the next while one is recorded, so a stop in between loses
//! nothing.

use std::collections::BTreeMap;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::LogError;
use crate::keys::{KEY_BYTES, NONCE_BYTES, TAG_BYTES};

/// The store keys' `format`.
pub const FORMAT: &str = "myriad.store-keys";
/// The store keys' `version`.
pub const VERSION: u32 = 1;
/// Bytes of one sealed key.
pub const SEALED_KEY_BYTES: usize = NONCE_BYTES + KEY_BYTES + TAG_BYTES;

/// Where a file is on its way to being sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Its key is made; the file is still in the clear.
    Migrating,
    /// The sealed copy has the file's name; the clear one is aside.
    Swapped,
    /// Sealed, and nothing in the clear is left.
    Ready,
}

/// One generation of a file's key: the key sealed (base64).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub generation: u64,
    pub key: String,
}

/// One file's key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileKey {
    pub generation: u64,
    pub state: State,
    pub key: String,
    /// The generation a rekey is moving the file to, while it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<Generation>,
}

/// The store keys: every file's key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreKeys {
    pub format: String,
    pub version: u32,
    pub files: BTreeMap<String, FileKey>,
}

impl Default for StoreKeys {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            files: BTreeMap::new(),
        }
    }
}

fn check_name(file: &str) -> Result<(), LogError> {
    let fits = !file.is_empty()
        && file.len() <= 32
        && file
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if fits {
        Ok(())
    } else {
        Err(LogError::new(
            "a file's name is 1 to 32 of a-z, 0-9 and the hyphen",
        ))
    }
}

fn check_key(key: &str) -> Result<(), LogError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(key.as_bytes())
        .map_err(|_| LogError::new("a sealed key is base64"))?;
    if bytes.len() == SEALED_KEY_BYTES {
        Ok(())
    } else {
        Err(LogError::new(format!(
            "a sealed key is {SEALED_KEY_BYTES} bytes"
        )))
    }
}

impl StoreKeys {
    /// Refused unless it is store keys this version reads, every name fits,
    /// every key is one sealed key and a next generation is the one after.
    pub fn check(&self) -> Result<(), LogError> {
        if self.format != FORMAT {
            return Err(LogError::new("not store keys"));
        }
        if self.version != VERSION {
            return Err(LogError::new(format!(
                "store keys of version {} are not read here",
                self.version
            )));
        }
        for (name, file) in &self.files {
            check_name(name)?;
            if file.generation == 0 {
                return Err(LogError::new("a generation starts at 1"));
            }
            check_key(&file.key)?;
            if let Some(next) = &file.next {
                if file.state != State::Ready {
                    return Err(LogError::new("only a sealed file moves to a next key"));
                }
                if next.generation != file.generation + 1 {
                    return Err(LogError::new("the next generation is the one after"));
                }
                check_key(&next.key)?;
            }
        }
        Ok(())
    }
}

/// One step a host took. A new key comes sealed for the generation
/// [`settle`] or [`FileKey::generation`] named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    /// A key for moving the file from the clear: `migrating`.
    Begin(Generation),
    /// A key for a file made new, sealed from its first byte: `ready`.
    Created(Generation),
    /// The sealed copy has the file's name: `swapped`.
    Swapped,
    /// The sealed file opened as the host always opens it: `ready`.
    Opened,
    /// Moving from the clear is given up: the file's key goes and the
    /// clear file stays the file.
    Abandon,
    /// The next generation, before the rekey.
    Rotate(Generation),
    /// The rekey went through: the next is the current key.
    Rotated,
    /// The rekey did not happen: the next generation goes.
    DropNext,
}

/// The store keys after `event` on `file`. Refused when the step does not
/// follow from where the file is.
pub fn step(keys: Option<StoreKeys>, file: &str, event: Event) -> Result<StoreKeys, LogError> {
    check_name(file)?;
    let mut keys = keys.unwrap_or_default();
    keys.check()?;
    let current = keys.files.get(file).cloned();
    let after_last = current.as_ref().map_or(1, |key| key.generation + 1);
    let fresh = |generation: &Generation, state: State| -> Result<FileKey, LogError> {
        if generation.generation != after_last {
            return Err(LogError::new(format!(
                "a new key for this file is generation {after_last}"
            )));
        }
        check_key(&generation.key)?;
        Ok(FileKey {
            generation: generation.generation,
            state,
            key: generation.key.clone(),
            next: None,
        })
    };
    let wrong = |what: &str| Err(LogError::new(format!("{what} does not follow from here")));
    match (current, event) {
        (
            None
            | Some(FileKey {
                state: State::Ready,
                next: None,
                ..
            }),
            Event::Begin(generation),
        ) => {
            keys.files
                .insert(file.into(), fresh(&generation, State::Migrating)?);
        }
        (
            None
            | Some(FileKey {
                state: State::Ready,
                next: None,
                ..
            }),
            Event::Created(generation),
        ) => {
            keys.files
                .insert(file.into(), fresh(&generation, State::Ready)?);
        }
        (
            Some(
                mut key @ FileKey {
                    state: State::Migrating,
                    ..
                },
            ),
            Event::Swapped,
        ) => {
            key.state = State::Swapped;
            keys.files.insert(file.into(), key);
        }
        (
            Some(
                mut key @ FileKey {
                    state: State::Swapped,
                    ..
                },
            ),
            Event::Opened,
        ) => {
            key.state = State::Ready;
            keys.files.insert(file.into(), key);
        }
        (
            Some(FileKey {
                state: State::Migrating | State::Swapped,
                ..
            }),
            Event::Abandon,
        ) => {
            keys.files.remove(file);
        }
        (
            Some(
                mut key @ FileKey {
                    state: State::Ready,
                    next: None,
                    ..
                },
            ),
            Event::Rotate(next),
        ) => {
            if next.generation != key.generation + 1 {
                return Err(LogError::new("the next generation is the one after"));
            }
            check_key(&next.key)?;
            key.next = Some(next);
            keys.files.insert(file.into(), key);
        }
        (Some(mut key @ FileKey { next: Some(_), .. }), Event::Rotated) => {
            let next = key.next.take().expect("matched");
            key.generation = next.generation;
            key.key = next.key;
            keys.files.insert(file.into(), key);
        }
        (Some(mut key @ FileKey { next: Some(_), .. }), Event::DropNext) => {
            key.next = None;
            keys.files.insert(file.into(), key);
        }
        (_, Event::Begin(_)) => return wrong("a key for moving from the clear"),
        (_, Event::Created(_)) => return wrong("a key for a new file"),
        (_, Event::Swapped) => return wrong("swapped"),
        (_, Event::Opened) => return wrong("opened"),
        (_, Event::Abandon) => return wrong("abandon"),
        (_, Event::Rotate(_)) => return wrong("a next key"),
        (_, Event::Rotated) => return wrong("rotated"),
        (_, Event::DropNext) => return wrong("drop the next key"),
    }
    Ok(keys)
}

/// What the host found where the file lives: the file itself (`plain`
/// when it starts as a database in the clear does, `sealed` when it holds
/// bytes and does not, `missing` when it is not there or empty), and
/// whether the sealed copy being made and the clear copy aside are there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Found {
    pub main: Main,
    pub encrypting: bool,
    pub aside: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Main {
    Plain,
    Sealed,
    Missing,
}

/// What to do next. After any action but `open`, `lost_key` and
/// `keep_plain`, the host asks again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Open the sealed file with its key (or the next one while there is).
    Open,
    /// Start moving the clear file: a new key of `generation`, `begin`.
    Migrate,
    /// Copy the clear file sealed to `<file>.encrypting` with the recorded
    /// key, check the copy against it, then move the clear file aside and
    /// the copy to the file's name, then `swapped`.
    Export,
    /// Delete what is left of an unfinished sealed copy.
    DeleteEncrypting,
    /// The clear file is aside already: move the sealed copy to its name.
    FinishSwap,
    /// The sealed copy has the name: record `swapped`.
    MarkSwapped,
    /// Open the sealed file as always; then `opened` and delete the clear
    /// copy aside. If it does not open: delete the sealed file, move the
    /// clear copy back, `abandon`.
    OpenThenDropAside,
    /// The clear copy aside is no longer needed: delete it.
    DeleteAside,
    /// Record `opened`: the clear copy aside is gone already.
    MarkReady,
    /// Move the clear copy aside back to the file's name.
    RestoreAside,
    /// The file went back to the clear: `abandon`.
    Abandon,
    /// Make the file new with a key of `generation`, `created`.
    Create,
    /// The file is sealed and no key here is recorded for it. Never make a
    /// new key over it: say so and keep the file as it is.
    LostKey,
    /// Two clear files where one is expected: leave both, open the file in
    /// the clear this time and say so.
    KeepPlain,
}

/// What to do next, and the generation for a new key when it needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Next {
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

/// From where the store keys say `file` is and what is on disk, what to do
/// next.
pub fn settle(keys: Option<&StoreKeys>, file: &str, found: Found) -> Result<Next, LogError> {
    check_name(file)?;
    if let Some(keys) = keys {
        keys.check()?;
    }
    let key = keys.and_then(|keys| keys.files.get(file));
    let after_last = key.map_or(1, |key| key.generation + 1);
    let plain = |action| Next {
        action,
        generation: None,
    };
    let new_key = |action| Next {
        action,
        generation: Some(after_last),
    };
    let Found {
        main,
        encrypting,
        aside,
    } = found;
    Ok(match key.map(|key| key.state) {
        None => match main {
            _ if encrypting => plain(Action::DeleteEncrypting),
            Main::Plain if aside => plain(Action::KeepPlain),
            Main::Plain => new_key(Action::Migrate),
            Main::Sealed => plain(Action::LostKey),
            Main::Missing if aside => plain(Action::RestoreAside),
            Main::Missing => new_key(Action::Create),
        },
        Some(State::Migrating) => match main {
            Main::Plain if aside => plain(Action::KeepPlain),
            Main::Plain if encrypting => plain(Action::DeleteEncrypting),
            Main::Plain => plain(Action::Export),
            Main::Sealed => plain(Action::MarkSwapped),
            Main::Missing if encrypting => plain(Action::FinishSwap),
            Main::Missing if aside => plain(Action::RestoreAside),
            Main::Missing => plain(Action::Abandon),
        },
        Some(State::Swapped) => match main {
            _ if encrypting => plain(Action::DeleteEncrypting),
            Main::Sealed if aside => plain(Action::OpenThenDropAside),
            Main::Sealed => plain(Action::MarkReady),
            Main::Missing if aside => plain(Action::RestoreAside),
            Main::Plain | Main::Missing => plain(Action::Abandon),
        },
        Some(State::Ready) => match main {
            Main::Sealed if aside => plain(Action::DeleteAside),
            _ if encrypting => plain(Action::DeleteEncrypting),
            Main::Sealed => plain(Action::Open),
            // A copy in the clear put back (from a backup made before):
            // sealed again under a new key.
            Main::Plain if aside => plain(Action::KeepPlain),
            Main::Plain => new_key(Action::Migrate),
            Main::Missing if aside => plain(Action::RestoreAside),
            Main::Missing => new_key(Action::Create),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Bound, open, seal};

    const WRAP: [u8; KEY_BYTES] = [3; KEY_BYTES];

    fn sealed(file: &str, generation: u64, key: u8) -> Generation {
        let bound = Bound::StoreKey {
            file: file.into(),
            generation,
        };
        let bytes = seal(
            &WRAP,
            &[generation as u8; NONCE_BYTES],
            &bound,
            &[key; KEY_BYTES],
        )
        .unwrap();
        Generation {
            generation,
            key: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    fn found(main: Main, encrypting: bool, aside: bool) -> Found {
        Found {
            main,
            encrypting,
            aside,
        }
    }

    /// Runs a host that does what `settle` says over a file that is in the
    /// clear, and returns the store keys once the file opens.
    fn migrate(mut keys: Option<StoreKeys>, mut disk: Found) -> StoreKeys {
        for _ in 0..12 {
            let next = settle(keys.as_ref(), "myriad", disk).unwrap();
            match next.action {
                Action::Migrate => {
                    let generation = next.generation.unwrap();
                    keys = Some(
                        step(
                            keys,
                            "myriad",
                            Event::Begin(sealed("myriad", generation, 9)),
                        )
                        .unwrap(),
                    );
                }
                Action::Export => {
                    disk = found(Main::Sealed, false, true);
                    keys = Some(step(keys, "myriad", Event::Swapped).unwrap());
                }
                Action::OpenThenDropAside => {
                    keys = Some(step(keys, "myriad", Event::Opened).unwrap());
                    disk.aside = false;
                }
                Action::Open => return keys.unwrap(),
                other => panic!("{other:?}"),
            }
        }
        panic!("did not settle")
    }

    #[test]
    fn a_clear_file_is_sealed_and_its_key_opens_for_its_own_generation_only() {
        let keys = migrate(None, found(Main::Plain, false, false));
        let file = &keys.files["myriad"];
        assert_eq!((file.generation, file.state), (1, State::Ready));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.key)
            .unwrap();
        let bound = |generation| Bound::StoreKey {
            file: "myriad".into(),
            generation,
        };
        assert_eq!(open(&WRAP, &bytes, &bound(1)).unwrap(), [9; KEY_BYTES]);
        assert!(open(&WRAP, &bytes, &bound(2)).is_err());
        let other = Bound::StoreKey {
            file: "record".into(),
            generation: 1,
        };
        assert!(open(&WRAP, &bytes, &other).is_err());
    }

    #[test]
    fn a_host_stopped_anywhere_comes_back_to_one_sealed_file() {
        let begun = step(None, "myriad", Event::Begin(sealed("myriad", 1, 9))).unwrap();
        let swapped = step(Some(begun.clone()), "myriad", Event::Swapped).unwrap();
        // Stopped while copying, while moving the clear file aside, after
        // the copy took the name, and before the clear copy was deleted.
        for (keys, disk, first) in [
            (
                &begun,
                found(Main::Plain, true, false),
                Action::DeleteEncrypting,
            ),
            (&begun, found(Main::Missing, true, true), Action::FinishSwap),
            (
                &begun,
                found(Main::Sealed, false, true),
                Action::MarkSwapped,
            ),
            (
                &swapped,
                found(Main::Sealed, false, true),
                Action::OpenThenDropAside,
            ),
            (
                &swapped,
                found(Main::Sealed, false, false),
                Action::MarkReady,
            ),
            (
                &swapped,
                found(Main::Missing, false, true),
                Action::RestoreAside,
            ),
        ] {
            assert_eq!(
                settle(Some(keys), "myriad", disk).unwrap().action,
                first,
                "{disk:?}"
            );
        }
    }

    #[test]
    fn a_sealed_file_without_its_key_is_never_given_a_new_one() {
        let next = settle(None, "myriad", found(Main::Sealed, false, false)).unwrap();
        assert_eq!(next.action, Action::LostKey);
        assert_eq!(next.generation, None);
        let other = step(None, "record", Event::Created(sealed("record", 1, 4))).unwrap();
        let next = settle(Some(&other), "myriad", found(Main::Sealed, false, false)).unwrap();
        assert_eq!(next.action, Action::LostKey);
    }

    #[test]
    fn a_clear_copy_put_back_is_sealed_again_under_a_new_generation() {
        let keys = migrate(None, found(Main::Plain, false, false));
        let next = settle(Some(&keys), "myriad", found(Main::Plain, false, false)).unwrap();
        assert_eq!((next.action, next.generation), (Action::Migrate, Some(2)));
        let keys = migrate(Some(keys), found(Main::Plain, false, false));
        assert_eq!(keys.files["myriad"].generation, 2);
    }

    #[test]
    fn a_rekey_moves_to_the_next_generation_or_drops_it() {
        let keys = step(None, "myriad", Event::Created(sealed("myriad", 1, 1))).unwrap();
        assert!(
            step(
                Some(keys.clone()),
                "myriad",
                Event::Rotate(sealed("myriad", 3, 2))
            )
            .is_err()
        );
        let rotating = step(
            Some(keys.clone()),
            "myriad",
            Event::Rotate(sealed("myriad", 2, 2)),
        )
        .unwrap();
        assert_eq!(
            rotating.files["myriad"].next.as_ref().unwrap().generation,
            2
        );
        assert!(
            step(
                Some(rotating.clone()),
                "myriad",
                Event::Rotate(sealed("myriad", 2, 3))
            )
            .is_err()
        );
        let rotated = step(Some(rotating.clone()), "myriad", Event::Rotated).unwrap();
        assert_eq!(
            rotated.files["myriad"],
            FileKey {
                generation: 2,
                state: State::Ready,
                key: sealed("myriad", 2, 2).key,
                next: None,
            }
        );
        let dropped = step(Some(rotating), "myriad", Event::DropNext).unwrap();
        assert_eq!(dropped, keys);
        assert!(step(Some(keys), "myriad", Event::Rotated).is_err());
    }

    #[test]
    fn steps_out_of_order_and_malformed_keys_are_refused() {
        assert!(step(None, "myriad", Event::Swapped).is_err());
        assert!(step(None, "myriad", Event::Begin(sealed("myriad", 2, 1))).is_err());
        assert!(step(None, "Myriad", Event::Begin(sealed("myriad", 1, 1))).is_err());
        let short = Generation {
            generation: 1,
            key: "AAAA".into(),
        };
        assert!(step(None, "myriad", Event::Begin(short)).is_err());
        let begun = step(None, "myriad", Event::Begin(sealed("myriad", 1, 9))).unwrap();
        assert!(step(Some(begun.clone()), "myriad", Event::Opened).is_err());
        assert!(
            step(
                Some(begun.clone()),
                "myriad",
                Event::Rotate(sealed("myriad", 2, 1))
            )
            .is_err()
        );
        let mut old = begun;
        old.version = 2;
        assert!(settle(Some(&old), "myriad", found(Main::Sealed, false, false)).is_err());
    }
}
