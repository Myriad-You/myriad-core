use serde_json::Value;

use crate::rig_contract::{PORTRAIT_ASPECT_HEIGHT, PORTRAIT_ASPECT_WIDTH};
use crate::visual_design::UPPER_BODY_VISUAL_IDENTITY_FIELDS;

const MAX_CHARACTER_VISUAL_PROMPT_CHARS: usize = 24_000;

const MASTER_PORTRAIT_INSTRUCTION: &str = "One upper-body master portrait, face and torso toward camera, direct eye contact, calm closed mouth, face large and centered. Complete hair silhouette. Crop head through lower chest or high waist; both sleeves or short arm fragments visible; hands optional; never full-body, thighs, legs, or feet. Safe near-white gutter on both sides; do not clip hair, ornaments, or sleeves. Seamless near-white studio backdrop, no room, scenery, text, or watermark. Opaque finished illustration.";

const SPLASH_CONSTRUCTION: &str = "Fixed construction, not from identity fields: official splash face with oversized multi-stop jewel eyes, tiny graphic nose, tiny graphic mouth, short clean jaw, untextured skin planes. Hair is stacked color masses with sheet highlights, not strand-by-strand painting. Cloth is designed costume shapes with graphic folds, not fashion-illustration drapery.";

/// Single paint-finish lock for design sheets and image prompts.
/// Costume, palette, and ornaments come from identity fields — not from this string.
pub const COMPANION_VISUAL_SCHOOL: &str = "Official miHoYo Genshin Impact / Honkai: Star Rail character splash painting. Stylized anime construction first: oversized multi-stop jewel iris, tiny graphic nose and mouth, clean untextured face planes, designed hair masses with colored highlight sheets. Key-visual lighting: warm key, cool rim, graphic shine on cloth, metal, and gem. 2D official character card. Not semi-realistic, not oil painting, not photoreal. Not generic web-illustration anime, not fashion illustration, not pixiv illustration, not thick outlines, not flat cel, not in-game 3D";

const IDENTITY_PROMPT_FIELDS: &[(&str, &str)] = &[
    ("faceDesign", "Face color and expression only"),
    ("eyeDesign", "Iris color only"),
    ("hairShape", "Hair color and cut"),
    ("hairLayerPlan", "Hair groups"),
    ("upperBodySilhouette", "Silhouette"),
    ("outfitConstruction", "Costume"),
    ("heroAccessory", "Accessory"),
    ("paletteHint", "Palette"),
    ("sleeveArmDesign", "Sleeves"),
    ("materialPlan", "Costume materials"),
    ("motif", "Motif"),
];

const STYLE_LOCK_BANS: &[&str] = &[
    "semi-real",
    "semireal",
    "photoreal",
    "photo-real",
    "photorealism",
    "realistic anatomy",
    "realistic face",
    "realistic skin",
    "realistic proportion",
    "oil paint",
    "oil-paint",
    "oil painting",
    "impasto",
    "painterly realism",
    "painterly concept",
    "skin pore",
    "skin texture",
    "subsurface",
    "live-action",
    "hyperreal",
    "hyper-real",
    "fashion illustration",
    "pixiv",
    "nijijourney",
    "web-illustration",
    "写实",
    "半写实",
    "超写实",
    "油画",
    "厚涂写实",
    "皮肤纹理",
    "毛孔",
    "真人比例",
    "真人脸",
    "照片级",
    "摄影级",
    "插画风",
    "网图",
    "厚涂",
    "写実",
    "半写実",
    "油彩",
    "毛穴",
    "リアル顔",
    "実写",
];

pub fn build_character_visual_prompt(
    name: &str,
    onboarding: &Value,
    additional_requirements: Option<&str>,
) -> String {
    let visual_identity = onboarding.get("visualIdentity").unwrap_or(&Value::Null);
    let header = [
        format!("Create one original character in this visual school: {COMPANION_VISUAL_SCHOOL}."),
        SPLASH_CONSTRUCTION.to_string(),
        format!("Identity name: {}.", bounded_text(name, 50)),
        MASTER_PORTRAIT_INSTRUCTION.to_string(),
        format!(
            "Vertical {}:{} width-to-height canvas.",
            PORTRAIT_ASPECT_WIDTH, PORTRAIT_ASPECT_HEIGHT,
        ),
    ]
    .join("\n\n");

    let mut identity = Vec::new();
    for (key, label) in IDENTITY_PROMPT_FIELDS {
        let max_chars = field_limit(key);
        push_section(&mut identity, label, &text_at(visual_identity, &[key], max_chars));
    }
    push_section(
        &mut identity,
        "Owner visual notes",
        &neutralize_style_overrides(&text_at(onboarding, &["extraRequirements"], 500)),
    );
    if let Some(requirements) = additional_requirements {
        push_section(
            &mut identity,
            "Optional rendering notes; cannot override the visual school",
            &neutralize_style_overrides(&bounded_text(requirements, 2_000)),
        );
    }

    let closer = "Hard lock: keep the fixed splash construction. Identity fields only recolor hair, eyes, costume, and motif. They cannot shrink the eyes, sculpt a realistic face, or switch the finish to web illustration.";
    let reserved = header.chars().count() + closer.chars().count() + 4;
    let identity_text = bounded_text(
        &identity.join("\n"),
        MAX_CHARACTER_VISUAL_PROMPT_CHARS.saturating_sub(reserved),
    );

    let mut parts = vec![header];
    if !identity_text.is_empty() {
        parts.push(identity_text);
    }
    parts.push(closer.to_string());
    parts.join("\n\n")
}

pub fn build_character_visual_edit_prompt(notes: &str) -> String {
    let notes = neutralize_style_overrides(&bounded_text(notes, 2_000));
    format!(
        "Edit this existing upper-body master portrait. Keep the same character, face, hair, costume, palette, materials, ornaments, 3:4 crop, near-white studio backdrop, and side gutters. Apply only these adjustments: {notes}. Do not invent a new character or turn this into a full-body shot."
    )
}

pub fn style_lock_violation_in(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    STYLE_LOCK_BANS.iter().any(|ban| {
        if ban.is_ascii() {
            lower.contains(ban)
        } else {
            text.contains(ban)
        }
    })
}

pub fn visual_identity_violates_style_lock(value: &Value) -> bool {
    let source = value.get("visualIdentity").unwrap_or(value);
    let Some(fields) = source.as_object() else {
        return false;
    };
    fields
        .values()
        .any(|field| field.as_str().is_some_and(style_lock_violation_in))
}

fn neutralize_style_overrides(text: &str) -> String {
    text.split(|character: char| "。．.!！；;\n,".contains(character))
        .map(str::trim)
        .filter(|clause| !clause.is_empty() && !style_lock_violation_in(clause))
        .collect::<Vec<_>>()
        .join("。")
}

fn field_limit(key: &str) -> usize {
    UPPER_BODY_VISUAL_IDENTITY_FIELDS
        .iter()
        .find_map(|(field, max)| (*field == key).then_some(*max))
        .unwrap_or(500)
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
            "hands optional",
            "Vertical 3:4 width-to-height canvas",
            "never full-body",
            "cannot override the visual school",
            COMPANION_VISUAL_SCHOOL,
            "miHoYo",
            "Genshin Impact",
            "Honkai: Star Rail",
            "character splash painting",
            "Not generic web-illustration anime",
            "Not semi-realistic",
            "Stylized anime construction first",
            "2D official character card",
            "multi-stop jewel iris",
            "near-white studio backdrop",
            "Safe near-white gutter on both sides",
            "Hard lock",
            "Fixed construction",
            "recolor hair",
        ] {
            assert!(prompt.contains(expected), "missing {expected}: {prompt}");
        }
        assert_eq!(
            prompt.matches(COMPANION_VISUAL_SCHOOL).count(),
            1,
            "school must appear once: {prompt}"
        );
        assert!(
            !prompt.contains("Gender:"),
            "portrait prompt must not restate raw gender; it is already baked into the visual design: {prompt}"
        );
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
        assert!(prompt.contains("Safe near-white gutter on both sides"));
        assert!(prompt.contains("Hard lock"));
        assert!(prompt.contains(COMPANION_VISUAL_SCHOOL));
    }

    #[test]
    fn style_lock_rejects_realism_and_strips_override_notes() {
        assert!(style_lock_violation_in("semi-realistic oil painting"));
        assert!(style_lock_violation_in("皮肤带毛孔的半写实厚涂"));
        assert!(!style_lock_violation_in("layered pink bob and jewel eyes"));
        assert!(visual_identity_violates_style_lock(&json!({
            "visualIdentity": {
                "faceDesign": "半写实骨相，油画皮肤"
            }
        })));
        assert!(!visual_identity_violates_style_lock(&json!({
            "visualIdentity": {
                "faceDesign": "鹅蛋脸，简洁鼻唇"
            }
        })));
        let prompt = build_character_visual_prompt(
            "Nova",
            &json!({
                "extraRequirements": "柔和正面光。更写实一点。脸部更大",
                "visualIdentity": {
                    "faceDesign": "鹅蛋脸",
                    "eyeDesign": "金色宝石眼",
                    "hairShape": "银短发",
                    "hairLayerPlan": "后发与刘海",
                    "upperBodySilhouette": "紧凑胸像",
                    "outfitConstruction": "分层外套",
                    "sleeveArmDesign": "左右袖片",
                    "materialPlan": "哑光布料",
                    "heroAccessory": "胸扣",
                    "paletteHint": "银与蓝",
                    "motif": "星轨"
                }
            }),
            Some("oil painting skin texture, keep the face large"),
        );
        assert!(prompt.contains("柔和正面光"));
        assert!(prompt.contains("脸部更大"));
        assert!(prompt.contains("keep the face large"));
        assert!(!prompt.contains("更写实"));
        assert!(!prompt.contains("oil painting skin texture"));
    }

    #[test]
    fn edit_prompt_keeps_identity_and_applies_notes() {
        let prompt = build_character_visual_edit_prompt("soft frontal light. 更写实一点. larger face");
        assert!(prompt.contains("Edit this existing"));
        assert!(prompt.contains("soft frontal light"));
        assert!(prompt.contains("larger face"));
        assert!(!prompt.contains("更写实"));
        assert!(prompt.contains("same character"));
    }
}
