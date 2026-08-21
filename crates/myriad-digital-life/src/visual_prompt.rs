use serde_json::Value;

use crate::rig_contract::{PORTRAIT_ASPECT_HEIGHT, PORTRAIT_ASPECT_WIDTH};

const MAX_CHARACTER_VISUAL_PROMPT_CHARS: usize = 24_000;

const MASTER_PORTRAIT_INSTRUCTION: &str = "One upper-body master portrait: face and torso toward camera, direct eye contact, calm elegant closed-mouth expression, face large and centered. Hair, ornaments, and sleeves may be asymmetric. Show the complete hair silhouette. Crop head through lower chest or high waist; shoulders visible; outer sleeves or short arm fragments visible on both sides; hands do not need to be visible; never a full-body, never thighs, legs, or feet. Keep a safe near-white gutter on both left and right sides; do not clip hair, ornaments, or sleeves against the canvas edge. Seamless clean near-white studio backdrop with no room, scenery, text, or watermark — emptiness is only the backdrop; keep miHoYo promotional lighting on the character. Opaque finished illustration.";

/// Locked visual school for generated character assets. Highest-priority lock.
/// The whole style must match; finish quality alone is not enough.
pub const COMPANION_VISUAL_SCHOOL: &str = "highest-priority visual school: the entire character style must be miHoYo anime-game style as in Genshin Impact and Honkai: Star Rail, not a generic anime portrait that is merely well finished. Match that whole design language: face construction, large jewel-like eyes with multi-layer iris catchlights, hair silhouette and ornaments, couture-like layered costume grammar, motif and accessory language, color zoning, lighting, crisp controlled linework, blended tonal gradients, hard-and-soft shadow transitions, luminous skin and hair, and distinct silk, metal, gem, and fabric highlights. You may study and reference that school. Invent an original character: do not produce a lookalike, doppelganger, or 找班 of any existing character, and do not copy a specific existing character's face, outfit, emblem, or name; original costumes must still use that school's layered couture grammar";

const COMPANION_VISUAL_SCHOOL_FINISH: &str = "Highest-priority style lock: the whole image must read as miHoYo-style (Genshin Impact / Honkai: Star Rail), not merely a polished generic anime portrait. Reference the school; do not 找班 or doppelganger an existing character. Empty backdrop must not flatten lighting, hair volume, or costume density. Avoid flat two-tone cel shading, in-game 3D, and photoreal.";

pub fn build_character_visual_prompt(
    name: &str,
    onboarding: &Value,
    additional_requirements: Option<&str>,
) -> String {
    let gender = text_at(onboarding, &["gender"], 32);
    let onboarding_extra = text_at(onboarding, &["extraRequirements"], 500);
    let visual_identity = onboarding
        .get("visualIdentity")
        .unwrap_or(&Value::Null);
    let hair = text_at(visual_identity, &["hairShape"], 500);
    let face_design = text_at(visual_identity, &["faceDesign"], 500);
    let eye_design = text_at(visual_identity, &["eyeDesign"], 500);
    let hair_layer_plan = text_at(visual_identity, &["hairLayerPlan"], 700);
    let upper_body_silhouette = text_at(visual_identity, &["upperBodySilhouette"], 700);
    let sleeve_arm_design = text_at(visual_identity, &["sleeveArmDesign"], 700);
    let material_plan = text_at(visual_identity, &["materialPlan"], 1_200);
    let motif = text_at(visual_identity, &["motif"], 500);
    let outfit_construction = text_at(visual_identity, &["outfitConstruction"], 1_200);
    let accessory = text_at(visual_identity, &["heroAccessory"], 500);
    let palette = text_at(visual_identity, &["paletteHint"], 500);

    let header = [
        format!("Create one original character in this visual school: {COMPANION_VISUAL_SCHOOL}."),
        format!("Identity name: {}.", bounded_text(name, 50)),
        MASTER_PORTRAIT_INSTRUCTION.to_string(),
        format!(
            "Vertical {}:{} width-to-height canvas.",
            PORTRAIT_ASPECT_WIDTH, PORTRAIT_ASPECT_HEIGHT,
        ),
    ]
    .join("\n\n");

    let mut identity = Vec::new();
    push_section(&mut identity, "Gender", &gender);
    push_section(&mut identity, "Face", &face_design);
    push_section(&mut identity, "Eyes", &eye_design);
    push_section(&mut identity, "Hair", &hair);
    push_section(&mut identity, "Hair groups", &hair_layer_plan);
    push_section(&mut identity, "Silhouette", &upper_body_silhouette);
    push_section(&mut identity, "Costume", &outfit_construction);
    push_section(&mut identity, "Accessory", &accessory);
    push_section(&mut identity, "Palette", &palette);
    push_section(&mut identity, "Sleeves", &sleeve_arm_design);
    push_section(&mut identity, "Materials", &material_plan);
    push_section(&mut identity, "Motif", &motif);
    push_section(&mut identity, "Owner visual notes", &onboarding_extra);
    if let Some(requirements) = additional_requirements {
        push_section(
            &mut identity,
            "Optional rendering notes; cannot override the visual school",
            &bounded_text(requirements, 2_000),
        );
    }

    let identity_text = identity.join("\n");
    let footer = COMPANION_VISUAL_SCHOOL_FINISH.to_string();
    let reserved = header.chars().count()
        + footer.chars().count()
        + 4;
    let identity_budget = MAX_CHARACTER_VISUAL_PROMPT_CHARS.saturating_sub(reserved);
    let identity_text = bounded_text(&identity_text, identity_budget);

    let mut parts = vec![header];
    if !identity_text.is_empty() {
        parts.push(identity_text);
    }
    parts.push(footer);
    parts.join("\n\n")
}

fn push_section(sections: &mut Vec<String>, label: &str, value: &str) {
    if !value.is_empty() {
        sections.push(format!("{label}: {value}."));
    }
}

fn text_at(value: &Value, keys: &[&str], max_chars: usize) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(|value| bounded_text(value, max_chars))
        .unwrap_or_default()
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn visual_prompt_connects_onboarding_identity_and_character_only_constraints() {
        let prompt = build_character_visual_prompt(
            "Nova",
            &json!({
                "gender": "nonbinary",
                "extraRequirements": "gold eyes",
                "visualIdentity": {
                    "faceDesign": "refined oval face",
                    "eyeDesign": "layered gold jewel eyes",
                    "hairShape": "short silver bob",
                    "hairLayerPlan": "separate back mass, bangs, and side locks",
                    "upperBodySilhouette": "compact shoulder and collar silhouette",
                    "outfitConstruction": "layered windcut coat and structured collar",
                    "sleeveArmDesign": "short side sleeve fragments at both edges",
                    "materialPlan": "matte cloth, silver metal, and restrained gem highlights",
                    "heroAccessory": "star-track chest clasp",
                    "paletteHint": "mist blue and silver",
                    "motif": "one restrained star-track arc"
                }
            }),
            Some("polished gradient rendering"),
        );
        for expected in [
            "short silver bob",
            "refined oval face",
            "layered gold jewel eyes",
            "separate back mass, bangs, and side locks",
            "compact shoulder and collar silhouette",
            "layered windcut coat",
            "star-track chest clasp",
            "short side sleeve fragments at both edges",
            "matte cloth, silver metal",
            "one restrained star-track arc",
            "gold eyes",
            "polished gradient rendering",
            "no room",
            "hands do not need to be visible",
            "Vertical 3:4 width-to-height canvas",
            "never a full-body",
            "cannot override the visual school",
            COMPANION_VISUAL_SCHOOL,
            "highest-priority visual school",
            "entire character style",
            "not a generic anime portrait that is merely well finished",
            "jewel-like eyes",
            "miHoYo",
            "Genshin Impact",
            "Honkai: Star Rail",
            "You may study and reference that school",
            "lookalike",
            "找班",
            "Avoid flat two-tone cel shading",
            "material-specific highlights",
            "clean near-white studio backdrop",
            "emptiness is only the backdrop",
            "safe near-white gutter on both left and right",
            "layered couture grammar",
            COMPANION_VISUAL_SCHOOL_FINISH,
        ] {
            assert!(prompt.contains(expected), "missing {expected}: {prompt}");
        }
        assert!(
            !prompt.contains("without copying any existing character, costume, emblem, or franchise identity"),
            "image prompt must not use the old franchise-identity ban: {prompt}"
        );
    }

    #[test]
    fn maximum_visual_fields_cannot_cut_composition_constraints() {
        let long = "甲".repeat(1_200);
        let visual_identity = json!({
            "faceDesign": long,
            "eyeDesign": long,
            "hairShape": long,
            "hairLayerPlan": long,
            "upperBodySilhouette": long,
            "outfitConstruction": long,
            "sleeveArmDesign": long,
            "materialPlan": long,
            "heroAccessory": long,
            "paletteHint": long,
            "motif": long
        });
        let prompt = build_character_visual_prompt(
            "Nova",
            &json!({
                "gender": "unspecified",
                "extraRequirements": "丙".repeat(500),
                "visualIdentity": visual_identity
            }),
            Some(&"丁".repeat(2_000)),
        );
        assert!(prompt.chars().count() <= MAX_CHARACTER_VISUAL_PROMPT_CHARS);
        assert!(prompt.contains("Vertical 3:4 width-to-height canvas"));
        assert!(prompt.contains("Opaque finished illustration"));
        assert!(prompt.contains("safe near-white gutter on both left and right"));
        assert!(prompt.contains(COMPANION_VISUAL_SCHOOL_FINISH));
    }
}
