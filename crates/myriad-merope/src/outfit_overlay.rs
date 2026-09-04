//! Chat-only temporary outfit overlay.
//!
//! Picks an already-saved wardrobe item from the addressee's line. It never
//! writes `activeOutfitId` or the live rig pointer.

use serde_json::Value;

use crate::visual_design::DEFAULT_WARDROBE_ID;

const DEFAULT_WARDROBE_LABEL: &str = "默认服装";

const GENERIC_HINTS: &[&str] = &[
    "衣服", "服装", "衣装", "外套", "上衣", "那套", "这套", "一件", "一套", "outfit", "clothes",
    "clothing", "dress", "wear", "coat",
];

const CHANGE_PHRASES: &[&str] = &[
    "换上",
    "换成",
    "换一件",
    "换一套",
    "换一下",
    "换衣服",
    "换装",
    "穿上",
    "改穿",
    "着替えて",
    "着替",
    "服を換えて",
    "服を着て",
];

const CHANGE_LATIN: &[&str] = &[
    "change into",
    "switch outfit",
    "change outfit",
    "put on",
    "wear",
];

const REVERT_PHRASES: &[&str] = &[
    "换回来",
    "换回去",
    "换回原来",
    "换回默认",
    "穿回来",
    "穿回原来",
    "恢复原来",
    "换回日常",
    "元に戻",
    "いつもの服",
];

const REVERT_LATIN: &[&str] = &[
    "change back",
    "original outfit",
    "default outfit",
    "default clothes",
    "usual clothes",
];

const OTHER_PHRASES: &[&str] = &["别的", "其他", "随便", "另一套", "別の"];
const OTHER_LATIN: &[&str] = &["another", "something else", "different one"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeLook {
    pub id: String,
    pub label: String,
    pub clothing_style: String,
    pub portrait_asset_id: Option<String>,
    pub rig_asset_id: Option<String>,
    pub generation_fingerprint: Option<String>,
    pub hints: Vec<String>,
}

impl WardrobeLook {
    pub fn playable(&self) -> bool {
        self.portrait_asset_id.is_some() || self.rig_asset_id.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayDecision<'a> {
    Unchanged,
    Clear,
    Wear(&'a str),
}

pub fn worn_outfit_id(profile: &Value) -> Option<&str> {
    profile.get("activeOutfitId").and_then(Value::as_str)
}

pub fn wardrobe_look<'a>(looks: &'a [WardrobeLook], id: &str) -> Option<&'a WardrobeLook> {
    looks.iter().find(|look| look.id == id)
}

pub fn looks_from_visual_profile(profile: &Value) -> Vec<WardrobeLook> {
    let Some(items) = profile.get("wardrobe").and_then(Value::as_array) else {
        return Vec::new();
    };
    items.iter().filter_map(look_from_item).collect()
}

fn look_from_item(item: &Value) -> Option<WardrobeLook> {
    let id = item.get("id").and_then(Value::as_str)?.trim();
    if id.is_empty() {
        return None;
    }
    let clothing_style = item.get("clothingStyle").and_then(Value::as_str)?.trim();
    if clothing_style.is_empty() {
        return None;
    }
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let label = if id == DEFAULT_WARDROBE_ID {
        DEFAULT_WARDROBE_LABEL.to_string()
    } else {
        name.unwrap_or(clothing_style).to_string()
    };
    let construction = item
        .get("outfit")
        .and_then(|outfit| outfit.get("outfitConstruction"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut hints = Vec::new();
    push_hint(&mut hints, &label);
    if let Some(name) = name {
        push_hint(&mut hints, name);
    }
    push_hint(&mut hints, clothing_style);
    for alias in clothing_style_aliases(clothing_style) {
        push_hint(&mut hints, alias);
    }
    if id == DEFAULT_WARDROBE_ID {
        for alias in ["默认", "default outfit", "default"] {
            push_hint(&mut hints, alias);
        }
    }
    push_hint(&mut hints, construction);
    Some(WardrobeLook {
        id: id.to_string(),
        label,
        clothing_style: clothing_style.to_string(),
        portrait_asset_id: string_field(item, "portraitAssetId"),
        rig_asset_id: string_field(item, "rigAssetId"),
        generation_fingerprint: string_field(item, "generationFingerprint"),
        hints,
    })
}

fn string_field(item: &Value, key: &str) -> Option<String> {
    item.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn push_hint(hints: &mut Vec<String>, raw: &str) {
    let value = raw.trim();
    if value.is_empty() {
        return;
    }
    if !hints.iter().any(|existing| existing == value) {
        hints.push(value.to_string());
    }
}

fn clothing_style_aliases(style: &str) -> &'static [&'static str] {
    match style {
        "everyday" => &["日常", "便服", "casual"],
        "uniform" => &["校服", "制服"],
        "fantasy" => &["幻想", "奇幻"],
        "urban" => &["都市", "街头"],
        "east-asian" => &["国风", "中式"],
        "japanese" => &["和风", "着物"],
        "sci-fi" => &["科幻"],
        "formal" => &["正装", "礼服"],
        "sport" => &["运动"],
        "idol" => &["偶像"],
        "gothic" => &["哥特"],
        "lounge" => &["居家", "睡衣"],
        "royal" => &["宫廷"],
        "mystic" => &["神秘"],
        "travel" => &["旅行"],
        "vintage" => &["复古"],
        "rain" => &["雨衣", "雨天"],
        _ => &[],
    }
}

pub fn format_chat_wardrobe_section(
    looks: &[WardrobeLook],
    worn_id: &str,
    overlay_id: Option<&str>,
) -> Option<String> {
    if looks.is_empty() {
        return None;
    }
    let showing_id = overlay_id.unwrap_or(worn_id);
    let showing = wardrobe_look(looks, showing_id)
        .map(|look| look.label.as_str())
        .unwrap_or(showing_id);
    let names = looks
        .iter()
        .map(|look| look.label.as_str())
        .collect::<Vec<_>>()
        .join("、");
    let wearing = if overlay_id.is_some() && overlay_id != Some(worn_id) {
        format!("这一轮你穿着：{showing}（聊天里临时换的，不是正在穿着的那套）")
    } else {
        format!("这一轮你穿着：{showing}")
    };
    Some(format!(
        "## 衣服\n{wearing}\n衣柜里有：{names}\n\
         对方让你换衣服时按性格接话。换装由现场处理，不要念清单或编号，也不要说你做不到换衣服。"
    ))
}

pub fn resolve_chat_outfit_overlay<'a>(
    input: &str,
    looks: &'a [WardrobeLook],
    worn_id: &str,
    current_overlay: Option<&str>,
) -> OverlayDecision<'a> {
    let showing = current_overlay.unwrap_or(worn_id);
    if has_revert_intent(input) {
        return finalize(OverlayDecision::Clear, worn_id, current_overlay);
    }
    if !has_change_intent(input) {
        return OverlayDecision::Unchanged;
    }
    if wants_other(input) {
        let others: Vec<&WardrobeLook> = looks
            .iter()
            .filter(|look| look.playable() && look.id != showing)
            .collect();
        return if others.len() == 1 {
            finalize(
                OverlayDecision::Wear(others[0].id.as_str()),
                worn_id,
                current_overlay,
            )
        } else {
            OverlayDecision::Unchanged
        };
    }
    let mut best: Option<(&WardrobeLook, u32)> = None;
    let mut tied = false;
    for look in looks.iter().filter(|look| look.playable()) {
        let score = score_look(input, look, looks);
        if score == 0 {
            continue;
        }
        match best {
            None => best = Some((look, score)),
            Some((_, best_score)) if score > best_score => {
                best = Some((look, score));
                tied = false;
            }
            Some((_, best_score)) if score == best_score => tied = true,
            Some(_) => {}
        }
    }
    if tied {
        return OverlayDecision::Unchanged;
    }
    match best {
        Some((look, _)) => finalize(
            OverlayDecision::Wear(look.id.as_str()),
            worn_id,
            current_overlay,
        ),
        None => OverlayDecision::Unchanged,
    }
}

fn finalize<'a>(
    decision: OverlayDecision<'a>,
    worn_id: &str,
    current_overlay: Option<&str>,
) -> OverlayDecision<'a> {
    match decision {
        OverlayDecision::Wear(id) if id == worn_id => {
            if current_overlay.is_none() {
                OverlayDecision::Unchanged
            } else {
                OverlayDecision::Clear
            }
        }
        OverlayDecision::Wear(id) if current_overlay == Some(id) => OverlayDecision::Unchanged,
        OverlayDecision::Clear if current_overlay.is_none() => OverlayDecision::Unchanged,
        other => other,
    }
}

fn has_change_intent(input: &str) -> bool {
    CHANGE_PHRASES.iter().any(|phrase| input.contains(phrase))
        || CHANGE_LATIN
            .iter()
            .any(|phrase| contains_latin_phrase(input, phrase))
}

fn has_revert_intent(input: &str) -> bool {
    REVERT_PHRASES.iter().any(|phrase| input.contains(phrase))
        || REVERT_LATIN
            .iter()
            .any(|phrase| contains_latin_phrase(input, phrase))
}

fn wants_other(input: &str) -> bool {
    OTHER_PHRASES.iter().any(|phrase| input.contains(phrase))
        || OTHER_LATIN
            .iter()
            .any(|phrase| contains_latin_phrase(input, phrase))
}

fn contains_latin_phrase(input: &str, phrase: &str) -> bool {
    let haystack = latin_folded(input);
    let needle = latin_folded(phrase);
    if needle.is_empty() {
        return false;
    }
    if needle.contains(' ') {
        return haystack.contains(&needle);
    }
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == needle)
}

fn latin_folded(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn score_look(input: &str, look: &WardrobeLook, all: &[WardrobeLook]) -> u32 {
    let mut score = 0;
    if contains_hint(input, &look.label) {
        score += 100;
    }
    if contains_hint(input, &look.clothing_style) {
        score += 40;
    }
    for hint in &look.hints {
        if !contains_hint(input, hint) {
            continue;
        }
        if is_generic_hint(hint) {
            continue;
        }
        if !hint_is_unique(hint, look.id.as_str(), all) && hint != &look.label {
            continue;
        }
        score += if hint == &look.label { 0 } else { 50 };
    }
    score
}

fn contains_hint(input: &str, hint: &str) -> bool {
    let hint = hint.trim();
    if hint.chars().count() < 2 {
        return false;
    }
    if hint.chars().all(|ch| ch.is_ascii()) {
        return contains_latin_phrase(input, hint);
    }
    input.contains(hint)
}

fn hint_is_unique(hint: &str, owner_id: &str, all: &[WardrobeLook]) -> bool {
    all.iter()
        .filter(|other| other.id != owner_id)
        .all(|other| !look_mentions(other, hint))
}

fn look_mentions(look: &WardrobeLook, hint: &str) -> bool {
    contains_hint(&look.label, hint)
        || look
            .hints
            .iter()
            .any(|candidate| contains_hint(candidate, hint))
}

fn is_generic_hint(hint: &str) -> bool {
    let folded = hint.trim();
    GENERIC_HINTS
        .iter()
        .any(|generic| folded.eq_ignore_ascii_case(generic))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn coat() -> WardrobeLook {
        WardrobeLook {
            id: "w-coat".into(),
            label: "冬日大衣".into(),
            clothing_style: "urban".into(),
            portrait_asset_id: Some("/uploads/coat.png".into()),
            rig_asset_id: None,
            generation_fingerprint: None,
            hints: vec![
                "冬日大衣".into(),
                "urban".into(),
                "都市".into(),
                "高领内搭叠短大衣".into(),
            ],
        }
    }

    fn uniform() -> WardrobeLook {
        WardrobeLook {
            id: "w-uniform".into(),
            label: "校服".into(),
            clothing_style: "uniform".into(),
            portrait_asset_id: Some("/uploads/uniform.png".into()),
            rig_asset_id: None,
            generation_fingerprint: None,
            hints: vec!["校服".into(), "uniform".into(), "水手领内搭叠短外套".into()],
        }
    }

    fn default_look() -> WardrobeLook {
        WardrobeLook {
            id: DEFAULT_WARDROBE_ID.into(),
            label: DEFAULT_WARDROBE_LABEL.into(),
            clothing_style: "everyday".into(),
            portrait_asset_id: Some("/uploads/default.png".into()),
            rig_asset_id: None,
            generation_fingerprint: None,
            hints: vec![
                DEFAULT_WARDROBE_LABEL.into(),
                "默认".into(),
                "everyday".into(),
                "日常".into(),
            ],
        }
    }

    fn catalog() -> Vec<WardrobeLook> {
        vec![default_look(), coat(), uniform()]
    }

    #[test]
    fn naming_an_outfit_without_a_change_verb_does_not_switch() {
        assert_eq!(
            resolve_chat_outfit_overlay("这件冬日大衣真好看", &catalog(), "default", None),
            OverlayDecision::Unchanged
        );
    }

    #[test]
    fn a_named_change_request_wears_that_set() {
        assert_eq!(
            resolve_chat_outfit_overlay("换上冬日大衣", &catalog(), "default", None),
            OverlayDecision::Wear("w-coat")
        );
        assert_eq!(
            resolve_chat_outfit_overlay(
                "Can you change into the uniform?",
                &catalog(),
                "default",
                None
            ),
            OverlayDecision::Wear("w-uniform")
        );
    }

    #[test]
    fn change_back_clears_the_overlay_instead_of_wearing_default() {
        assert_eq!(
            resolve_chat_outfit_overlay("换回来", &catalog(), "w-coat", Some("w-uniform")),
            OverlayDecision::Clear
        );
        assert_eq!(
            resolve_chat_outfit_overlay("换回原来的衣服", &catalog(), "w-coat", Some("w-uniform")),
            OverlayDecision::Clear
        );
    }

    #[test]
    fn wearing_the_persisted_set_clears_a_temporary_overlay() {
        assert_eq!(
            resolve_chat_outfit_overlay("换上默认服装", &catalog(), "default", Some("w-coat")),
            OverlayDecision::Clear
        );
        assert_eq!(
            resolve_chat_outfit_overlay("换上默认服装", &catalog(), "default", None),
            OverlayDecision::Unchanged
        );
    }

    #[test]
    fn another_outfit_picks_the_only_other_playable_set() {
        let two = vec![default_look(), coat()];
        assert_eq!(
            resolve_chat_outfit_overlay("换一套别的", &two, "default", None),
            OverlayDecision::Wear("w-coat")
        );
        assert_eq!(
            resolve_chat_outfit_overlay("换一套别的", &catalog(), "default", None),
            OverlayDecision::Unchanged
        );
    }

    #[test]
    fn unplayable_sets_are_not_candidates() {
        let mut empty_coat = coat();
        empty_coat.portrait_asset_id = None;
        empty_coat.rig_asset_id = None;
        let looks = vec![default_look(), empty_coat];
        assert_eq!(
            resolve_chat_outfit_overlay("换上冬日大衣", &looks, "default", None),
            OverlayDecision::Unchanged
        );
    }

    #[test]
    fn repeating_the_current_overlay_is_unchanged() {
        assert_eq!(
            resolve_chat_outfit_overlay("换上冬日大衣", &catalog(), "default", Some("w-coat")),
            OverlayDecision::Unchanged
        );
    }

    #[test]
    fn looks_read_saved_wardrobe_items_and_the_prompt_hides_ids() {
        let profile = json!({
            "activeOutfitId": "default",
            "wardrobe": [
                {
                    "id": "default",
                    "clothingStyle": "everyday",
                    "portraitAssetId": "/uploads/default.png",
                    "outfit": { "outfitConstruction": "水手领内搭叠短外套" }
                },
                {
                    "id": "w-coat",
                    "name": "冬日大衣",
                    "clothingStyle": "urban",
                    "portraitAssetId": "/uploads/coat.png",
                    "outfit": { "outfitConstruction": "高领内搭叠短大衣" }
                }
            ]
        });
        let looks = looks_from_visual_profile(&profile);
        assert_eq!(looks.len(), 2);
        assert_eq!(looks[0].label, "默认服装");
        assert_eq!(looks[1].id, "w-coat");
        assert_eq!(worn_outfit_id(&profile), Some("default"));
        let section = format_chat_wardrobe_section(&looks, "default", Some("w-coat")).unwrap();
        assert!(section.contains("冬日大衣（聊天里临时换的"));
        assert!(section.contains("默认服装、冬日大衣"));
        assert!(!section.contains("w-coat"));
        assert!(!section.contains("activeOutfitId"));
        assert!(!section.contains("JSON"));
    }
}
