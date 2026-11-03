//! How she is drawn, the same on every end: the prompts her portraits, her
//! full-body sets and her sticker avatar are drawn with
//! (`myriad_merope::visual_prompt`, `sticker_avatar`), the contracts those
//! pictures are kept against (`visual_contract`), the checks a visual design
//! passes before it is drawn (`visual_design`, `visual_prompt`), and the
//! fallback a persona draft starts from (`persona`). Only the crate's own
//! functions, called as the site calls them; no rule is decided here.

use myriad_merope::{
    CLOTHING_STYLES, CharacterAssetProfile, FULL_BODY_PORTRAIT_INSTRUCTION_VERSION, FullBodySource,
    MEROPE_STICKER_STYLE_REFERENCE_SHA256, MEROPE_STYLE_REFERENCE_SHA256, MEROPE_VISUAL_SCHOOL,
    MEROPE_VISUAL_SCHOOL_VERSION, STICKER_AVATAR_CONTRACT_VERSION, STICKER_AVATAR_SIZE,
    VisualProfileIssue, build_character_asset_contract, build_character_visual_edit_prompt,
    build_character_visual_prompt, build_full_body_asset_contract, build_full_body_design_prompt,
    build_full_body_portrait_prompt, build_sticker_avatar_contract, build_sticker_avatar_prompt,
    build_upright_portrait_prompt, character_asset_contract_fingerprint, clothing_style_grammar,
    ensure_visual_identity_states_gender, fallback_persona_draft, normalize_clothing_style,
    normalize_visual_identity_for_prompt_checked,
    normalize_visual_requirements_for_design_with_gender, persona_draft_is_complete,
    persona_has_literary_sludge, portrait_adjustment_is_within_scope, sanitize_persona_draft,
    sanitize_upper_body_visual_identity_checked, visual_identity_has_body_proportion_drift,
    visual_identity_has_camera_composition_drift, visual_identity_has_facial_construction_drift,
    visual_identity_has_high_collar, visual_identity_has_literary_sludge,
    visual_identity_matches_gender_presentation, visual_identity_violates_style_lock,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Nothing, input, text};
use crate::Failure;

/// The fixed words and numbers of her pictures: the visual school and its
/// version, the style references' hashes, the full-body instruction's
/// version, the sticker contract, and the clothing styles a design may name.
pub(super) fn texts(raw: &[u8]) -> Result<Value, Failure> {
    let _: Nothing = input(raw)?;
    Ok(json!({
        "school": MEROPE_VISUAL_SCHOOL,
        "schoolVersion": MEROPE_VISUAL_SCHOOL_VERSION,
        "styleReferenceSha256": MEROPE_STYLE_REFERENCE_SHA256,
        "fullBodyInstructionVersion": FULL_BODY_PORTRAIT_INSTRUCTION_VERSION,
        "clothingStyles": CLOTHING_STYLES,
        "sticker": {
            "contractVersion": STICKER_AVATAR_CONTRACT_VERSION,
            "size": STICKER_AVATAR_SIZE,
            "styleReferenceSha256": MEROPE_STICKER_STYLE_REFERENCE_SHA256,
        },
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Drawn {
    name: String,
    /// The visual profile (gender, language, clothing style, visual identity).
    profile: Value,
    #[serde(default)]
    notes: Option<String>,
}

/// The bust master portrait's prompt (`build_character_visual_prompt`).
pub(super) fn character_prompt(raw: &[u8]) -> Result<Value, Failure> {
    let req: Drawn = input(raw)?;
    Ok(text(Some(build_character_visual_prompt(
        &req.name,
        &req.profile,
        req.notes.as_deref(),
    ))))
}

/// A full-body set drawn from the confirmed design (`build_full_body_design_prompt`).
pub(super) fn full_body_design_prompt(raw: &[u8]) -> Result<Value, Failure> {
    let req: Drawn = input(raw)?;
    if req.notes.is_some() {
        return Err(Failure::BadInput(
            "a full-body design takes no notes".into(),
        ));
    }
    Ok(text(Some(build_full_body_design_prompt(
        &req.name,
        &req.profile,
    ))))
}

/// Editing the master portrait with notes inside the locked school.
pub(super) fn edit_prompt(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        notes: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(build_character_visual_edit_prompt(&req.notes))))
}

/// A full-body set redrawn from a bust master portrait.
pub(super) fn full_body_portrait_prompt(raw: &[u8]) -> Result<Value, Failure> {
    let _: Nothing = input(raw)?;
    Ok(text(Some(build_full_body_portrait_prompt())))
}

/// An uploaded picture redrawn upright, as a bust (`bust`) or a full-body
/// standing illustration (`fullBody`).
pub(super) fn upright_prompt(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        profile: CharacterAssetProfile,
    }
    let req: In = input(raw)?;
    Ok(text(Some(build_upright_portrait_prompt(req.profile))))
}

/// Whether notes for a portrait only adjust it (light, expression, framing)
/// and change nobody.
pub(super) fn adjustment_in_scope(raw: &[u8]) -> Result<Value, Failure> {
    let req: Plain = input(raw)?;
    Ok(json!({ "inScope": portrait_adjustment_is_within_scope(&req.text) }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plain {
    text: String,
}

/// What someone asked of her design, rewritten into what may be drawn for
/// this gender.
pub(super) fn requirements(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        text: String,
        gender: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(
        normalize_visual_requirements_for_design_with_gender(&req.text, &req.gender),
    )))
}

fn checked(result: Result<Value, VisualProfileIssue>) -> Value {
    match result {
        Ok(identity) => json!({ "identity": identity, "issue": null }),
        Err(issue) => json!({
            "identity": null,
            "issue": { "field": issue.field, "reason": issue.reason.as_str() },
        }),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    value: Value,
}

/// A visual identity kept to its fields and their lengths, or why not.
pub(super) fn identity(raw: &[u8]) -> Result<Value, Failure> {
    let req: Identity = input(raw)?;
    Ok(checked(sanitize_upper_body_visual_identity_checked(
        &req.value,
    )))
}

/// A visual identity as a prompt draws it: kept, then its drifting words
/// rewritten into the house proportions, or why not.
pub(super) fn identity_for_prompt(raw: &[u8]) -> Result<Value, Failure> {
    let req: Identity = input(raw)?;
    Ok(checked(normalize_visual_identity_for_prompt_checked(
        &req.value,
    )))
}

/// A visual identity whose face states its gender (prefixed in its language
/// when it does not), or nothing when it is not one.
pub(super) fn states_gender(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        value: Value,
        gender: String,
        language: String,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "identity": ensure_visual_identity_states_gender(&req.value, &req.gender, &req.language),
    }))
}

/// What a visual identity does that a design may not: break the style lock,
/// drift the camera, the face or the body, wear a high collar, write
/// literary sludge; and whether its face reads as `gender`.
pub(super) fn checks(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        value: Value,
        gender: String,
    }
    let req: In = input(raw)?;
    let value = &req.value;
    Ok(json!({
        "styleLock": visual_identity_violates_style_lock(value),
        "cameraDrift": visual_identity_has_camera_composition_drift(value),
        "facialDrift": visual_identity_has_facial_construction_drift(value),
        "bodyDrift": visual_identity_has_body_proportion_drift(value),
        "highCollar": visual_identity_has_high_collar(value),
        "literarySludge": visual_identity_has_literary_sludge(value),
        "genderMatches": visual_identity_matches_gender_presentation(value, &req.gender),
    }))
}

/// A clothing style by its name, and the costume language it draws in.
pub(super) fn clothing_style(raw: &[u8]) -> Result<Value, Failure> {
    let req: Plain = input(raw)?;
    let style = normalize_clothing_style(&req.text);
    Ok(json!({
        "style": style,
        "grammar": style.and_then(clothing_style_grammar),
    }))
}

/// A contract, the text it is fingerprinted as, and its fingerprint.
fn contract(contract: Value) -> Result<Value, Failure> {
    let text =
        serde_json::to_string(&contract).map_err(|error| Failure::BadInput(error.to_string()))?;
    let fingerprint = character_asset_contract_fingerprint(&contract);
    Ok(json!({ "contract": contract, "text": text, "fingerprint": fingerprint }))
}

/// The bust master portrait's contract.
pub(super) fn character_contract(raw: &[u8]) -> Result<Value, Failure> {
    let req: Drawn = input(raw)?;
    contract(build_character_asset_contract(
        &req.name,
        &req.profile,
        req.notes.as_deref(),
    ))
}

/// A full-body set's contract: redrawn from a bust portrait, or drawn from
/// the design.
pub(super) fn full_body_contract(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    enum Source {
        #[serde(rename_all = "camelCase")]
        Bust {
            portrait: String,
            #[serde(default)]
            generation_fingerprint: Option<String>,
        },
        Design {
            name: String,
            profile: Value,
        },
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        source: Source,
    }
    let req: In = input(raw)?;
    contract(build_full_body_asset_contract(match &req.source {
        Source::Bust {
            portrait,
            generation_fingerprint,
        } => FullBodySource::Bust {
            portrait,
            generation_fingerprint: generation_fingerprint.as_deref(),
        },
        Source::Design { name, profile } => FullBodySource::Design {
            name,
            visual_profile: profile,
        },
    }))
}

/// Her sticker avatar's prompt.
pub(super) fn sticker_prompt(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        name: String,
        profile: Value,
    }
    let req: In = input(raw)?;
    Ok(text(Some(build_sticker_avatar_prompt(
        &req.name,
        &req.profile,
    ))))
}

/// Her sticker avatar's contract, anchored to the portrait it came from.
pub(super) fn sticker_contract(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct In {
        name: String,
        profile: Value,
        source_portrait_asset_id: String,
    }
    let req: In = input(raw)?;
    let contract =
        build_sticker_avatar_contract(&req.name, &req.profile, &req.source_portrait_asset_id);
    let text =
        serde_json::to_string(&contract).map_err(|error| Failure::BadInput(error.to_string()))?;
    Ok(json!({ "contract": contract, "text": text }))
}

/// The persona a new one starts from when the model's draft falls short.
pub(super) fn persona_fallback(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        name: String,
        language: String,
        tags: Vec<String>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "draft": fallback_persona_draft(&req.name, &req.language, &req.tags) }))
}

/// A persona draft kept to its fields over `fallback`, whether it is
/// complete, and whether it writes literary sludge or quotes a line.
pub(super) fn persona_draft(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        value: Value,
        fallback: Value,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "draft": sanitize_persona_draft(&req.value, &req.fallback),
        "complete": persona_draft_is_complete(&req.value),
        "sludge": persona_has_literary_sludge(&req.value),
    }))
}
