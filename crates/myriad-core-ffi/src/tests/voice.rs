//! How she says it, the same on every end: every case in the shared
//! conformance files (`crates/conformance/v1/voice`) through the C entry
//! point, as another platform runs them, and what the site put together
//! before these moved here (2026-11-03) said again through it.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi voice` writes the
//! outputs again after a deliberate change.

use std::path::PathBuf;

use myriad_merope::{draft, onboarding_prompts, sending, speaking, timing};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok, text};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "bits.ask",
    "draft.ask",
    "draft.send",
    "onboarding.import_system",
    "onboarding.name_system",
    "onboarding.observe_portrait_system",
    "onboarding.persona_system",
    "onboarding.tags_system",
    "onboarding.visual_design_system",
    "persona.flatten",
    "self_story.ask",
    "self_story.checked",
    "sending.goes_out",
    "sending.limit_take",
    "speaking.contract_at",
    "speaking.home",
    "speaking.soul",
    "timing.until_read",
    "timing.where_she_is",
];

#[test]
fn every_voice_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/voice");
    let bless = std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut calls = Vec::new();
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_string();
        for case in file["cases"].as_array_mut().unwrap() {
            let (status, out) = ffi(&call, &case["input"]);
            let got = match status {
                STATUS_OK => out,
                STATUS_BAD_INPUT => json!({ "error": "bad_input" }),
                other => panic!("{call}: status {other}: {out}"),
            };
            if bless {
                case["output"] = got;
                continue;
            }
            assert_eq!(
                canonical_bytes(&got),
                canonical_bytes(&case["output"]),
                "{} › {}",
                path.display(),
                case["name"]
            );
        }
        if bless {
            let mut written = serde_json::to_string_pretty(&file).unwrap();
            written.push('\n');
            std::fs::write(&path, written).unwrap();
        }
        calls.push(call);
    }
    assert_eq!(calls, TESTED);
}

/// The site's soul (`resolve_speaking_soul`: the persona, a blank line,
/// the site line) is what `speaking.soul` says where she lives on the site;
/// on her Mac the contract says the Mac and nothing names a site.
#[test]
fn her_soul_on_the_site_is_what_the_site_put_together() {
    let home = ok("speaking.home", json!({ "at": "site", "name": "星海" }))["home"].clone();
    let personality = "气质：想到就试\n\n她说话很短。";
    let site = text(
        "speaking.soul",
        json!({ "name": "瞳", "personality": personality, "home": home }),
    );
    assert_eq!(
        site.unwrap(),
        format!(
            "{}\n\n{}",
            speaking::format_persona("瞳", personality).unwrap(),
            speaking::site_line("星海")
        )
    );
    let mac = ok("speaking.home", json!({ "at": "mac" }))["home"].clone();
    let soul = text(
        "speaking.soul",
        json!({ "name": "瞳", "personality": personality, "home": mac }),
    )
    .unwrap();
    assert!(soul.contains("you live on this Mac") && !soul.contains("site"));
    assert_eq!(
        text("speaking.contract_at", json!({ "place": "this site" })).as_deref(),
        Some(speaking::PERSONA_SPEAKING_CONTRACT)
    );
}

/// What the site sends a turn as, and how long it types each, is what
/// `sending.goes_out` says for the same draws.
#[test]
fn a_reply_goes_out_as_the_site_sends_it() {
    let reply = "在呢。\n刚吃完饭！\n你呢？\n今天好冷";
    let (split, typing) = (0.62, [0.1, 0.9, 0.4]);
    let out = ok(
        "sending.goes_out",
        json!({ "reply": reply, "rolls": { "split": split, "typing": typing } }),
    );
    let lines = myriad_agent_rules::channel::as_messages(reply);
    let most = myriad_merope::talk_shape::messages_this_turn(lines.len(), split);
    let site = myriad_merope::talk_shape::goes_out_as(&lines, most, None);
    let got: Vec<&str> = out["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| message["text"].as_str().unwrap())
        .collect();
    assert_eq!(got, site);
    for (index, line) in site.iter().enumerate() {
        assert_eq!(
            out["messages"][index]["typingSeconds"]
                .as_f64()
                .unwrap()
                .to_bits(),
            timing::typing(line.chars().count(), typing[index]).to_bits()
        );
    }
    assert_eq!(out["stopped"], Value::Null);
    let (kept, stopped) = sending::limited(reply);
    assert_eq!((kept.as_str(), stopped), (reply, None));
}

#[test]
fn the_look_and_the_prompts_are_the_crates() {
    let asked = ok(
        "draft.ask",
        json!({ "said": "周六几点集合", "draft": "九点吧", "came": [{ "who": "阿明", "text": "十点" }] }),
    );
    assert_eq!(asked["ask"]["system"], draft::system());
    assert_eq!(
        asked["ask"]["input"],
        draft::input("周六几点集合", "九点吧", &[("阿明".into(), "十点".into())])
    );
    assert_eq!(asked["ask"]["withinSeconds"], draft::LOOK_WITHIN_SECONDS);
    assert_eq!(
        text("onboarding.persona_system", json!({})).as_deref(),
        Some(onboarding_prompts::PERSONA_SYSTEM_PROMPT)
    );
    assert_eq!(
        text("onboarding.name_system", json!({ "nameStyle": "mythic" })),
        Some(onboarding_prompts::name_system_prompt("mythic"))
    );
    let (status, out) = ffi(
        "persona.flatten",
        &json!({ "fields": { "secret": "x" }, "language": "zh-CN" }),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(
        !out.to_string().contains("\"x\""),
        "a refusal quotes no value: {out}"
    );
}
