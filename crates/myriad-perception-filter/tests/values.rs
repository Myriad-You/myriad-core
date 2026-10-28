//! `secrets::values`: what `secrets::scan` wipes, on the original text. The
//! same pass as the scan, so putting a mark over every stretch it returns
//! must give the scan's text, byte for byte.

mod synthetic;

use myriad_perception_filter::secrets::{self, Found, MARK, SecretKind};
use serde_json::Value;

/// The scan's text, rebuilt from the original and the stretches alone.
fn marked(text: &str, found: &[Found]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for stretch in found {
        out.push_str(&text[at..stretch.start]);
        out.push_str(MARK);
        at = stretch.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Every stretch is its own substring, in order, not overlapping, and the
/// marks over them are the scan.
fn holds(text: &str) -> Vec<Found> {
    let found = secrets::values(text);
    let mut at = 0;
    for stretch in &found {
        assert!(
            stretch.start >= at && stretch.start < stretch.end,
            "{found:?}"
        );
        assert_eq!(stretch.value, text[stretch.start..stretch.end]);
        at = stretch.end;
    }
    assert_eq!(marked(text, &found), secrets::scan(text).text, "{found:?}");
    found
}

fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(synthetic::expand(text).text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[test]
fn the_marks_over_the_stretches_are_the_scan_on_every_fixture_string() {
    let dir = format!("{}/fixtures/host-filter", env!("CARGO_MANIFEST_DIR"));
    let mut texts = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let raw = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        strings(&serde_json::from_str(&raw).unwrap(), &mut texts);
    }
    let mut wiped = 0;
    for text in &texts {
        wiped += holds(text).len();
    }
    assert!(texts.len() > 500 && wiped > 50, "{} {wiped}", texts.len());
}

#[test]
fn every_synthetic_key_is_one_stretch_in_any_setting() {
    for (name, key) in synthetic::named() {
        for text in [
            format!("cat .env 输出了 {key} 然后继续"),
            format!("Authorization: Bearer {key}"),
            format!("export OPENAI_API_KEY={key}\n下一行"),
            format!("Cookie: a={key}; b=2"),
            format!("https://probe:{key}@git.example/r.git"),
            format!("{key}\n{key}"),
        ] {
            let found = holds(&text);
            assert!(!found.is_empty(), "{name}: {text}");
            assert!(
                found
                    .iter()
                    .all(|stretch| stretch.value.contains(&key) || key.contains(&stretch.value)),
                "{name}: {found:?}"
            );
        }
    }
}

/// The prefix rule wipes the key first, then the cookie rule swallows its
/// mark: one stretch, the cookie's, on the original text.
#[test]
fn a_swallowed_mark_comes_back_as_the_outer_stretch() {
    let key = synthetic::named()[0].1.clone();
    let text = format!("Cookie: a={key}; b=2");
    let scan = secrets::scan(&text);
    assert_eq!(scan.wiped, [SecretKind::VendorPrefix, SecretKind::Cookie]);
    let found = holds(&text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, SecretKind::Cookie);
    assert_eq!(found[0].value, format!("a={key}; b=2"));
    assert_eq!(found[0].start, "Cookie: ".len());
    assert_eq!(found[0].end, text.len());
    assert!(!found[0].value.contains(MARK));
}

#[test]
fn a_key_across_lines_is_one_stretch() {
    let pem = synthetic::named().pop().unwrap().1;
    let text = format!("before\n{pem}\nafter");
    let found = holds(&text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, SecretKind::PrivateKey);
    assert_eq!(found[0].value, pem);
    assert_eq!(found[0].start, "before\n".len());
    assert!(found[0].value.contains('\n'));
}

/// Offsets count UTF-8 bytes of the original, not characters or UTF-16
/// units: `密码：` is 9 bytes, 3 characters.
#[test]
fn offsets_are_utf8_bytes_of_the_original() {
    let value = synthetic::random(18, "abcdefghijklmnop", 21);
    let text = format!("密码：password={value} 好的");
    let found = holds(&text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, SecretKind::Assignment);
    assert_eq!(found[0].value, value);
    assert_eq!(found[0].start, "密码：password=".len());
    assert_eq!(found[0].start, 18);
    assert_eq!(found[0].end, 18 + value.len());
}

#[test]
fn nothing_to_wipe_is_nothing_found() {
    for text in [
        "",
        "max_tokens=4096",
        "普通的一句话",
        "agent_watch_2026_10_19",
    ] {
        assert!(holds(text).is_empty(), "{text}");
    }
}

/// The value never reaches a debug print, so a failed assertion or a log
/// line cannot carry it.
#[test]
fn debug_never_prints_the_value() {
    let key = synthetic::named()[2].1.clone();
    let found = secrets::values(&format!("token {key}"));
    assert!(!found.is_empty());
    assert!(!format!("{found:?}").contains(&key));
}
