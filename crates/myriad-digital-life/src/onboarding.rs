use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{report_dna_json, ReportDnaProvenance};
use crate::rig_contract::{PORTRAIT_GENERATION_HEIGHT, PORTRAIT_GENERATION_WIDTH};

pub const DNA_SCHEMA_VERSION: u8 = 7;
/// Step-1 bubble pool + user selection cap (aligned with experimental 12–20 AI pool / ≤28 picks).
pub const MAX_ONBOARDING_TAGS: usize = 28;
pub const MAX_ONBOARDING_TAG_CHARS: usize = 24;
pub const ONBOARDING_SCHEMA_VERSION: u8 = 2;
pub const TOTAL_ONBOARDING_STEPS: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CharacterVisualSlot {
    Master,
    RigTurnaround,
    DirectionSouth,
    DirectionSouthwest,
    DirectionWest,
    DirectionNorthwest,
    DirectionNorth,
    DirectionNortheast,
    DirectionEast,
    DirectionSoutheast,
}

impl CharacterVisualSlot {
    pub const DERIVED: [Self; 9] = [
        Self::RigTurnaround,
        Self::DirectionSouth,
        Self::DirectionSouthwest,
        Self::DirectionWest,
        Self::DirectionNorthwest,
        Self::DirectionNorth,
        Self::DirectionNortheast,
        Self::DirectionEast,
        Self::DirectionSoutheast,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::RigTurnaround => "rig-turnaround",
            Self::DirectionSouth => "direction-south",
            Self::DirectionSouthwest => "direction-southwest",
            Self::DirectionWest => "direction-west",
            Self::DirectionNorthwest => "direction-northwest",
            Self::DirectionNorth => "direction-north",
            Self::DirectionNortheast => "direction-northeast",
            Self::DirectionEast => "direction-east",
            Self::DirectionSoutheast => "direction-southeast",
        }
    }

    pub const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::RigTurnaround => (1536, 1024),
            _ => (PORTRAIT_GENERATION_WIDTH, PORTRAIT_GENERATION_HEIGHT),
        }
    }

    pub const fn requires_master(self) -> bool {
        !matches!(self, Self::Master)
    }
}

impl std::str::FromStr for CharacterVisualSlot {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "master" => Ok(Self::Master),
            "rig-turnaround" => Ok(Self::RigTurnaround),
            "direction-south" => Ok(Self::DirectionSouth),
            "direction-southwest" => Ok(Self::DirectionSouthwest),
            "direction-west" => Ok(Self::DirectionWest),
            "direction-northwest" => Ok(Self::DirectionNorthwest),
            "direction-north" => Ok(Self::DirectionNorth),
            "direction-northeast" => Ok(Self::DirectionNortheast),
            "direction-east" => Ok(Self::DirectionEast),
            "direction-southeast" => Ok(Self::DirectionSoutheast),
            _ => Err("unsupported character visual slot"),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingPatch {
    pub step: Option<u8>,
    pub completed: Option<bool>,
    pub selected_tags: Option<Vec<String>>,
    pub gender: Option<String>,
    pub extra_requirements: Option<String>,
    pub display_name: Option<String>,
    pub outfit_design: Option<Value>,
    pub report_signal_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingPatchError {
    InvalidStep,
    InvalidGender,
    InvalidDisplayName,
    InvalidExtraRequirements,
    InvalidOutfitDesign,
}

pub fn initial_onboarding_state(
    display_name: &str,
    tags: &[String],
    gender: Option<&str>,
    extra_requirements: Option<&str>,
    outfit_design: Option<Value>,
    report_signal_fingerprint: Option<&str>,
) -> Result<Value, OnboardingPatchError> {
    apply_onboarding_patch(
        &json!({
            "schemaVersion": ONBOARDING_SCHEMA_VERSION,
            "step": 3,
            "completed": false,
        }),
        OnboardingPatch {
            step: Some(3),
            selected_tags: Some(tags.to_vec()),
            gender: gender.map(str::to_string),
            extra_requirements: extra_requirements.map(str::to_string),
            display_name: Some(display_name.to_string()),
            outfit_design,
            report_signal_fingerprint: report_signal_fingerprint.map(str::to_string),
            ..OnboardingPatch::default()
        },
    )
}

pub fn apply_onboarding_patch(
    current: &Value,
    patch: OnboardingPatch,
) -> Result<Value, OnboardingPatchError> {
    let mut state = current.as_object().cloned().unwrap_or_default();
    state.insert("schemaVersion".into(), json!(ONBOARDING_SCHEMA_VERSION));

    if let Some(step) = patch.step {
        if !(1..=TOTAL_ONBOARDING_STEPS).contains(&step) {
            return Err(OnboardingPatchError::InvalidStep);
        }
        state.insert("step".into(), json!(step));
    }
    if let Some(completed) = patch.completed {
        state.insert("completed".into(), json!(completed));
        if completed {
            state.insert("step".into(), json!(TOTAL_ONBOARDING_STEPS));
        }
    }
    if let Some(tags) = patch.selected_tags {
        state.insert("selectedTags".into(), json!(sanitize_onboarding_tags(&tags)));
    }
    if let Some(gender) = patch.gender {
        if !matches!(gender.as_str(), "female" | "male" | "nonbinary" | "unspecified") {
            return Err(OnboardingPatchError::InvalidGender);
        }
        state.insert("gender".into(), json!(gender));
    }
    if let Some(display_name) = patch.display_name {
        let display_name = sanitize_scalar(&display_name, 50)
            .ok_or(OnboardingPatchError::InvalidDisplayName)?;
        state.insert("displayName".into(), json!(display_name));
    }
    if let Some(extra_requirements) = patch.extra_requirements {
        if extra_requirements.chars().any(char::is_control)
            || extra_requirements.chars().count() > 500
        {
            return Err(OnboardingPatchError::InvalidExtraRequirements);
        }
        state.insert(
            "extraRequirements".into(),
            json!(extra_requirements.trim()),
        );
    }
    if let Some(outfit_design) = patch.outfit_design {
        state.insert("outfitDesign".into(), sanitize_outfit_design(outfit_design)?);
    }
    if let Some(fingerprint) = patch.report_signal_fingerprint {
        let fingerprint = sanitize_scalar(&fingerprint, 128)
            .ok_or(OnboardingPatchError::InvalidExtraRequirements)?;
        state.insert("reportSignalFingerprint".into(), json!(fingerprint));
    }
    Ok(Value::Object(state))
}

fn sanitize_scalar(value: &str, max_chars: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_string())
}

fn sanitize_outfit_design(value: Value) -> Result<Value, OnboardingPatchError> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    let source = value
        .as_object()
        .ok_or(OnboardingPatchError::InvalidOutfitDesign)?;
    let mut sanitized = Map::new();
    for (key, max_chars) in [
        ("id", 80),
        ("titleZh", 120),
        ("eraCueZh", 240),
        ("designZh", 1_200),
        ("layersEn", 1_200),
        ("heroAccessoryZh", 500),
        ("paletteHintZh", 500),
    ] {
        if let Some(raw) = source.get(key).and_then(Value::as_str) {
            if let Some(value) = sanitize_scalar(raw, max_chars) {
                sanitized.insert(key.to_string(), json!(value));
            }
        }
    }
    if !sanitized.contains_key("id")
        || !sanitized.contains_key("titleZh")
        || !sanitized.contains_key("designZh")
    {
        return Err(OnboardingPatchError::InvalidOutfitDesign);
    }
    Ok(Value::Object(sanitized))
}

pub fn sanitize_onboarding_tags(tags: &[String]) -> Vec<String> {
    let mut sanitized = Vec::new();
    for tag in tags {
        let candidate = tag.trim();
        if candidate.is_empty()
            || candidate.chars().count() > MAX_ONBOARDING_TAG_CHARS
            || candidate.chars().any(char::is_control)
            || sanitized
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(candidate))
        {
            continue;
        }
        sanitized.push(candidate.to_string());
        if sanitized.len() == MAX_ONBOARDING_TAGS {
            break;
        }
    }
    sanitized
}

pub fn build_onboarding_dna(tags: &[String]) -> Value {
    json!({
        "schemaVersion": DNA_SCHEMA_VERSION,
        "source": "onboarding",
        "tags": sanitize_onboarding_tags(tags),
    })
}

pub fn build_onboarding_dna_with_report(
    tags: &[String],
    provenance: Option<&ReportDnaProvenance>,
) -> Value {
    let tags = sanitize_onboarding_tags(tags);
    let mut dna = build_onboarding_dna(&tags);
    if let Some(provenance) = provenance {
        dna["source"] = json!("onboarding+report-signals");
        dna["reportSignals"] = report_dna_json(&tags, provenance);
    }
    dna
}

pub fn build_onboarding_persona(name: &str, language: &str, tags: &[String]) -> Value {
    let tags = sanitize_onboarding_tags(tags);
    json!({
        "displayName": name,
        "summary": "",
        "traits": tags,
        "speechStyle": "",
        "language": language,
        "draftSource": "seed",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_and_bounds_multilingual_onboarding_tags() {
        let tags = vec![
            " gentle ".to_string(),
            "GENTLE".to_string(),
            "好奇".to_string(),
            "bad\nvalue".to_string(),
            "x".repeat(MAX_ONBOARDING_TAG_CHARS + 1),
            "playful".to_string(),
        ];
        assert_eq!(
            sanitize_onboarding_tags(&tags),
            vec!["gentle", "好奇", "playful"]
        );
    }

    #[test]
    fn seeds_schema_v7_dna_and_persona_traits() {
        let tags = vec!["curious".to_string(), "calm".to_string()];
        let dna = build_onboarding_dna(&tags);
        let persona = build_onboarding_persona("Nova", "en-US", &tags);
        assert_eq!(dna["schemaVersion"], DNA_SCHEMA_VERSION);
        assert_eq!(dna["source"], "onboarding");
        assert_eq!(persona["traits"], dna["tags"]);
        assert_eq!(persona["language"], "en-US");
    }

    #[test]
    fn report_provenance_keeps_only_bounded_metadata() {
        let provenance = ReportDnaProvenance {
            fingerprint: "sha256-test".into(),
            report_count: 2,
            platforms: vec!["github".into(), "steam".into()],
        };
        let dna = build_onboarding_dna_with_report(&["curious".into()], Some(&provenance));
        assert_eq!(dna["source"], "onboarding+report-signals");
        assert_eq!(dna["reportSignals"]["rawReportsStored"], false);
        assert!(dna["reportSignals"].get("summary").is_none());
    }

    #[test]
    fn onboarding_is_character_only_and_has_five_steps() {
        let state = initial_onboarding_state(
            "Nova",
            &["curious".into()],
            Some("nonbinary"),
            Some("silver hair"),
            Some(json!({
                "id": "coat",
                "titleZh": "长外套",
                "designZh": "层叠长外套",
                "layersEn": "layered long coat",
                "heroAccessoryZh": "胸针",
                "paletteHintZh": "银蓝",
            })),
            None,
        )
        .unwrap();
        assert_eq!(state["step"], 3);
        assert!(state.get("environment").is_none());
        assert!(state.get("roomStyle").is_none());
        assert!(apply_onboarding_patch(
            &state,
            OnboardingPatch {
                step: Some(6),
                ..OnboardingPatch::default()
            },
        )
        .is_err());
    }

    #[test]
    fn visual_slots_exclude_world_rooms_furniture_and_objects() {
        assert_eq!(CharacterVisualSlot::DERIVED.len(), 9);
        for slot in CharacterVisualSlot::DERIVED {
            assert!(slot.requires_master());
            assert!(!slot.as_str().contains("world"));
            assert!(!slot.as_str().contains("room"));
            assert!(!slot.as_str().contains("furniture"));
            assert!(!slot.as_str().contains("object"));
        }
        assert!("world".parse::<CharacterVisualSlot>().is_err());
        assert!("furniture".parse::<CharacterVisualSlot>().is_err());
    }

    #[test]
    fn portrait_slots_use_the_canonical_three_by_four_canvas() {
        assert_eq!(CharacterVisualSlot::Master.dimensions(), (1152, 1536));
        assert_eq!(
            CharacterVisualSlot::DirectionSouthwest.dimensions(),
            (1152, 1536)
        );
        assert_eq!(CharacterVisualSlot::RigTurnaround.dimensions(), (1536, 1024));
    }
}
