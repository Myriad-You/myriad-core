use serde_json::Value;

use crate::rig_contract::{PORTRAIT_ASPECT_HEIGHT, PORTRAIT_ASPECT_WIDTH};
use crate::visual_design::UPPER_BODY_VISUAL_IDENTITY_FIELDS;

const MAX_CHARACTER_VISUAL_PROMPT_CHARS: usize = 24_000;

const MASTER_PORTRAIT_INSTRUCTION: &str = "One upper-body portrait, face and torso toward camera, direct eye contact, calm closed mouth. Fill the 3:4 canvas: large head, face close and readable, shoulders almost the full width. Keep the entire head and complete hair silhouette inside the frame, including the crown, bangs, and side hair. Thin side air only — one-sixteenth of the canvas width on the LEFT and the same on the RIGHT, just enough that hair, sleeves, cuffs, and ornaments do not touch or leave either side. A slim near-white gutter above the hair. Do not pull the camera back. Do not shrink the figure into a stamp in the middle of white. Do not leave wide side panels. Do not cut or clip the head, crown, hair, sleeves, or ornaments. Frame from the top of the hair down through lower chest or high waist; both sleeves or short arm fragments visible; hands optional; never full-body, thighs, legs, or feet. Seamless near-white studio backdrop, no room, scenery, text, or watermark. Opaque finished illustration.";

const SPLASH_CONSTRUCTION: &str = "Fixed face and paint construction, not from identity fields. Crown to chin about half the canvas height. Each eye about one-quarter of the face height and taller than the nose and mouth together; multi-stop jewel iris; hard graphic catchlights; simple lids; no eye-socket depth, tear trough, or eyelid thickness. Tiny wedge nose, tiny graphic mouth, short small chin, large forehead, flat untextured skin with no pores, contour, or blush-modeling. Hair in stacked color masses with colored highlight sheets, not strand-by-strand painting. Costume in designed shapes with graphic folds and hard shine on cloth, metal, and gem. High chroma; dark clothes stay saturated, not gray or brown-muted. Not a muted painterly web portrait, not a realistic person, not fashion-illustration drapery.";

/// Paint-finish lock for design sheets and image prompts.
/// Costume, palette, and ornaments come from identity fields — not from this string.
pub const COMPANION_VISUAL_SCHOOL: &str = "Genshin Impact / Honkai: Star Rail 2D anime paint. 2D anime face, not a real face. Eyes about one-quarter of the face height and taller than the nose and mouth together; multi-stop jewel iris; hard graphic catchlights; simple lids with no eye-socket depth, tear trough, or eyelid thickness. Tiny wedge nose; tiny graphic mouth; short small chin; large forehead; flat untextured skin with no pores, contour, or blush-modeling. Hair as stacked color masses with colored highlight sheets, not strand-by-strand painting. Costume as designed game-outfit shapes with graphic folds and hard shine on cloth, metal, and gem. Warm key light, cool rim, hard material highlights. High chroma even on dark clothes. Not semi-realistic, not oil painting, not photoreal. Not generic web-illustration anime, not fashion illustration, not pixiv illustration, not a pretty illustration portrait, not a muted web illustration, not thick outlines, not flat cel, not in-game 3D";

const IDENTITY_PROMPT_FIELDS: &[(&str, &str)] = &[
    ("faceDesign", "Face color and expression only"),
    ("eyeDesign", "Eyes: iris, pupil, and catchlights"),
    ("hairShape", "Hair color and cut"),
    ("hairLayerPlan", "Hair groups"),
    ("upperBodySilhouette", "Silhouette"),
    ("outfitConstruction", "Costume"),
    ("heroAccessory", "Accessories: hero piece plus supporting ornaments, placements, and colors"),
    ("paletteHint", "Costume palette: main, secondary, accent on named parts"),
    ("sleeveArmDesign", "Sleeves"),
    ("materialPlan", "Costume materials"),
    ("motif", "Motif on garments and accessories"),
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
    let visual_identity = crate::visual_design::flatten_visual_identity(onboarding)
        .unwrap_or_else(|| onboarding.get("visualIdentity").cloned().unwrap_or(Value::Null));
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
    if let Some(style) = crate::visual_design::clothing_style_of(onboarding) {
        if let Some(grammar) = crate::visual_design::clothing_style_grammar(style) {
            push_section(
                &mut identity,
                "Costume language for garments and accessories",
                grammar,
            );
        }
    }
    for (key, label) in IDENTITY_PROMPT_FIELDS {
        let max_chars = field_limit(key);
        push_section(&mut identity, label, &text_at(&visual_identity, &[key], max_chars));
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

    let closer = "Hard lock: keep the fixed face construction and high-chroma 2D anime paint. Fill the 3:4 canvas. Character fields stay the same person. Outfit fields may change with a later costume swap. Garments and accessories stay in the costume language; do not add court crests or frog-button hardware unless that language asks for them. Notes cannot shrink the eyes, soften them into illustration eyes, sculpt a realistic face, add facial anatomy, switch to muted web illustration, or shrink the figure into empty side panels.";
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
        "Edit this existing upper-body master portrait. Keep the same character, face, hair, costume, palette, materials, ornaments, 3:4 fill, near-white studio backdrop, and only thin side air of one-sixteenth canvas width. Keep the entire head, hair, and both sleeves inside the frame. Do not pull the camera back or shrink the figure. Apply only these adjustments: {notes}. Do not invent a new character, cut the head, clip the sleeves, or turn this into a full-body shot."
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
    any_string_matches(value.get("visualIdentity").unwrap_or(value), style_lock_violation_in)
}

const LITERARY_SLUDGE: &[&str] = &[
    "质感",
    "美学",
    "余温",
    "藏锋",
    "证明存在",
    "取自",
    "像把",
    "像在",
    "在心里",
    "过夜",
    "才肯",
    "才出声",
    "写完才",
    "moonlight aesthetic",
];

pub fn literary_sludge_in(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    LITERARY_SLUDGE.iter().any(|ban| {
        if ban.is_ascii() {
            lower.contains(ban)
        } else {
            text.contains(ban)
        }
    })
}

pub fn visual_identity_has_literary_sludge(value: &Value) -> bool {
    any_string_matches(value.get("visualIdentity").unwrap_or(value), literary_sludge_in)
}

const PERSONA_LITERARY_SLUDGE: &[&str] = &[
    "质感",
    "美学",
    "余温",
    "藏锋",
    "证明存在",
    "取自",
    "像把",
    "像在",
    "在心里",
    "moonlight aesthetic",
];

pub fn persona_literary_sludge_in(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    PERSONA_LITERARY_SLUDGE.iter().any(|ban| {
        if ban.is_ascii() {
            lower.contains(ban)
        } else {
            text.contains(ban)
        }
    })
}

pub fn persona_has_literary_sludge(value: &Value) -> bool {
    any_string_matches(value.get("persona").unwrap_or(value), persona_literary_sludge_in)
}

fn any_string_matches(value: &Value, check: fn(&str) -> bool) -> bool {
    match value {
        Value::String(text) => check(text),
        Value::Object(fields) => fields.values().any(|field| any_string_matches(field, check)),
        Value::Array(items) => items.iter().any(|item| any_string_matches(item, check)),
        _ => false,
    }
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
                "clothingStyle": "idol",
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
            "Genshin Impact",
            "Honkai: Star Rail",
            "2D anime paint",
            "Not generic web-illustration anime",
            "Not semi-realistic",
            "2D anime face",
            "multi-stop jewel iris",
            "near-white studio backdrop",
            "gutter above the hair",
            "one-sixteenth of the canvas width",
            "Fill the 3:4 canvas",
            "Do not pull the camera back",
            "Do not shrink the figure",
            "do not touch or leave either side",
            "Do not cut or clip the head",
            "High chroma",
            "stacked color masses",
            "Hard lock",
            "Fixed face and paint construction",
            "Hair color and cut",
            "Face color and expression only",
            "Outfit fields may change",
            "Costume language for garments and accessories",
            "live-stage / performance wear",
            "not always a cropped jacket",
            "stay in the costume language",
            "court crests or frog-button hardware",
            "Not court ceremonial dress",
            "not a pretty illustration portrait",
            "high-chroma 2D anime paint",
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

        let modular_prompt = build_character_visual_prompt(
            "Nova",
            &json!({
                "visualIdentity": {
                    "character": {
                        "faceDesign": "refined oval face",
                        "eyeDesign": "layered gold jewel eyes",
                        "hairShape": "short silver bob",
                        "hairLayerPlan": "separate back mass, bangs, and side locks"
                    },
                    "outfit": {
                        "upperBodySilhouette": "compact shoulder and collar silhouette",
                        "outfitConstruction": "layered windcut coat and structured collar",
                        "sleeveArmDesign": "short side sleeve fragments at both edges",
                        "materialPlan": "matte cloth, silver metal, and restrained gem highlights",
                        "heroAccessory": "star-track chest clasp",
                        "paletteHint": "mist blue and silver",
                        "motif": "one restrained star-track arc"
                    }
                }
            }),
            None,
        );
        assert!(modular_prompt.contains("short silver bob"));
        assert!(modular_prompt.contains("layered windcut coat"));
        assert!(modular_prompt.contains("Outfit fields may change"));
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
        assert!(prompt.contains("gutter above the hair"));
        assert!(prompt.contains("one-sixteenth of the canvas width"));
        assert!(prompt.contains("Fill the 3:4 canvas"));
        assert!(!prompt.contains("official card"));
        assert!(!prompt.contains("wish card"));
        assert!(!prompt.contains("character-card"));
        assert!(prompt.contains("Do not cut or clip the head"));
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
        assert!(literary_sludge_in("色彩取自过夜的纸条"));
        assert!(literary_sludge_in("像把话在心里过完整才肯露面"));
        assert!(!literary_sludge_in("主色暖象牙，辅色墨青，强调色朱砂"));
        assert!(visual_identity_has_literary_sludge(&json!({
            "visualIdentity": {
                "paletteHint": "色彩取自叠在杯底过夜的纸条"
            }
        })));
        assert!(persona_literary_sludge_in("像把话在心里过完整才肯露面"));
        assert!(persona_literary_sludge_in("色彩取自叠在杯底过夜的纸条"));
        assert!(!persona_literary_sludge_in("先听，熟了才肯把句子拉长"));
        assert!(persona_has_literary_sludge(&json!({
            "persona": {
                "likes": ["色彩取自过夜的纸条"]
            }
        })));
        assert!(!persona_has_literary_sludge(&json!({
            "persona": {
                "likes": ["夜里听雨", "把桌面重新排好"]
            }
        })));
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
