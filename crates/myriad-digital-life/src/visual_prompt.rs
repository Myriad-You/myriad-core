use serde_json::Value;

use crate::CharacterVisualSlot;
use crate::rig_contract::{PORTRAIT_ASPECT_HEIGHT, PORTRAIT_ASPECT_WIDTH};

/// Locked visual school for companion name, costume, and image generation.
/// Onboarding prompts and `build_character_visual_prompt` must share this copy.
pub const COMPANION_VISUAL_SCHOOL: &str = "original high-detail Japanese anime-game character portrait, polished commercial visual-novel and VTuber key art, crisp controlled linework, luminous soft cel-paint rendering, refined non-chibi proportions";

pub fn build_character_visual_prompt(
    name: &str,
    persona: &Value,
    onboarding: &Value,
    slot: CharacterVisualSlot,
    additional_requirements: Option<&str>,
) -> String {
    let summary = text_at(persona, &["summary"], 1_200);
    let temperament = list_at(persona, &["temperament", "traits"], 12, 120);
    let gender = text_at(onboarding, &["gender"], 32);
    let onboarding_extra = text_at(onboarding, &["extraRequirements"], 500);
    let outfit = onboarding.get("outfitDesign").unwrap_or(&Value::Null);
    let visual_identity = onboarding
        .get("visualIdentity")
        .or_else(|| persona.get("visualIdentity"))
        .unwrap_or(&Value::Null);
    let hair = first_non_empty([
        text_at(onboarding, &["hairShape"], 500),
        text_at(visual_identity, &["hairShape"], 500),
    ]);
    let outfit_title = text_at(outfit, &["titleZh"], 120);
    let outfit_design = text_at(outfit, &["designZh"], 1_200);
    let outfit_layers = text_at(outfit, &["layersEn"], 1_200);
    let outfit_construction = text_at(visual_identity, &["outfitConstruction"], 1_200);
    let accessory = first_non_empty([
        text_at(outfit, &["heroAccessoryZh"], 500),
        text_at(visual_identity, &["heroAccessory"], 500),
    ]);
    let palette = first_non_empty([
        text_at(outfit, &["paletteHintZh"], 500),
        text_at(visual_identity, &["paletteHint"], 500),
    ]);

    let mut sections = vec![
        format!("Create one polished {COMPANION_VISUAL_SCHOOL} digital companion character. Character only: no room, no furniture, no scenery, no environment, no vehicle, no text, no watermark."),
        format!("Identity name: {}.", bounded_text(name, 50)),
    ];
    push_section(&mut sections, "Persona", &summary);
    push_section(&mut sections, "Temperament", &temperament.join(", "));
    push_section(&mut sections, "Gender presentation", &gender);
    push_section(&mut sections, "Hair identity lock", &hair);
    push_section(&mut sections, "Selected costume", &outfit_title);
    push_section(&mut sections, "Costume design lock", &outfit_design);
    push_section(
        &mut sections,
        "Costume construction lock",
        &first_non_empty([outfit_layers, outfit_construction]),
    );
    push_section(&mut sections, "Hero accessory lock", &accessory);
    push_section(&mut sections, "Palette lock", &palette);
    push_section(
        &mut sections,
        "Additional character requirements",
        &onboarding_extra,
    );
    if let Some(requirements) = additional_requirements {
        push_section(
            &mut sections,
            "User-provided generation refinement",
            &bounded_text(requirements, 2_000),
        );
    }

    if slot.requires_master() {
        sections.push(
            "Use the attached current master portrait as the authoritative identity reference. Preserve its face, hair silhouette, costume topology, signature accessory, palette, and proportions; change only the camera direction or sheet layout required below."
                .to_string(),
        );
    }
    sections.push(slot_instruction(slot).to_string());
    if !matches!(slot, CharacterVisualSlot::RigTurnaround) {
        sections.push(format!(
            "Use a vertical {}:{} width-to-height canvas (height-to-width {}:{}). Keep the full character silhouette inside that exact portrait canvas without stretching or later reframing.",
            PORTRAIT_ASPECT_WIDTH,
            PORTRAIT_ASPECT_HEIGHT,
            PORTRAIT_ASPECT_HEIGHT,
            PORTRAIT_ASPECT_WIDTH,
        ));
    }
    sections.push(
        "Preserve the exact face, hairstyle, upper-body costume construction, accessory placement, palette, and identity across every generated character asset. Use the same close upper-body composition as a premium character portrait: full hair and head safely inside the top and side margins, face large and centered, shoulders and chest fully readable, cropped between lower chest and high waist. Show both shoulder lines and enough outer sleeve or partial arm silhouette on the left and right edges for later rigid-layer separation; hands and a complete shoulder-to-hand anatomy are not required. Keep the two side fragments visually separable from the torso and from each other with clear overlap-safe boundaries. Do not pose or segment shoulder, elbow, wrist, or fingers as an articulated limb chain. Keep hair, face, eyes, brows, mouth, neck, topwear, chest, accessories, and side sleeve layers visually separable. Do not generate visible legs or feet. Do not collapse the selected costume into generic streetwear or a plain sweater."
            .to_string(),
    );
    bounded_text(&sections.join(" "), 8_000)
}

fn slot_instruction(slot: CharacterVisualSlot) -> &'static str {
    match slot {
        CharacterVisualSlot::Master => {
            "Output one finished upper-body master portrait, front-facing and nearly symmetrical, with direct eye contact and a calm subtle closed-mouth smile. Use a close head-and-torso crop like premium anime character key art: full hair visible, shoulders and chest visible, lower edge between lower chest and high waist, with outer sleeves or partial arms entering naturally from both side edges. Hands do not need to be visible. Render the character over a seamless clean near-white studio background with no environment, props, floor, border, text, or watermark. The master portrait is an opaque finished illustration; transparent layers are produced later by the decomposition stage."
        }
        CharacterVisualSlot::RigTurnaround => {
            "Output a character-only upper-body turnaround sheet with front, three-quarter, side, and back high-waist views in neutral poses. Keep partial forearms and hands visible where the camera allows, and keep every view at identical scale and costume construction on a transparent background with clean alpha; no labels, legs, or feet."
        }
        CharacterVisualSlot::DirectionSouth => direction_instruction("front / south"),
        CharacterVisualSlot::DirectionSouthwest => {
            direction_instruction("front-left / southwest")
        }
        CharacterVisualSlot::DirectionWest => direction_instruction("left profile / west"),
        CharacterVisualSlot::DirectionNorthwest => {
            direction_instruction("back-left / northwest")
        }
        CharacterVisualSlot::DirectionNorth => direction_instruction("back / north"),
        CharacterVisualSlot::DirectionNortheast => {
            direction_instruction("back-right / northeast")
        }
        CharacterVisualSlot::DirectionEast => direction_instruction("right profile / east"),
        CharacterVisualSlot::DirectionSoutheast => {
            direction_instruction("front-right / southeast")
        }
    }
}

fn direction_instruction(direction: &str) -> &'static str {
    match direction {
        "front / south" => "Output one upper-body character asset viewed from front / south, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "front-left / southwest" => "Output one upper-body character asset viewed from front-left / southwest, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "left profile / west" => "Output one upper-body character asset viewed from left profile / west, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "back-left / northwest" => "Output one upper-body character asset viewed from back-left / northwest, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "back / north" => "Output one upper-body character asset viewed from back / north, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "back-right / northeast" => "Output one upper-body character asset viewed from back-right / northeast, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        "right profile / east" => "Output one upper-body character asset viewed from right profile / east, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
        _ => "Output one upper-body character asset viewed from front-right / southeast, framed from head through high waist with partial forearms and hands visible on a transparent background with clean alpha.",
    }
}

fn push_section(sections: &mut Vec<String>, label: &str, value: &str) {
    if !value.is_empty() {
        sections.push(format!("{label}: {value}."));
    }
}

fn first_non_empty<const N: usize>(values: [String; N]) -> String {
    values
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn text_at(value: &Value, keys: &[&str], max_chars: usize) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(|value| bounded_text(value, max_chars))
        .unwrap_or_default()
}

fn list_at(value: &Value, keys: &[&str], max_items: usize, max_chars: usize) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| bounded_text(item, max_chars))
                .filter(|item| !item.is_empty())
                .take(max_items)
                .collect()
        })
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
                "summary": "Quiet, curious, and careful with trust.",
                "traits": ["calm", "curious"],
                "visualIdentity": { "hairShape": "short silver bob" }
            }),
            &json!({
                "gender": "nonbinary",
                "extraRequirements": "gold eyes",
                "outfitDesign": {
                    "titleZh": "星轨风裁",
                    "designZh": "灰蓝分层风衣叠精密腰封",
                    "layersEn": "layered windcut coat and structured boots",
                    "heroAccessoryZh": "胸口星轨扣饰",
                    "paletteHintZh": "雾蓝 / 银"
                }
            }),
            CharacterVisualSlot::Master,
            Some("soft cel shading"),
        );
        for expected in [
            "short silver bob",
            "layered windcut coat",
            "胸口星轨扣饰",
            "gold eyes",
            "soft cel shading",
            "no room",
            "Hands do not need to be visible",
            "vertical 3:4 width-to-height canvas",
            "later rigid-layer separation",
            "Do not generate visible legs or feet",
            COMPANION_VISUAL_SCHOOL,
            "refined non-chibi proportions",
            "clean near-white studio background",
        ] {
            assert!(prompt.contains(expected), "missing {expected}: {prompt}");
        }
    }

    #[test]
    fn derived_visual_prompt_keeps_identity_and_direction() {
        let prompt = build_character_visual_prompt(
            "Nova",
            &json!({}),
            &json!({}),
            CharacterVisualSlot::DirectionNorthwest,
            None,
        );
        assert!(prompt.contains("back-left / northwest"));
        assert!(prompt.contains("Preserve the exact face"));
        assert!(prompt.contains("attached current master portrait"));
        assert!(prompt.contains(COMPANION_VISUAL_SCHOOL));
    }
}
