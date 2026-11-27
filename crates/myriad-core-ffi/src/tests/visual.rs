//! How she is drawn, and the persona a new one starts from: through the C
//! entry point and straight from the crate. The prompts and contracts are
//! also pinned byte for byte by the shared conformance files
//! (`crates/conformance/v1/visual`);
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi visual_case`
//! writes their outputs again after a deliberate change.

use myriad_merope::{
    CharacterAssetProfile, FullBodySource, MEROPE_STYLE_REFERENCE_SHA256, RenderingProvenance,
    STICKER_AVATAR_CONTRACT_VERSION, build_character_asset_contract,
    build_character_asset_contract_with, build_character_visual_edit_prompt,
    build_character_visual_prompt, build_full_body_asset_contract,
    build_full_body_asset_contract_with, build_full_body_design_prompt,
    build_full_body_portrait_prompt, build_sticker_avatar_contract, build_sticker_avatar_prompt,
    build_upright_portrait_prompt, character_asset_contract_fingerprint,
    ensure_visual_identity_states_gender, fallback_persona_draft,
    normalize_visual_identity_for_prompt_checked,
    normalize_visual_requirements_for_design_with_gender, sanitize_persona_draft,
};
use serde_json::{Value, json};

use super::{ok, text};

pub(super) const TESTED: &[&str] = &[
    "motion.standing_together",
    "persona.draft",
    "persona.fallback",
    "sticker.avatar_contract",
    "sticker.avatar_prompt",
    "visual.adjustment_in_scope",
    "visual.character_contract",
    "visual.character_prompt",
    "visual.checks",
    "visual.clothing_style",
    "visual.edit_prompt",
    "visual.full_body_contract",
    "visual.full_body_design_prompt",
    "visual.full_body_portrait_prompt",
    "visual.identity",
    "visual.identity_for_prompt",
    "visual.requirements",
    "visual.states_gender",
    "visual.texts",
    "visual.upright_prompt",
];

fn identity() -> Value {
    json!({
        "character": {
            "faceDesign": "feminine young-adult read, elongated oval face with a calm gaze",
            "eyeDesign": "violet eyes with layered irises",
            "hairShape": "long silver hair in broad ribbon masses",
            "hairLayerPlan": "a few broad front locks over a long back mass",
        },
        "outfit": {
            "upperBodySilhouette": "fitted jacket over a wrapped layer",
            "outfitConstruction": "asymmetric closure with an open neckline",
            "sleeveArmDesign": "split sleeves with ribbon cuffs",
            "heroAccessory": "a crescent brooch",
            "materialPlan": "matte wool and polished silver",
            "paletteHint": "silver, violet and white",
            "motif": "crescent moons",
            "clothingStyle": "everyday",
        },
    })
}

fn profile() -> Value {
    json!({
        "gender": "female",
        "language": "en-US",
        "clothingStyle": "everyday",
        "visualIdentity": identity(),
    })
}

#[test]
fn her_picture_prompts_are_the_crates() {
    let profile = profile();
    assert_eq!(
        text(
            "visual.character_prompt",
            json!({ "name": "星见", "profile": profile })
        ),
        Some(build_character_visual_prompt("星见", &profile, None)),
    );
    assert_eq!(
        text(
            "visual.character_prompt",
            json!({ "name": "星见", "profile": profile, "notes": "softer light" }),
        ),
        Some(build_character_visual_prompt(
            "星见",
            &profile,
            Some("softer light")
        )),
    );
    assert_eq!(
        text(
            "visual.full_body_design_prompt",
            json!({ "name": "星见", "profile": profile })
        ),
        Some(build_full_body_design_prompt("星见", &profile)),
    );
    assert_eq!(
        text("visual.edit_prompt", json!({ "notes": "a small smile" })),
        Some(build_character_visual_edit_prompt("a small smile")),
    );
    assert_eq!(
        text("visual.full_body_portrait_prompt", json!({})),
        Some(build_full_body_portrait_prompt()),
    );
    for (profile, expected) in [
        ("bust", CharacterAssetProfile::Bust),
        ("fullBody", CharacterAssetProfile::FullBody),
    ] {
        assert_eq!(
            text("visual.upright_prompt", json!({ "profile": profile })),
            Some(build_upright_portrait_prompt(expected)),
        );
    }
    assert_eq!(
        text(
            "sticker.avatar_prompt",
            json!({ "name": "星见", "profile": profile })
        ),
        Some(build_sticker_avatar_prompt("星见", &profile)),
    );
    let texts = ok("visual.texts", json!({}));
    assert_eq!(texts["styleReferenceSha256"], MEROPE_STYLE_REFERENCE_SHA256);
    assert_eq!(
        texts["sticker"]["contractVersion"],
        STICKER_AVATAR_CONTRACT_VERSION
    );
    assert!(
        texts["clothingStyles"]
            .as_array()
            .is_some_and(|styles| styles.contains(&json!("everyday")))
    );
}

#[test]
fn her_picture_contracts_are_the_crates_and_their_text_is_what_is_fingerprinted() {
    let profile = profile();
    let bust = ok(
        "visual.character_contract",
        json!({ "name": "星见", "profile": profile }),
    );
    let expected = build_character_asset_contract("星见", &profile, None);
    assert_eq!(bust["contract"], expected);
    assert_eq!(bust["text"], serde_json::to_string(&expected).unwrap());
    assert_eq!(
        bust["fingerprint"],
        character_asset_contract_fingerprint(&expected)
    );
    let from_bust = ok(
        "visual.full_body_contract",
        json!({ "source": { "bust": { "portrait": "asset-1", "generationFingerprint": "abc" } } }),
    );
    assert_eq!(
        from_bust["contract"],
        build_full_body_asset_contract(FullBodySource::Bust {
            portrait: "asset-1",
            generation_fingerprint: Some("abc"),
        }),
    );
    let from_design = ok(
        "visual.full_body_contract",
        json!({ "source": { "design": { "name": "星见", "profile": profile } } }),
    );
    assert_eq!(
        from_design["contract"],
        build_full_body_asset_contract(FullBodySource::Design {
            name: "星见",
            visual_profile: &profile,
        }),
    );
    let sticker = ok(
        "sticker.avatar_contract",
        json!({ "name": "星见", "profile": profile, "sourcePortraitAssetId": "asset-1" }),
    );
    let expected = build_sticker_avatar_contract("星见", &profile, "asset-1");
    assert_eq!(sticker["contract"], expected);
    assert_eq!(sticker["text"], serde_json::to_string(&expected).unwrap());
}

/// A contract for a portrait a recipe service drew records that service's
/// provenance; without `rendering` it is this build's own, as before.
#[test]
fn a_contract_carries_the_rendering_it_is_given() {
    let profile = profile();
    let rendering = json!({
        "recipe": "merope-persona",
        "visualSchoolVersion": "house-v6",
        "styleReferenceSha256": "b".repeat(64),
        "fullBodyInstructionVersion": "full-body-standing-v1",
    });
    let provenance: RenderingProvenance = serde_json::from_value(rendering.clone()).unwrap();
    let bust = ok(
        "visual.character_contract",
        json!({ "name": "星见", "profile": profile, "rendering": rendering }),
    );
    assert_eq!(
        bust["contract"],
        build_character_asset_contract_with("星见", &profile, None, &provenance)
    );
    assert_eq!(bust["contract"]["rendering"]["recipe"], "merope-persona");
    let from_design = ok(
        "visual.full_body_contract",
        json!({
            "source": { "design": { "name": "星见", "profile": profile } },
            "rendering": rendering,
        }),
    );
    assert_eq!(
        from_design["contract"],
        build_full_body_asset_contract_with(
            FullBodySource::Design {
                name: "星见",
                visual_profile: &profile,
            },
            &provenance
        )
    );
    let mut bad = rendering;
    bad["styleReferenceSha256"] = json!("not-a-hash");
    let (status, out) = super::ffi(
        "visual.character_contract",
        &json!({ "name": "星见", "profile": profile, "rendering": bad }),
    );
    assert_ne!(status, crate::STATUS_OK);
    assert_eq!(out["error"]["kind"], "bad_input");
}

#[test]
fn a_design_is_checked_as_the_crate_checks_it() {
    let checked = ok("visual.identity_for_prompt", json!({ "value": profile() }));
    assert_eq!(
        checked["identity"],
        normalize_visual_identity_for_prompt_checked(&profile()).unwrap()
    );
    assert_eq!(checked["issue"], Value::Null);
    let broken = ok("visual.identity", json!({ "value": { "character": {} } }));
    assert_eq!(broken["identity"], Value::Null);
    assert!(broken["issue"]["field"].is_string() && broken["issue"]["reason"].is_string());
    let checks = ok(
        "visual.checks",
        json!({ "value": identity(), "gender": "female" }),
    );
    assert_eq!(checks["genderMatches"], true);
    assert_eq!(checks["facialDrift"], true);
    assert_eq!(checks["highCollar"], false);
    let plain = json!({ "character": { "faceDesign": "calm gaze" }, "outfit": {} });
    assert_eq!(
        ok(
            "visual.states_gender",
            json!({ "value": identity(), "gender": "male", "language": "zh-CN" })
        )["identity"],
        ensure_visual_identity_states_gender(&identity(), "male", "zh-CN").unwrap_or(Value::Null),
    );
    assert_eq!(
        ok(
            "visual.states_gender",
            json!({ "value": plain, "gender": "male", "language": "zh-CN" })
        )["identity"],
        Value::Null,
    );
    assert_eq!(
        text(
            "visual.requirements",
            json!({ "text": "少年感，高领", "gender": "female" })
        ),
        Some(normalize_visual_requirements_for_design_with_gender(
            "少年感，高领",
            "female"
        )),
    );
    assert_eq!(
        ok(
            "visual.adjustment_in_scope",
            json!({ "text": "softer lighting" })
        )["inScope"],
        true
    );
    assert_eq!(
        ok(
            "visual.adjustment_in_scope",
            json!({ "text": "change her hairstyle" })
        )["inScope"],
        false
    );
    let style = ok("visual.clothing_style", json!({ "text": "everyday" }));
    assert_eq!(style["style"], "everyday");
    assert!(style["grammar"].is_string());
    assert_eq!(
        ok("visual.clothing_style", json!({ "text": "nope" }))["style"],
        Value::Null
    );
}

#[test]
fn a_new_persona_starts_from_the_crates_fallback() {
    let tags = vec!["温柔".to_string()];
    let fallback = ok(
        "persona.fallback",
        json!({ "name": "星见", "language": "en-US", "tags": [] }),
    );
    assert_eq!(
        fallback["draft"],
        fallback_persona_draft("星见", "en-US", &[])
    );
    assert_eq!(
        fallback["draft"]["summary"],
        "星见's personality will grow through shared experiences."
    );
    let base = fallback_persona_draft("星见", "zh-CN", &tags);
    let draft = json!({ "persona": { "summary": "她说「你好」。" } });
    let out = ok("persona.draft", json!({ "value": draft, "fallback": base }));
    assert_eq!(
        out["draft"],
        sanitize_persona_draft(&draft, &base).unwrap_or(Value::Null)
    );
    assert_eq!(out["complete"], false);
    assert_eq!(out["sludge"], true);
    assert_eq!(
        ok(
            "motion.standing_together",
            json!({ "what": "listening to a song" })
        )["activity"],
        "listening to a song, and the person who has her face open is listening along with her",
    );
}

/// Every case in `crates/conformance/v1/visual`, through the C entry point.
/// A recipe service that copies these prompts compares itself against the
/// same files.
#[test]
fn every_visual_case_answers_through_the_c_entry_point() {
    use myriad_persona_sync::canonical::canonical_bytes;
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/visual");
    let bless = std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some();
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
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
            let got = ok(&call, case["input"].clone());
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
    assert_eq!(
        calls,
        [
            "visual.character_contract",
            "visual.character_prompt",
            "visual.edit_prompt",
            "visual.full_body_contract",
            "visual.full_body_design_prompt",
            "visual.full_body_portrait_prompt",
        ]
    );
}
