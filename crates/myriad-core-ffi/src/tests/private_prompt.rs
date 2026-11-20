//! Her whole private-chat prompt: through the C entry point it is the
//! crate's, byte for byte, whatever order an end hands its sections in.

use myriad_merope::private_prompt::{Part, system, their_message};
use serde_json::json;

use super::{STATUS_OK, ffi, ok};

pub(super) const TESTED: &[&str] = &["speaking.private_prompt"];

#[test]
fn an_end_gets_the_crates_prompt_whatever_order_it_hands_sections_in() {
    let sections = [
        (Part::InnerMomentAgo, "## Inside you a moment ago\n还好"),
        (Part::Wardrobe, "## Your wardrobe\n两套"),
        (Part::Addressee, "## Addressee\n阿城"),
        (Part::Stance, "## Where you stand\n还在气"),
        (Part::Day, "## Your day\n一早"),
    ];
    let out = ok(
        "speaking.private_prompt",
        json!({
            "soul": "你是她。",
            "sections": sections
                .iter()
                .map(|(part, text)| json!({ "part": part, "text": text }))
                .collect::<Vec<_>>(),
            "words": "在吗",
            "beside": "They are on the page /music.",
        }),
    );
    let owned: Vec<(Part, String)> = sections
        .iter()
        .map(|(part, text)| (*part, text.to_string()))
        .collect();
    assert_eq!(out["system"], system("你是她。", &owned));
    assert_eq!(
        out["message"],
        their_message("在吗", "They are on the page /music.")
    );
    let system = out["system"].as_str().unwrap();
    let at = |heading: &str| system.find(heading).unwrap();
    assert!(at("## Addressee") < at("## Where you stand"));
    assert!(at("## Your day") < at("## Your wardrobe"));
    assert!(at("## Your wardrobe") < at("## Inside you a moment ago"));
}

#[test]
fn nothing_beside_their_words_is_just_their_words() {
    let out = ok(
        "speaking.private_prompt",
        json!({ "soul": "你是她。", "sections": [], "words": "在吗" }),
    );
    assert_eq!(out["message"], "在吗");
}

#[test]
fn a_part_it_does_not_know_is_refused() {
    let (status, _) = ffi(
        "speaking.private_prompt",
        &json!({
            "soul": "",
            "sections": [{ "part": "somewhereElse", "text": "x" }],
            "words": "",
        }),
    );
    assert_ne!(status, STATUS_OK);
}
