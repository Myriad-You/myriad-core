use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::rig_contract::{
    CHARACTER_ASSET_CONTRACT_VERSION, MAX_RIGID_ARM_ROTATION_DEGREES, MAX_RIG_BONES, MAX_RIG_CLIPS,
    MAX_RIG_COLLISION_VOLUMES, MAX_RIG_KEYFRAMES_PER_TRACK, MAX_RIG_PARTS, MAX_RIG_TEXTURES,
    MAX_RIG_TOTAL_VERTICES, MAX_RIG_VERTICES_PER_PART, MIN_SUPPORTED_RIG_IR_VERSION,
    PORTRAIT_CANVAS_HEIGHT, PORTRAIT_CANVAS_WIDTH, RIG_IR_VERSION, RIG_SCHEMA_VERSION,
    STANDARD_CLIP_LIBRARY_VERSION,
};
use crate::rig_contract::PRESENTATION_SLOT_VARIANTS;
use crate::rig_outfit::{
    create_outfit_profile, default_semantic_anchors, outfit_profile_is_valid,
    semantic_anchors_are_valid,
};
pub use crate::rig_outfit::{
    infer_outfit_profile, RigOutfitProfile, RigOutfitTopology, RigSemanticAnchor,
};
pub use crate::rig_semantics::RigSemantics;
use crate::rig_semantics::{
    default_rig_semantics, migrate_rig_semantics, rig_semantics_are_valid,
};
use crate::rig_spatial::{infer_spatial_profile, spatial_profile_is_valid, RigSpatialProfile};

const MAX_RIGID_ARM_FRAGMENT_ROTATION: f32 =
    MAX_RIGID_ARM_ROTATION_DEGREES * std::f32::consts::PI / 180.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RigQuality {
    PortraitFallback,
    #[serde(rename = "layered-2d", alias = "articulated")]
    Layered2d,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigTexture {
    pub id: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigBone {
    pub id: String,
    pub parent: Option<String>,
    pub pivot: RigPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigVertex {
    pub position: RigPoint,
    pub uv: RigPoint,
    pub joints: [u8; 4],
    pub weights: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigPart {
    pub id: String,
    pub texture_id: String,
    pub z_index: i16,
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub vertices: Vec<RigVertex>,
    pub indices: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigTransform {
    pub translation: RigPoint,
    pub rotation: f32,
    pub scale: RigPoint,
}

impl RigTransform {
    pub const IDENTITY: Self = Self {
        translation: RigPoint { x: 0.0, y: 0.0 },
        rotation: 0.0,
        scale: RigPoint { x: 1.0, y: 1.0 },
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigKeyframe {
    pub time: f32,
    pub transform: RigTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigTrack {
    pub bone_id: String,
    pub keyframes: Vec<RigKeyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RigExpressionPresentation {
    Neutral,
    Happy,
    Surprise,
    Sad,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigPresentationKeyframe {
    pub progress: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<RigExpressionPresentation>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigClipPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<RigExpressionPresentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<RigPresentationKeyframe>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigClipEvent {
    pub progress: f32,
    pub kind: String,
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigClipGenerationProfile {
    pub max_amplitude_scale: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigClip {
    pub id: String,
    pub duration: f32,
    pub looping: bool,
    pub tracks: Vec<RigTrack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<RigClipPresentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<RigClipEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<RigClipGenerationProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigBreathMotionProfile {
    pub min_frequency_hz: f32,
    pub max_frequency_hz: f32,
    pub amplitude: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigBlinkMotionProfile {
    pub min_interval_seconds: f32,
    pub max_interval_seconds: f32,
    pub duration_seconds: f32,
    pub double_chance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigSecondaryMotionProfile {
    pub enabled: bool,
    pub frequency_hz: f32,
    pub damping_ratio: f32,
    pub response: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigMotionProfile {
    pub seed: u32,
    pub breath: RigBreathMotionProfile,
    pub blink: RigBlinkMotionProfile,
    pub secondary: RigSecondaryMotionProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigManifest {
    pub schema_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig_ir_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_asset_contract_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_master_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_generation_fingerprint: Option<String>,
    pub quality: RigQuality,
    pub canvas: RigSize,
    pub textures: Vec<RigTexture>,
    pub bones: Vec<RigBone>,
    pub parts: Vec<RigPart>,
    pub clips: Vec<RigClip>,
    pub default_clip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_clip_library_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_profile: Option<RigMotionProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outfit_profile: Option<RigOutfitProfile>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub semantic_anchors: HashMap<String, RigSemanticAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantics: Option<RigSemantics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_profile: Option<RigSpatialProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigBoneHandle {
    pub bone_id: String,
    pub start: RigPoint,
    pub end: RigPoint,
    pub falloff: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigLayerMeshSource {
    pub vertices: Vec<RigPoint>,
    pub indices: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigLayerSource {
    pub id: String,
    pub texture_id: String,
    pub texture_bounds: RigRect,
    pub z_index: i16,
    pub opacity: f32,
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub contours: Vec<Vec<RigPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<RigLayerMeshSource>,
    pub bone_handles: Vec<RigBoneHandle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RigCompileSource {
    pub rig_ir_version: Option<u16>,
    pub character_asset_contract_version: Option<u16>,
    pub source_master_asset_id: Option<String>,
    pub source_generation_fingerprint: Option<String>,
    pub canvas: RigSize,
    pub textures: Vec<RigTexture>,
    pub bones: Vec<RigBone>,
    pub layers: Vec<RigLayerSource>,
    pub clips: Vec<RigClip>,
    pub default_clip: String,
    pub motion_profile: Option<RigMotionProfile>,
    pub outfit_profile: Option<RigOutfitProfile>,
    pub semantic_anchors: HashMap<String, RigSemanticAnchor>,
    pub semantics: Option<RigSemantics>,
    pub spatial_profile: Option<RigSpatialProfile>,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum RigValidationError {
    #[error("unsupported rig schema version")]
    SchemaVersion,
    #[error("rig canvas is invalid")]
    Canvas,
    #[error("character asset contract is invalid")]
    AssetContract,
    #[error("rig contains too many bones")]
    TooManyBones,
    #[error("rig identifier is empty or duplicated")]
    Identifier,
    #[error("rig bone hierarchy is invalid")]
    BoneHierarchy,
    #[error("rig mesh is invalid")]
    Mesh,
    #[error("rig skin weights are invalid")]
    SkinWeights,
    #[error("rig animation is invalid")]
    Animation,
    #[error("rig motion profile is invalid")]
    MotionProfile,
    #[error("rig outfit profile is invalid")]
    OutfitProfile,
    #[error("rig semantic anchors are invalid")]
    SemanticAnchors,
    #[error("rig semantic IR is invalid")]
    Semantics,
    #[error("rig spatial profile is invalid")]
    SpatialProfile,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum RigCompileError {
    #[error("rig source IR version is not current")]
    IrVersion,
    #[error("rig source character asset contract is not current")]
    AssetContract,
    #[error("rig source contour is invalid")]
    Contour,
    #[error("rig source texture bounds are invalid")]
    TextureBounds,
    #[error("rig source bone handle is invalid")]
    BoneHandle,
    #[error("rig source mesh exceeds index limits")]
    MeshTooLarge,
    #[error("rig source exceeds complexity limits")]
    Complexity,
    #[error("rig source triangulation failed")]
    Triangulation,
    #[error(transparent)]
    Validation(#[from] RigValidationError),
}

impl RigManifest {
    pub fn validate(&self) -> Result<(), RigValidationError> {
        if self.schema_version != RIG_SCHEMA_VERSION {
            return Err(RigValidationError::SchemaVersion);
        }
        if self.rig_ir_version.is_some_and(|version| {
            !(MIN_SUPPORTED_RIG_IR_VERSION..=RIG_IR_VERSION).contains(&version)
        }) {
            return Err(RigValidationError::Semantics);
        }
        if self
            .character_asset_contract_version
            .is_some_and(|version| version != CHARACTER_ASSET_CONTRACT_VERSION)
            || self
                .source_master_asset_id
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 512)
            || self
                .source_generation_fingerprint
                .as_deref()
                .is_some_and(|value| {
                    value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit())
                })
        {
            return Err(RigValidationError::AssetContract);
        }
        if !self.canvas.width.is_finite()
            || !self.canvas.height.is_finite()
            || self.canvas.width <= 0.0
            || self.canvas.height <= 0.0
        {
            return Err(RigValidationError::Canvas);
        }
        if self.bones.is_empty() || self.bones.len() > MAX_RIG_BONES {
            return Err(RigValidationError::TooManyBones);
        }
        if self.textures.is_empty()
            || self.textures.len() > MAX_RIG_TEXTURES
            || self.parts.is_empty()
            || self.parts.len() > MAX_RIG_PARTS
            || self.clips.is_empty()
            || self.clips.len() > MAX_RIG_CLIPS
            || self
                .parts
                .iter()
                .map(|part| part.vertices.len())
                .sum::<usize>()
                > MAX_RIG_TOTAL_VERTICES
        {
            return Err(RigValidationError::Mesh);
        }

        let texture_ids = unique_ids(self.textures.iter().map(|texture| texture.id.as_str()))?;
        let bone_ids = unique_ids(self.bones.iter().map(|bone| bone.id.as_str()))?;
        unique_ids(self.parts.iter().map(|part| part.id.as_str()))?;
        let clip_ids = unique_ids(self.clips.iter().map(|clip| clip.id.as_str()))?;
        if !clip_ids.contains(self.default_clip.as_str()) {
            return Err(RigValidationError::Animation);
        }
        if self.textures.iter().any(|texture| {
            texture.url.trim().is_empty()
                || texture.width == 0
                || texture.height == 0
                || texture.width > 16_384
                || texture.height > 16_384
        }) {
            return Err(RigValidationError::Mesh);
        }

        for bone in &self.bones {
            if !point_is_finite(bone.pivot)
                || bone
                    .parent
                    .as_deref()
                    .is_some_and(|parent| parent == bone.id || !bone_ids.contains(parent))
            {
                return Err(RigValidationError::BoneHierarchy);
            }
        }
        validate_bone_cycles(&self.bones)?;

        for part in &self.parts {
            if !texture_ids.contains(part.texture_id.as_str())
                || part.vertices.len() < 3
                || part.indices.len() < 3
                || part.indices.len() % 3 != 0
                || part.vertices.len() > MAX_RIG_VERTICES_PER_PART
                || !part.opacity.is_finite()
                || !(0.0..=1.0).contains(&part.opacity)
                || part.slot.is_some() != part.variant.is_some()
                || part
                    .slot
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 64)
                || part
                    .variant
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 64)
                || part
                    .indices
                    .iter()
                    .any(|index| usize::from(*index) >= part.vertices.len())
            {
                return Err(RigValidationError::Mesh);
            }
            for vertex in &part.vertices {
                if !point_is_finite(vertex.position)
                    || !point_is_finite(vertex.uv)
                    || vertex
                        .joints
                        .iter()
                        .any(|joint| usize::from(*joint) >= self.bones.len())
                    || vertex
                        .weights
                        .iter()
                        .any(|weight| !weight.is_finite() || *weight < 0.0 || *weight > 1.0)
                    || (vertex.weights.iter().sum::<f32>() - 1.0).abs() > 0.002
                {
                    return Err(RigValidationError::SkinWeights);
                }
            }
        }
        if self.rig_ir_version.unwrap_or(0) >= 3 {
            validate_presentation_parts(&self.parts)?;
        }

        for clip in &self.clips {
            if !clip.duration.is_finite()
                || clip.duration <= 0.0
                || clip.duration > 120.0
                || clip.tracks.is_empty()
                || clip.tracks.len() > self.bones.len()
                || clip
                    .presentation
                    .as_ref()
                    .is_some_and(|presentation| !clip_presentation_is_valid(presentation))
                || clip.events.len() > 16
                || !clip.events.iter().all(|event| {
                    event.progress.is_finite()
                        && (0.0..=1.0).contains(&event.progress)
                        && !event.kind.is_empty()
                        && event.kind.len() <= 64
                        && event.kind.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                        })
                        && event.intensity.is_finite()
                        && (0.0..=1.2).contains(&event.intensity)
                })
                || !clip
                    .events
                    .windows(2)
                    .all(|pair| pair[0].progress <= pair[1].progress)
                || clip.generation.as_ref().is_some_and(|profile| {
                    !profile.max_amplitude_scale.is_finite()
                        || !(0.62..=1.24).contains(&profile.max_amplitude_scale)
                })
            {
                return Err(RigValidationError::Animation);
            }
            let mut tracked_bones = HashSet::new();
            for track in &clip.tracks {
                if !bone_ids.contains(track.bone_id.as_str())
                    || !tracked_bones.insert(track.bone_id.as_str())
                    || track.keyframes.is_empty()
                    || track.keyframes.len() > MAX_RIG_KEYFRAMES_PER_TRACK
                    || track
                        .keyframes
                        .windows(2)
                        .any(|pair| pair[0].time >= pair[1].time)
                    || track.keyframes.iter().any(|keyframe| {
                        !keyframe.time.is_finite()
                            || keyframe.time < 0.0
                            || keyframe.time > clip.duration
                            || !transform_is_valid(keyframe.transform)
                            || (is_rigid_arm_fragment(&track.bone_id)
                                && keyframe.transform.rotation.abs()
                                    > MAX_RIGID_ARM_FRAGMENT_ROTATION + 0.0001)
                    })
                {
                    return Err(RigValidationError::Animation);
                }
            }
        }
        if self
            .standard_clip_library_version
            .is_some_and(|version| version == 0)
        {
            return Err(RigValidationError::Animation);
        }
        if self
            .motion_profile
            .as_ref()
            .is_some_and(|profile| !motion_profile_is_valid(profile))
        {
            return Err(RigValidationError::MotionProfile);
        }
        if self
            .outfit_profile
            .as_ref()
            .is_some_and(|profile| !outfit_profile_is_valid(profile, &self.parts))
        {
            return Err(RigValidationError::OutfitProfile);
        }
        if !semantic_anchors_are_valid(&self.semantic_anchors, &self.bones, self.canvas) {
            return Err(RigValidationError::SemanticAnchors);
        }
        if self
            .semantics
            .as_ref()
            .is_some_and(|semantics| !rig_semantics_are_valid(semantics, &self.bones))
        {
            return Err(RigValidationError::Semantics);
        }
        if self.rig_ir_version.is_some()
            && (self.semantics.is_none() || self.spatial_profile.is_none())
        {
            return Err(RigValidationError::Semantics);
        }
        if self
            .spatial_profile
            .as_ref()
            .is_some_and(|profile| !spatial_profile_is_valid(profile, &self.bones, self.canvas))
        {
            return Err(RigValidationError::SpatialProfile);
        }
        Ok(())
    }
}

fn validate_presentation_parts(parts: &[RigPart]) -> Result<(), RigValidationError> {
    let mut variants_by_slot = HashMap::<&str, HashSet<&str>>::new();
    for part in parts {
        let (Some(slot), Some(variant)) = (part.slot.as_deref(), part.variant.as_deref()) else {
            continue;
        };
        let Some((_, _, allowed)) = PRESENTATION_SLOT_VARIANTS
            .iter()
            .find(|(candidate, _, _)| *candidate == slot)
        else {
            return Err(RigValidationError::Mesh);
        };
        if !allowed.contains(&variant) {
            return Err(RigValidationError::Mesh);
        }
        variants_by_slot.entry(slot).or_default().insert(variant);
    }
    for (slot, variants) in variants_by_slot {
        let fallback = PRESENTATION_SLOT_VARIANTS
            .iter()
            .find(|(candidate, _, _)| *candidate == slot)
            .map(|(_, fallback, _)| *fallback)
            .ok_or(RigValidationError::Mesh)?;
        if !variants.contains(fallback) {
            return Err(RigValidationError::Mesh);
        }
    }
    Ok(())
}

pub fn default_rig_motion_profile(seed: u32) -> RigMotionProfile {
    RigMotionProfile {
        seed: seed.max(1),
        breath: RigBreathMotionProfile {
            min_frequency_hz: 0.16,
            max_frequency_hz: 0.27,
            amplitude: 0.0036,
        },
        blink: RigBlinkMotionProfile {
            min_interval_seconds: 2.7,
            max_interval_seconds: 6.8,
            duration_seconds: 0.24,
            double_chance: 0.16,
        },
        secondary: RigSecondaryMotionProfile {
            enabled: true,
            frequency_hz: 2.15,
            damping_ratio: 0.52,
            response: 0.58,
        },
    }
}

pub fn migrate_rig_manifest(
    mut manifest: RigManifest,
    seed: u32,
) -> Result<(RigManifest, bool), RigValidationError> {
    let mut changed = false;
    if manifest.motion_profile.is_none() {
        manifest.motion_profile = Some(default_rig_motion_profile(seed));
        changed = true;
    }
    if manifest.outfit_profile.is_none() {
        manifest.outfit_profile = Some(infer_outfit_profile(&manifest.parts));
        changed = true;
    }
    let secondary = manifest
        .outfit_profile
        .as_ref()
        .map(|profile| profile.secondary_part_ids.as_slice())
        .unwrap_or_default();
    let migrated_semantics = migrate_rig_semantics(
        manifest.semantics.as_ref(),
        &manifest.bones,
        secondary,
    );
    if manifest.semantics.as_ref() != Some(&migrated_semantics) {
        manifest.semantics = Some(migrated_semantics);
        changed = true;
    }
    if manifest.spatial_profile.is_none() {
        manifest.spatial_profile = Some(infer_spatial_profile(
            manifest.canvas,
            &manifest.bones,
            &manifest.parts,
            manifest.semantics.as_ref(),
        ));
        changed = true;
    }
    if manifest.rig_ir_version != Some(RIG_IR_VERSION)
        && validate_presentation_parts(&manifest.parts).is_ok()
    {
        manifest.rig_ir_version = Some(RIG_IR_VERSION);
        changed = true;
    }
    if manifest.semantic_anchors.is_empty() {
        manifest.semantic_anchors =
            default_semantic_anchors(&manifest.bones, manifest.semantics.as_ref());
        changed = true;
    }
    let standard =
        build_standard_face_rig_clips_for_semantics(&manifest.bones, manifest.semantics.as_ref())?;
    let standard_ids = standard
        .iter()
        .map(|clip| clip.id.clone())
        .collect::<HashSet<_>>();
    let should_upgrade =
        manifest.standard_clip_library_version.unwrap_or(0) < STANDARD_CLIP_LIBRARY_VERSION;
    let mut canonical_by_id = standard
        .iter()
        .cloned()
        .map(|clip| (clip.id.clone(), clip))
        .collect::<HashMap<_, _>>();
    if should_upgrade {
        for existing in &mut manifest.clips {
            let Some(canonical) = canonical_by_id.remove(&existing.id) else {
                continue;
            };
            if *existing != canonical {
                *existing = canonical;
                changed = true;
            }
        }
    } else {
        for existing in &manifest.clips {
            canonical_by_id.remove(&existing.id);
        }
    }
    for clip in standard {
        if !canonical_by_id.contains_key(&clip.id) {
            continue;
        }
        if manifest.clips.len() >= MAX_RIG_CLIPS {
            break;
        }
        manifest.clips.push(clip);
        changed = true;
    }
    let upgraded_completely = standard_ids
        .iter()
        .all(|standard_id| manifest.clips.iter().any(|clip| &clip.id == standard_id));
    if upgraded_completely
        && manifest.standard_clip_library_version != Some(STANDARD_CLIP_LIBRARY_VERSION)
    {
        manifest.standard_clip_library_version = Some(STANDARD_CLIP_LIBRARY_VERSION);
        changed = true;
    }
    manifest.validate()?;
    Ok((manifest, changed))
}

pub fn build_portrait_fallback_rig(master_url: impl Into<String>) -> RigManifest {
    build_portrait_fallback_rig_with_generation(master_url, None)
}

pub fn build_portrait_fallback_rig_with_generation(
    master_url: impl Into<String>,
    source_generation_fingerprint: Option<String>,
) -> RigManifest {
    let master_url = master_url.into();
    let columns = 8_u16;
    let rows = 12_u16;
    let canvas_height = PORTRAIT_CANVAS_HEIGHT;
    let mut vertices = Vec::with_capacity(usize::from(columns + 1) * usize::from(rows + 1));
    for row in 0..=rows {
        let normalized_y = f32::from(row) / f32::from(rows);
        let head = 1.0 - smoothstep(0.22, 0.50, normalized_y);
        let root = smoothstep(0.63, 0.90, normalized_y);
        let body = (1.0 - head - root).max(0.0);
        let total = head + body + root;
        for column in 0..=columns {
            let x = f32::from(column) / f32::from(columns);
            vertices.push(RigVertex {
                position: RigPoint {
                    x,
                    y: normalized_y * canvas_height,
                },
                uv: RigPoint { x, y: normalized_y },
                joints: [0, 1, 2, 0],
                weights: [root / total, body / total, head / total, 0.0],
            });
        }
    }
    let mut indices = Vec::with_capacity(usize::from(columns) * usize::from(rows) * 6);
    for row in 0..rows {
        for column in 0..columns {
            let top_left = row * (columns + 1) + column;
            let top_right = top_left + 1;
            let bottom_left = top_left + columns + 1;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                top_right,
                bottom_left,
                top_right,
                bottom_right,
                bottom_left,
            ]);
        }
    }
    let bones = vec![
        RigBone {
            id: "root".to_string(),
            parent: None,
            pivot: RigPoint {
                x: 0.5,
                y: 0.82 * canvas_height,
            },
        },
        RigBone {
            id: "body".to_string(),
            parent: Some("root".to_string()),
            pivot: RigPoint {
                x: 0.5,
                y: 0.58 * canvas_height,
            },
        },
        RigBone {
            id: "head".to_string(),
            parent: Some("body".to_string()),
            pivot: RigPoint {
                x: 0.5,
                y: 0.29 * canvas_height,
            },
        },
    ];
    let clips = build_standard_face_rig_clips(&bones).expect("fallback skeleton is canonical");

    let semantics = default_rig_semantics(&bones, &[]);
    let semantic_anchors = default_semantic_anchors(&bones, Some(&semantics));
    let canvas = RigSize {
        width: PORTRAIT_CANVAS_WIDTH,
        height: canvas_height,
    };
    let spatial_profile = infer_spatial_profile(canvas, &bones, &[], Some(&semantics));
    RigManifest {
        schema_version: RIG_SCHEMA_VERSION,
        rig_ir_version: Some(RIG_IR_VERSION),
        character_asset_contract_version: Some(CHARACTER_ASSET_CONTRACT_VERSION),
        source_master_asset_id: Some(master_url.clone()),
        source_generation_fingerprint,
        quality: RigQuality::PortraitFallback,
        canvas,
        textures: vec![RigTexture {
            id: "portrait".to_string(),
            url: master_url,
            width: 1024,
            height: 1536,
        }],
        bones,
        parts: vec![RigPart {
            id: "portrait".to_string(),
            texture_id: "portrait".to_string(),
            z_index: 0,
            opacity: 1.0,
            slot: None,
            variant: None,
            vertices,
            indices,
        }],
        clips,
        default_clip: "idle".to_string(),
        standard_clip_library_version: Some(STANDARD_CLIP_LIBRARY_VERSION),
        motion_profile: None,
        outfit_profile: Some(create_outfit_profile(Vec::new(), Vec::new())),
        semantic_anchors,
        semantics: Some(semantics),
        spatial_profile: Some(spatial_profile),
    }
}

pub fn build_standard_face_rig_clips(
    bones: &[RigBone],
) -> Result<Vec<RigClip>, RigValidationError> {
    let bone_ids = bones
        .iter()
        .map(|bone| bone.id.as_str())
        .collect::<HashSet<_>>();
    if !["root", "body", "head"]
        .iter()
        .all(|bone| bone_ids.contains(bone))
    {
        return Err(RigValidationError::BoneHierarchy);
    }
    require_bone_parent(bones, "body", "root")?;
    require_bone_parent(bones, "head", "body")?;
    let has_face = require_optional_bone_parent(bones, "face", "head")?;
    let has_left_eye = require_optional_bone_parent(bones, "left-eye", "face")?;
    let has_right_eye = require_optional_bone_parent(bones, "right-eye", "face")?;
    let has_mouth = require_optional_bone_parent(bones, "mouth", "face")?;
    let mut clips = fallback_clips();
    clips.extend([
        clip(
            "idle-accent",
            1.8,
            false,
            vec![
                pose_track("body", 1.8, 0.8, transform(0.012, -0.004, 0.025, 1.0, 1.0)),
                pose_track(
                    "head",
                    1.8,
                    0.8,
                    transform(-0.006, -0.004, -0.040, 1.0, 1.0),
                ),
            ],
        ),
        clip(
            "nod",
            0.9,
            false,
            vec![track(
                "head",
                &[
                    (0.0, RigTransform::IDENTITY),
                    (0.32, transform(0.0, 0.012, 0.0, 1.0, 0.985)),
                    (0.62, transform(0.0, -0.005, 0.0, 1.0, 1.008)),
                    (0.9, RigTransform::IDENTITY),
                ],
            )],
        ),
        clip(
            "listen",
            1.1,
            false,
            vec![pose_track(
                "head",
                1.1,
                0.38,
                transform(0.004, -0.004, 0.045, 1.0, 1.0),
            )],
        ),
        clip(
            "respond",
            1.0,
            false,
            vec![track(
                "head",
                &[
                    (0.0, RigTransform::IDENTITY),
                    (0.28, transform(0.0, -0.008, -0.018, 1.0, 1.0)),
                    (0.58, transform(0.0, 0.006, 0.012, 1.0, 1.0)),
                    (1.0, RigTransform::IDENTITY),
                ],
            )],
        ),
        clip(
            "observe",
            1.8,
            false,
            vec![
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.48, transform(-0.004, 0.0, -0.018, 1.0, 1.0)),
                        (1.12, transform(0.004, 0.0, 0.018, 1.0, 1.0)),
                        (1.8, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.42, transform(-0.008, -0.004, -0.065, 1.0, 1.0)),
                        (1.08, transform(0.008, -0.004, 0.065, 1.0, 1.0)),
                        (1.8, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
        clip(
            "poke-reaction",
            0.9,
            false,
            vec![
                track(
                    "root",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.16, transform(-0.018, 0.0, -0.028, 1.0, 1.0)),
                        (0.36, transform(0.008, 0.0, 0.016, 1.0, 1.0)),
                        (0.9, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.16, transform(0.0, 0.006, 0.045, 0.99, 0.98)),
                        (0.9, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
    ]);
    clips.extend(core_performance_clips());
    if has_mouth {
        clips
            .iter_mut()
            .find(|clip| clip.id == "talking")
            .expect("fallback talking clip exists")
            .tracks
            .push(track(
                "mouth",
                &[
                    (0.0, transform(0.0, 0.0, 0.0, 1.0, 0.78)),
                    (0.18, transform(0.0, 0.0, 0.0, 1.03, 1.18)),
                    (0.36, transform(0.0, 0.0, 0.0, 0.98, 0.86)),
                    (0.54, transform(0.0, 0.0, 0.0, 1.02, 1.10)),
                    (0.72, transform(0.0, 0.0, 0.0, 1.0, 0.78)),
                ],
            ));
    }
    if has_face && has_left_eye && has_right_eye {
        clips.push(clip(
            "blink",
            0.30,
            false,
            vec![
                track(
                    "left-eye",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.10, transform(0.0, 0.002, 0.0, 1.0, 0.08)),
                        (0.16, transform(0.0, 0.002, 0.0, 1.0, 0.08)),
                        (0.30, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "right-eye",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.10, transform(0.0, 0.002, 0.0, 1.0, 0.08)),
                        (0.16, transform(0.0, 0.002, 0.0, 1.0, 0.08)),
                        (0.30, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ));
        clips.extend(expression_clips(has_mouth));
    }
    attach_standard_clip_metadata(&mut clips);
    Ok(clips)
}

/// Builds the canonical library against explicitly mapped destination bones.
/// This keeps authored clip semantics stable while allowing arbitrary source ids.
pub fn build_standard_face_rig_clips_for_semantics(
    bones: &[RigBone],
    semantics: Option<&RigSemantics>,
) -> Result<Vec<RigClip>, RigValidationError> {
    let Some(semantics) = semantics else {
        return build_standard_face_rig_clips(bones);
    };
    if !rig_semantics_are_valid(semantics, bones) {
        return Err(RigValidationError::Semantics);
    }
    let by_id = bones
        .iter()
        .map(|bone| (bone.id.as_str(), bone))
        .collect::<HashMap<_, _>>();
    fn standard_id(role: &str) -> &str {
        if role == "torso" {
            "body"
        } else {
            role
        }
    }
    let role_by_bone = semantics
        .bones
        .iter()
        .map(|(role, bone_id)| (bone_id.as_str(), standard_id(role)))
        .collect::<HashMap<_, _>>();
    let standard_to_actual = semantics
        .bones
        .iter()
        .map(|(role, bone_id)| (standard_id(role).to_string(), bone_id.clone()))
        .collect::<HashMap<_, _>>();
    let semantic_bones = semantics
        .bones
        .iter()
        .filter_map(|(role, bone_id)| {
            let bone = by_id.get(bone_id.as_str())?;
            Some(RigBone {
                id: standard_id(role).to_string(),
                parent: bone
                    .parent
                    .as_deref()
                    .and_then(|parent| role_by_bone.get(parent).copied())
                    .map(str::to_string),
                pivot: bone.pivot,
            })
        })
        .collect::<Vec<_>>();
    let mut clips = build_standard_face_rig_clips(&semantic_bones)?;
    for track in clips.iter_mut().flat_map(|clip| &mut clip.tracks) {
        track.bone_id = standard_to_actual
            .get(&track.bone_id)
            .cloned()
            .ok_or(RigValidationError::Semantics)?;
    }
    attach_rigid_arm_fragment_tracks(
        &mut clips,
        bones,
        semantics.bones.get("handwear").map(String::as_str),
    )?;
    Ok(clips)
}

fn attach_rigid_arm_fragment_tracks(
    clips: &mut [RigClip],
    bones: &[RigBone],
    handwear_parent: Option<&str>,
) -> Result<(), RigValidationError> {
    let Some(handwear_parent) = handwear_parent else {
        return Ok(());
    };
    let left = bones
        .iter()
        .find(|bone| bone.id == "a25d-handwear-left");
    let right = bones
        .iter()
        .find(|bone| bone.id == "a25d-handwear-right");
    for fragment in [left, right].into_iter().flatten() {
        if fragment.parent.as_deref() != Some(handwear_parent) {
            return Err(RigValidationError::BoneHierarchy);
        }
    }
    if left.is_none() && right.is_none() {
        return Ok(());
    }
    let authored = [
        ("bow", 0.08, -0.08),
        ("respond", 0.12, -0.12),
        ("surprise", 0.20, -0.20),
        ("shy", -0.18, 0.18),
        ("proud", 0.12, -0.12),
        ("happy", 0.20, -0.20),
        ("startle-settle", 0.22, -0.22),
        ("pat-reaction", -0.14, 0.14),
        ("poke-reaction", 0.16, -0.16),
        ("greet", 0.24, -0.06),
    ];
    for (clip_id, left_rotation, right_rotation) in authored {
        let Some(clip) = clips.iter_mut().find(|clip| clip.id == clip_id) else {
            continue;
        };
        let peak = clip.duration * if clip_id == "greet" { 0.38 } else { 0.42 };
        if left.is_some() {
            clip.tracks.push(pose_track(
                "a25d-handwear-left",
                clip.duration,
                peak,
                transform(0.0, 0.0, left_rotation, 1.0, 1.0),
            ));
        }
        if right.is_some() {
            clip.tracks.push(pose_track(
                "a25d-handwear-right",
                clip.duration,
                peak,
                transform(0.0, 0.0, right_rotation, 1.0, 1.0),
            ));
        }
    }
    Ok(())
}

fn core_performance_clips() -> Vec<RigClip> {
    vec![
        held_pose_clip(
            "bow",
            1.6,
            0.42,
            1.12,
            vec![
                ("root", transform(0.0, 0.016, 0.0, 1.0, 0.975)),
                ("body", transform(0.0, 0.018, 0.0, 1.015, 0.94)),
                ("head", transform(0.0, 0.024, 0.0, 1.0, 0.92)),
            ],
        ),
        clip(
            "shake-head",
            1.25,
            false,
            vec![track(
                "head",
                &[
                    (0.0, RigTransform::IDENTITY),
                    (0.25, transform(-0.008, 0.0, -0.095, 1.0, 1.0)),
                    (0.52, transform(0.008, 0.0, 0.095, 1.0, 1.0)),
                    (0.79, transform(-0.006, 0.0, -0.070, 1.0, 1.0)),
                    (1.02, transform(0.004, 0.0, 0.045, 1.0, 1.0)),
                    (1.25, RigTransform::IDENTITY),
                ],
            )],
        ),
        simple_pose_clip(
            "surprise",
            1.1,
            0.22,
            vec![
                ("root", transform(0.0, -0.018, 0.0, 1.02, 1.02)),
                ("body", transform(0.0, -0.008, 0.0, 1.018, 1.018)),
                ("head", transform(0.0, -0.012, 0.0, 1.045, 1.045)),
            ],
        ),
        held_pose_clip(
            "shy",
            1.8,
            0.48,
            1.34,
            vec![
                ("body", transform(-0.010, 0.006, -0.034, 0.995, 0.99)),
                ("head", transform(-0.012, 0.010, 0.078, 0.99, 0.99)),
            ],
        ),
        held_pose_clip(
            "proud",
            1.7,
            0.42,
            1.26,
            vec![
                ("body", transform(0.0, -0.012, 0.0, 1.018, 1.025)),
                ("head", transform(0.004, -0.008, -0.032, 1.01, 1.01)),
            ],
        ),
        clip(
            "sigh",
            1.9,
            false,
            vec![
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.55, transform(0.0, -0.010, 0.0, 1.01, 1.018)),
                        (1.18, transform(0.0, 0.016, 0.018, 0.985, 0.97)),
                        (1.9, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.55, transform(0.0, -0.006, 0.0, 1.005, 1.005)),
                        (1.18, transform(0.0, 0.014, 0.055, 0.99, 0.98)),
                        (1.9, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
        clip(
            "look-around",
            2.4,
            false,
            vec![
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.38, transform(-0.004, 0.0, -0.018, 1.0, 1.0)),
                        (0.92, transform(-0.008, 0.0, -0.035, 1.0, 1.0)),
                        (1.46, transform(0.006, 0.0, 0.028, 1.0, 1.0)),
                        (1.92, transform(0.003, 0.0, 0.012, 1.0, 1.0)),
                        (2.4, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.28, transform(-0.006, -0.002, -0.045, 1.0, 1.0)),
                        (0.78, transform(-0.014, -0.004, -0.11, 1.0, 1.0)),
                        (1.18, transform(0.004, -0.002, 0.025, 1.0, 1.0)),
                        (1.58, transform(0.014, -0.004, 0.11, 1.0, 1.0)),
                        (2.0, transform(0.004, 0.0, 0.028, 1.0, 1.0)),
                        (2.4, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
        clip(
            "deep-breath",
            2.8,
            false,
            vec![
                track(
                    "root",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.42, transform(0.0, 0.002, 0.0, 0.998, 0.995)),
                        (1.18, transform(0.0, -0.012, 0.0, 1.012, 1.024)),
                        (1.62, transform(0.0, -0.014, 0.0, 1.014, 1.028)),
                        (2.18, transform(0.0, 0.006, 0.0, 0.995, 0.99)),
                        (2.8, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.42, transform(0.0, 0.004, 0.012, 0.995, 0.99)),
                        (1.18, transform(0.0, -0.012, -0.012, 1.015, 1.028)),
                        (1.62, transform(0.0, -0.014, -0.016, 1.016, 1.03)),
                        (2.18, transform(0.0, 0.008, 0.018, 0.99, 0.982)),
                        (2.8, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.48, transform(0.0, 0.004, 0.022, 0.995, 0.995)),
                        (1.28, transform(0.0, -0.008, -0.018, 1.008, 1.008)),
                        (1.68, transform(0.0, -0.006, -0.012, 1.006, 1.006)),
                        (2.22, transform(0.0, 0.01, 0.03, 0.992, 0.988)),
                        (2.8, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
        clip(
            "startle-settle",
            1.65,
            false,
            vec![
                track(
                    "root",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.12, transform(0.0, 0.006, 0.0, 0.99, 0.985)),
                        (0.28, transform(0.0, -0.026, 0.0, 1.035, 1.04)),
                        (0.52, transform(0.0, 0.004, 0.0, 0.995, 0.99)),
                        (0.92, transform(0.0, -0.006, 0.0, 1.006, 1.008)),
                        (1.65, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.12, transform(0.0, 0.008, 0.018, 0.99, 0.985)),
                        (0.28, transform(0.0, -0.014, -0.026, 1.025, 1.03)),
                        (0.58, transform(0.0, 0.004, 0.016, 0.995, 0.99)),
                        (1.0, transform(0.0, -0.004, -0.008, 1.004, 1.004)),
                        (1.65, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.1, transform(0.0, 0.006, 0.035, 0.99, 0.985)),
                        (0.26, transform(0.0, -0.018, -0.055, 1.045, 1.045)),
                        (0.54, transform(0.0, 0.006, 0.028, 0.995, 0.99)),
                        (0.96, transform(0.0, -0.004, -0.014, 1.008, 1.008)),
                        (1.65, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
    ]
}

fn simple_pose_clip(
    id: &str,
    duration: f32,
    peak: f32,
    poses: Vec<(&str, RigTransform)>,
) -> RigClip {
    clip(
        id,
        duration,
        false,
        poses
            .into_iter()
            .map(|(bone_id, pose)| pose_track(bone_id, duration, peak, pose))
            .collect(),
    )
}

fn held_pose_clip(
    id: &str,
    duration: f32,
    enter: f32,
    exit: f32,
    poses: Vec<(&str, RigTransform)>,
) -> RigClip {
    clip(
        id,
        duration,
        false,
        poses
            .into_iter()
            .map(|(bone_id, pose)| held_pose_track(bone_id, duration, enter, exit, pose))
            .collect(),
    )
}

fn pose_track(bone_id: &str, duration: f32, peak: f32, pose: RigTransform) -> RigTrack {
    let anticipation = scale_transform_delta(pose, -0.08);
    let overshoot = scale_transform_delta(pose, 1.07);
    let recoil = scale_transform_delta(pose, 0.88);
    let release = scale_transform_delta(pose, 0.18);
    let return_span = duration - peak;
    track(
        bone_id,
        &[
            (0.0, RigTransform::IDENTITY),
            (peak * 0.32, anticipation),
            (peak * 0.82, overshoot),
            (peak, pose),
            (peak + return_span * 0.28, recoil),
            (peak + return_span * 0.72, release),
            (duration, RigTransform::IDENTITY),
        ],
    )
}

fn held_pose_track(
    bone_id: &str,
    duration: f32,
    enter: f32,
    exit: f32,
    pose: RigTransform,
) -> RigTrack {
    // Reversing before a very large joint rotation creates an angular-speed
    // spike even with spline interpolation.  Large overhead poses ramp in the
    // intended direction; smaller gestures retain the expressive counter-move.
    let lead_in = if pose.rotation.abs() > 1.8 {
        0.32
    } else {
        -0.07
    };
    let anticipation = scale_transform_delta(pose, lead_in);
    let overshoot = scale_transform_delta(pose, 1.06);
    let settle = scale_transform_delta(pose, 0.97);
    let follow_through = scale_transform_delta(pose, -0.04);
    let hold_span = exit - enter;
    let return_span = duration - exit;
    track(
        bone_id,
        &[
            (0.0, RigTransform::IDENTITY),
            (enter * 0.34, anticipation),
            (enter * 0.82, overshoot),
            (enter, pose),
            (enter + hold_span * 0.24, settle),
            (exit - hold_span * 0.16, pose),
            (exit, pose),
            (exit + return_span * 0.72, follow_through),
            (duration, RigTransform::IDENTITY),
        ],
    )
}

fn scale_transform_delta(pose: RigTransform, amount: f32) -> RigTransform {
    RigTransform {
        translation: RigPoint {
            x: pose.translation.x * amount,
            y: pose.translation.y * amount,
        },
        rotation: pose.rotation * amount,
        scale: RigPoint {
            x: 1.0 + (pose.scale.x - 1.0) * amount,
            y: 1.0 + (pose.scale.y - 1.0) * amount,
        },
    }
}

fn expression_clips(has_mouth: bool) -> Vec<RigClip> {
    let mut happy_tracks = vec![
        track(
            "root",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.42, transform(0.0, -0.018, 0.0, 1.0, 1.0)),
                (0.78, transform(0.0, 0.004, 0.0, 1.0, 1.0)),
                (1.16, transform(0.0, -0.010, 0.0, 1.0, 1.0)),
                (1.6, RigTransform::IDENTITY),
            ],
        ),
        track(
            "head",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.42, transform(0.0, -0.006, -0.035, 1.015, 1.015)),
                (1.16, transform(0.0, -0.004, 0.025, 1.01, 1.01)),
                (1.6, RigTransform::IDENTITY),
            ],
        ),
        eye_expression_track("left-eye", 1.6, 0.74),
        eye_expression_track("right-eye", 1.6, 0.74),
    ];
    let mut sad_tracks = vec![
        track(
            "body",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.66, transform(0.0, 0.012, 0.018, 0.995, 0.985)),
                (1.7, transform(0.0, 0.012, 0.018, 0.995, 0.985)),
                (2.2, RigTransform::IDENTITY),
            ],
        ),
        track(
            "head",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.62, transform(-0.006, 0.018, -0.045, 0.995, 0.985)),
                (1.7, transform(-0.006, 0.018, -0.045, 0.995, 0.985)),
                (2.2, RigTransform::IDENTITY),
            ],
        ),
        eye_expression_track("left-eye", 2.2, 0.86),
        eye_expression_track("right-eye", 2.2, 0.86),
    ];
    let mut sleep_tracks = vec![
        track(
            "body",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.8, transform(0.0, 0.008, 0.012, 1.0, 0.99)),
                (2.6, transform(0.0, 0.008, 0.012, 1.0, 0.99)),
                (3.4, RigTransform::IDENTITY),
            ],
        ),
        track(
            "head",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.72, transform(-0.008, 0.020, -0.075, 1.0, 0.985)),
                (2.6, transform(-0.008, 0.020, -0.075, 1.0, 0.985)),
                (3.4, RigTransform::IDENTITY),
            ],
        ),
        eye_expression_track("left-eye", 3.4, 0.08),
        eye_expression_track("right-eye", 3.4, 0.08),
    ];
    let mut pat_tracks = vec![
        track(
            "head",
            &[
                (0.0, RigTransform::IDENTITY),
                (0.34, transform(0.0, 0.010, 0.0, 1.015, 0.965)),
                (0.82, transform(0.0, 0.004, -0.018, 1.005, 0.985)),
                (1.4, RigTransform::IDENTITY),
            ],
        ),
        eye_expression_track("left-eye", 1.4, 0.12),
        eye_expression_track("right-eye", 1.4, 0.12),
    ];
    if has_mouth {
        happy_tracks.push(mouth_expression_track(1.6, 1.18));
        sad_tracks.push(mouth_expression_track(2.2, 0.82));
        sleep_tracks.push(mouth_expression_track(3.4, 0.92));
        pat_tracks.push(mouth_expression_track(1.4, 1.08));
    }
    vec![
        clip("happy", 1.6, false, happy_tracks),
        clip("sad", 2.2, false, sad_tracks),
        clip("sleep", 3.4, false, sleep_tracks),
        clip("pat-reaction", 1.4, false, pat_tracks),
    ]
}

fn eye_expression_track(bone_id: &str, duration: f32, scale_y: f32) -> RigTrack {
    track(
        bone_id,
        &[
            (0.0, RigTransform::IDENTITY),
            (duration * 0.22, transform(0.0, 0.002, 0.0, 1.0, scale_y)),
            (duration * 0.76, transform(0.0, 0.002, 0.0, 1.0, scale_y)),
            (duration, RigTransform::IDENTITY),
        ],
    )
}

fn mouth_expression_track(duration: f32, scale_y: f32) -> RigTrack {
    track(
        "mouth",
        &[
            (0.0, RigTransform::IDENTITY),
            (duration * 0.24, transform(0.0, 0.0, 0.0, 1.06, scale_y)),
            (duration * 0.76, transform(0.0, 0.0, 0.0, 1.06, scale_y)),
            (duration, RigTransform::IDENTITY),
        ],
    )
}

fn require_bone_parent(
    bones: &[RigBone],
    bone_id: &str,
    parent_id: &str,
) -> Result<(), RigValidationError> {
    let bone = bones
        .iter()
        .find(|bone| bone.id == bone_id)
        .ok_or(RigValidationError::BoneHierarchy)?;
    if bone.parent.as_deref() != Some(parent_id) {
        return Err(RigValidationError::BoneHierarchy);
    }
    Ok(())
}

fn require_optional_bone_parent(
    bones: &[RigBone],
    bone_id: &str,
    parent_id: &str,
) -> Result<bool, RigValidationError> {
    let Some(bone) = bones.iter().find(|bone| bone.id == bone_id) else {
        return Ok(false);
    };
    if bone.parent.as_deref() != Some(parent_id) {
        return Err(RigValidationError::BoneHierarchy);
    }
    Ok(true)
}

/// Authoritative server-side gate for the current upper-body character asset.
/// Browser PSD preflight improves feedback, but cannot be the trust boundary.
pub fn validate_character_asset_source(
    bones: &[RigBone],
    layers: &[RigLayerSource],
) -> Result<(), RigValidationError> {
    let has_layer = |prefix: &str| {
        layers.iter().any(|layer| {
            layer.id == prefix
                || layer
                    .id
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        })
    };
    let has_variant = |slot: &str, variant: &str| {
        layers.iter().any(|layer| {
            layer.slot.as_deref() == Some(slot) && layer.variant.as_deref() == Some(variant)
        })
    };
    let parent_is = |bone_id: &str, parent_id: &str| {
        bones.iter().any(|bone| {
            bone.id == bone_id && bone.parent.as_deref() == Some(parent_id)
        })
    };
    let rigid_fragment = |side: &str| {
        let layer_id = format!("a25d-handwear-{side}");
        layers.iter().any(|layer| {
            layer.id == layer_id
                && layer.bone_handles.len() == 1
                && layer.bone_handles[0].bone_id == layer_id
        }) && parent_is(&layer_id, "a25d-handwear")
    };
    let forbidden_bone = bones.iter().any(|bone| {
        let id = bone.id.to_ascii_lowercase();
        [
            "shoulder",
            "upper-arm",
            "upper_arm",
            "elbow",
            "forearm",
            "wrist",
            "thigh",
            "knee",
            "calf",
            "ankle",
            "leg",
            "foot",
        ]
        .iter()
        .any(|segment| id.contains(segment))
    });
    let forbidden_layer = layers.iter().any(|layer| {
        let id = layer.id.to_ascii_lowercase();
        id.contains("legwear") || id.contains("footwear")
    });
    let canonical_skeleton = parent_is("body", "root")
        && parent_is("head", "body")
        && parent_is("face", "head")
        && parent_is("left-eye", "face")
        && parent_is("right-eye", "face")
        && parent_is("mouth", "face")
        && parent_is("a25d-handwear", "body");
    let required_layers = has_layer("a25d-face")
        && has_layer("a25d-front-hair")
        && has_layer("a25d-back-hair")
        && has_layer("a25d-topwear")
        && has_variant("eye-left", "open")
        && has_variant("eye-left", "closed")
        && has_variant("eye-right", "open")
        && has_variant("eye-right", "closed")
        && has_variant("mouth", "open")
        && has_variant("mouth", "closed")
        && rigid_fragment("left")
        && rigid_fragment("right");
    if canonical_skeleton && required_layers && !forbidden_bone && !forbidden_layer {
        Ok(())
    } else {
        Err(RigValidationError::AssetContract)
    }
}

pub fn compile_layered_rig(source: RigCompileSource) -> Result<RigManifest, RigCompileError> {
    if source
        .rig_ir_version
        .is_some_and(|version| version != RIG_IR_VERSION)
    {
        return Err(RigCompileError::IrVersion);
    }
    if source.character_asset_contract_version != Some(CHARACTER_ASSET_CONTRACT_VERSION)
        || source
            .source_master_asset_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty() || value.len() > 512)
        || source
            .source_generation_fingerprint
            .as_deref()
            .is_some_and(|value| {
                value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit())
            })
        || (source.canvas.width - PORTRAIT_CANVAS_WIDTH).abs() > 0.0001
        || (source.canvas.height - PORTRAIT_CANVAS_HEIGHT).abs() > 0.0001
    {
        return Err(RigCompileError::AssetContract);
    }
    if source.bones.is_empty()
        || source.bones.len() > MAX_RIG_BONES
        || source.textures.is_empty()
        || source.textures.len() > MAX_RIG_TEXTURES
        || source.layers.is_empty()
        || source.layers.len() > MAX_RIG_PARTS
        || source
            .layers
            .iter()
            .map(|layer| {
                layer
                    .mesh
                    .as_ref()
                    .map(|mesh| mesh.vertices.len())
                    .unwrap_or_else(|| layer.contours.iter().map(Vec::len).sum())
            })
            .sum::<usize>()
            > MAX_RIG_TOTAL_VERTICES
    {
        return Err(RigCompileError::Complexity);
    }
    let bone_indexes = source
        .bones
        .iter()
        .enumerate()
        .map(|(index, bone)| (bone.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut parts = Vec::with_capacity(source.layers.len());
    for layer in source.layers {
        let explicit_mesh_valid = layer.mesh.as_ref().is_some_and(|mesh| {
            mesh.vertices.len() >= 3
                && mesh.vertices.len() <= MAX_RIG_VERTICES_PER_PART
                && mesh.indices.len() >= 3
                && mesh.indices.len() % 3 == 0
                && mesh.vertices.iter().all(|point| point_is_finite(*point))
                && mesh
                    .indices
                    .iter()
                    .all(|index| usize::from(*index) < mesh.vertices.len())
        });
        let contours_valid = !layer.contours.is_empty()
            && layer.contours[0].len() >= 3
            && layer
                .contours
                .iter()
                .flatten()
                .all(|point| point_is_finite(*point));
        if (layer.mesh.is_some() && !explicit_mesh_valid)
            || (layer.mesh.is_none() && !contours_valid)
        {
            return Err(RigCompileError::Contour);
        }
        if !rect_is_valid(layer.texture_bounds) {
            return Err(RigCompileError::TextureBounds);
        }
        if layer.bone_handles.is_empty()
            || layer.bone_handles.iter().any(|handle| {
                !bone_indexes.contains_key(handle.bone_id.as_str())
                    || !point_is_finite(handle.start)
                    || !point_is_finite(handle.end)
                    || !handle.falloff.is_finite()
                    || handle.falloff <= 0.0
            })
        {
            return Err(RigCompileError::BoneHandle);
        }
        let (points, indices) = if let Some(mesh) = layer.mesh {
            (mesh.vertices, mesh.indices)
        } else {
            let mut points = Vec::new();
            let mut hole_indices = Vec::new();
            for (index, contour) in layer.contours.iter().enumerate() {
                if index > 0 {
                    hole_indices.push(points.len());
                }
                points.extend_from_slice(contour);
            }
            if points.len() > usize::from(u16::MAX) {
                return Err(RigCompileError::MeshTooLarge);
            }
            let coordinates = points
                .iter()
                .flat_map(|point| [f64::from(point.x), f64::from(point.y)])
                .collect::<Vec<_>>();
            let raw_indices = earcutr::earcut(&coordinates, &hole_indices, 2)
                .map_err(|_| RigCompileError::Triangulation)?;
            if raw_indices.len() < 3 {
                return Err(RigCompileError::Triangulation);
            }
            let indices = raw_indices
                .into_iter()
                .map(|index| u16::try_from(index).map_err(|_| RigCompileError::MeshTooLarge))
                .collect::<Result<Vec<_>, _>>()?;
            (points, indices)
        };
        let bounds = point_bounds(&points).ok_or(RigCompileError::Contour)?;
        let weights = compute_bounded_weights(
            &points,
            &indices,
            &layer.bone_handles,
            &bone_indexes,
            source.bones.len(),
        );
        let vertices = points
            .iter()
            .zip(weights)
            .map(|(position, influences)| RigVertex {
                position: *position,
                uv: RigPoint {
                    x: layer.texture_bounds.x
                        + ((position.x - bounds.x) / bounds.width) * layer.texture_bounds.width,
                    y: layer.texture_bounds.y
                        + ((position.y - bounds.y) / bounds.height) * layer.texture_bounds.height,
                },
                joints: influences.0,
                weights: influences.1,
            })
            .collect();
        parts.push(RigPart {
            id: layer.id,
            texture_id: layer.texture_id,
            z_index: layer.z_index,
            opacity: layer.opacity,
            slot: layer.slot,
            variant: layer.variant,
            vertices,
            indices,
        });
    }
    let outfit_profile = source
        .outfit_profile
        .or_else(|| Some(infer_outfit_profile(&parts)));
    let secondary = outfit_profile
        .as_ref()
        .map(|profile| profile.secondary_part_ids.as_slice())
        .unwrap_or_default();
    let semantics = source
        .semantics
        .or_else(|| Some(default_rig_semantics(&source.bones, secondary)));
    let semantic_anchors = if source.semantic_anchors.is_empty() {
        default_semantic_anchors(&source.bones, semantics.as_ref())
    } else {
        source.semantic_anchors
    };
    let spatial_profile = source.spatial_profile.or_else(|| {
        Some(infer_spatial_profile(
            source.canvas,
            &source.bones,
            &parts,
            semantics.as_ref(),
        ))
    });
    let manifest = RigManifest {
        schema_version: RIG_SCHEMA_VERSION,
        rig_ir_version: source.rig_ir_version.or(Some(RIG_IR_VERSION)),
        character_asset_contract_version: source.character_asset_contract_version,
        source_master_asset_id: source.source_master_asset_id,
        source_generation_fingerprint: source.source_generation_fingerprint,
        quality: RigQuality::Layered2d,
        canvas: source.canvas,
        textures: source.textures,
        bones: source.bones,
        parts,
        clips: source.clips,
        default_clip: source.default_clip,
        standard_clip_library_version: None,
        motion_profile: source.motion_profile,
        outfit_profile,
        semantic_anchors,
        semantics,
        spatial_profile,
    };
    manifest.validate()?;
    Ok(manifest)
}

fn compute_bounded_weights(
    points: &[RigPoint],
    indices: &[u16],
    handles: &[RigBoneHandle],
    bone_indexes: &HashMap<&str, usize>,
    bone_count: usize,
) -> Vec<([u8; 4], [f32; 4])> {
    let mut weights = points
        .iter()
        .map(|point| raw_handle_weights(*point, handles, bone_indexes, bone_count))
        .collect::<Vec<_>>();
    let adjacency = mesh_adjacency(points.len(), indices);
    let anchors = points
        .iter()
        .map(|point| {
            let nearby = handles
                .iter()
                .filter_map(|handle| {
                    let bone = *bone_indexes.get(handle.bone_id.as_str())?;
                    let distance = point_segment_distance(*point, handle.start, handle.end);
                    (distance <= handle.falloff * 0.12).then_some((bone, distance))
                })
                .collect::<Vec<_>>();
            // Pin only vertices that unambiguously belong to one handle.
            // Overlap regions are precisely where a joint needs blended
            // weights; choosing the nearest handle there made layered seams
            // rigid even when the source supplied multiple influences.
            (nearby.len() == 1).then(|| nearby[0].0)
        })
        .collect::<Vec<_>>();
    for _ in 0..32 {
        let previous = weights.clone();
        for index in 0..weights.len() {
            if let Some(anchor) = anchors[index] {
                weights[index].fill(0.0);
                weights[index][anchor] = 1.0;
                continue;
            }
            if adjacency[index].is_empty() {
                continue;
            }
            for bone in 0..bone_count {
                let neighbor_average = adjacency[index]
                    .iter()
                    .map(|neighbor| previous[*neighbor][bone])
                    .sum::<f32>()
                    / adjacency[index].len() as f32;
                weights[index][bone] =
                    (previous[index][bone] * 0.58 + neighbor_average * 0.42).clamp(0.0, 1.0);
            }
            normalize_weights(&mut weights[index]);
        }
    }
    weights
        .into_iter()
        .map(|weights| top_four_weights(&weights))
        .collect()
}

fn raw_handle_weights(
    point: RigPoint,
    handles: &[RigBoneHandle],
    bone_indexes: &HashMap<&str, usize>,
    bone_count: usize,
) -> Vec<f32> {
    let mut weights = vec![0.0; bone_count];
    for handle in handles {
        let bone = bone_indexes[handle.bone_id.as_str()];
        let distance = point_segment_distance(point, handle.start, handle.end) / handle.falloff;
        weights[bone] += 1.0 / (0.04 + distance * distance);
    }
    normalize_weights(&mut weights);
    weights
}

fn top_four_weights(weights: &[f32]) -> ([u8; 4], [f32; 4]) {
    let mut ranked = weights.iter().copied().enumerate().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    let mut joints = [0_u8; 4];
    let mut selected = [0.0_f32; 4];
    for (slot, (bone, weight)) in ranked.into_iter().take(4).enumerate() {
        joints[slot] = u8::try_from(bone).unwrap_or(0);
        selected[slot] = weight;
    }
    let total = selected.iter().sum::<f32>();
    if total > f32::EPSILON {
        selected.iter_mut().for_each(|weight| *weight /= total);
    } else {
        selected[0] = 1.0;
    }
    (joints, selected)
}

fn mesh_adjacency(vertex_count: usize, indices: &[u16]) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); vertex_count];
    for triangle in indices.chunks_exact(3) {
        for (from, to) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let from = usize::from(from);
            let to = usize::from(to);
            if !adjacency[from].contains(&to) {
                adjacency[from].push(to);
            }
            if !adjacency[to].contains(&from) {
                adjacency[to].push(from);
            }
        }
    }
    adjacency
}

fn normalize_weights(weights: &mut [f32]) {
    let total = weights.iter().sum::<f32>();
    if total > f32::EPSILON {
        weights.iter_mut().for_each(|weight| *weight /= total);
    } else if let Some(first) = weights.first_mut() {
        *first = 1.0;
    }
}

fn point_segment_distance(point: RigPoint, start: RigPoint, end: RigPoint) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return ((point.x - start.x).powi(2) + (point.y - start.y).powi(2)).sqrt();
    }
    let amount =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    let closest = RigPoint {
        x: start.x + dx * amount,
        y: start.y + dy * amount,
    };
    ((point.x - closest.x).powi(2) + (point.y - closest.y).powi(2)).sqrt()
}

fn point_bounds(points: &[RigPoint]) -> Option<RigRect> {
    let first = *points.first()?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in &points[1..] {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    let width = max_x - min_x;
    let height = max_y - min_y;
    (width > f32::EPSILON && height > f32::EPSILON).then_some(RigRect {
        x: min_x,
        y: min_y,
        width,
        height,
    })
}

fn rect_is_valid(rect: RigRect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width > 0.0
        && rect.height > 0.0
        && rect.x >= 0.0
        && rect.y >= 0.0
        && rect.x + rect.width <= 1.0
        && rect.y + rect.height <= 1.0
}

fn fallback_clips() -> Vec<RigClip> {
    vec![
        clip(
            "idle",
            4.0,
            true,
            vec![
                track(
                    "body",
                    &[
                        (0.0, transform(0.0, 0.0, -0.006, 1.0, 1.0)),
                        (2.0, transform(0.0, -0.004, 0.008, 1.003, 1.003)),
                        (4.0, transform(0.0, 0.0, -0.006, 1.0, 1.0)),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, transform(0.0, 0.0, 0.010, 1.0, 1.0)),
                        (2.0, transform(0.0, -0.002, -0.012, 1.0, 1.0)),
                        (4.0, transform(0.0, 0.0, 0.010, 1.0, 1.0)),
                    ],
                ),
            ],
        ),
        clip(
            "thinking",
            2.8,
            true,
            vec![
                track(
                    "body",
                    &[
                        (0.0, transform(-0.004, 0.0, -0.012, 1.0, 1.0)),
                        (1.4, transform(0.004, -0.002, -0.020, 1.0, 1.0)),
                        (2.8, transform(-0.004, 0.0, -0.012, 1.0, 1.0)),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, transform(-0.004, 0.0, -0.032, 1.0, 1.0)),
                        (1.4, transform(0.002, 0.003, -0.045, 1.0, 1.0)),
                        (2.8, transform(-0.004, 0.0, -0.032, 1.0, 1.0)),
                    ],
                ),
            ],
        ),
        clip(
            "talking",
            0.72,
            true,
            vec![
                track(
                    "body",
                    &[
                        (0.0, transform(0.0, 0.0, -0.008, 1.0, 1.0)),
                        (0.36, transform(0.0, -0.003, 0.008, 1.002, 1.002)),
                        (0.72, transform(0.0, 0.0, -0.008, 1.0, 1.0)),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, transform(0.0, 0.0, 0.018, 1.0, 1.0)),
                        (0.36, transform(0.0, -0.004, -0.014, 1.0, 1.0)),
                        (0.72, transform(0.0, 0.0, 0.018, 1.0, 1.0)),
                    ],
                ),
            ],
        ),
        clip(
            "greet",
            1.3,
            false,
            vec![
                track(
                    "body",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.35, transform(0.0, -0.006, 0.028, 1.0, 1.0)),
                        (0.8, transform(0.0, -0.002, -0.018, 1.0, 1.0)),
                        (1.3, RigTransform::IDENTITY),
                    ],
                ),
                track(
                    "head",
                    &[
                        (0.0, RigTransform::IDENTITY),
                        (0.35, transform(0.0, -0.008, -0.045, 1.0, 1.0)),
                        (0.8, transform(0.0, -0.003, 0.032, 1.0, 1.0)),
                        (1.3, RigTransform::IDENTITY),
                    ],
                ),
            ],
        ),
    ]
}

fn clip(id: &str, duration: f32, looping: bool, tracks: Vec<RigTrack>) -> RigClip {
    RigClip {
        id: id.to_string(),
        duration,
        looping,
        tracks,
        presentation: None,
        events: Vec::new(),
        generation: None,
    }
}

fn attach_standard_clip_metadata(clips: &mut [RigClip]) {
    for clip in clips {
        clip.events = standard_clip_events(&clip.id, clip.duration);
        clip.presentation = standard_clip_presentation(&clip.id);
    }
}

fn standard_clip_events(id: &str, duration: f32) -> Vec<RigClipEvent> {
    let events: &[(&str, u32, f32)] = match id {
        "nod" => &[("contact-nod", 360, 0.42)],
        "bow" => &[("contact-bow", 620, 0.46)],
        _ => &[],
    };
    let duration_ms = (duration * 1_000.0).max(1.0);
    events
        .iter()
        .filter_map(|(kind, at_ms, intensity)| {
            let progress = *at_ms as f32 / duration_ms;
            (progress <= 1.0).then_some(RigClipEvent {
                progress,
                kind: (*kind).to_string(),
                intensity: *intensity,
            })
        })
        .collect()
}

fn standard_clip_presentation(id: &str) -> Option<RigClipPresentation> {
    use RigExpressionPresentation::{Happy, Neutral, Sad, Surprise};

    let expression = match id {
        "surprise" | "poke-reaction" | "startle-settle" => Some(Surprise),
        "happy" | "proud" => Some(Happy),
        "sad" | "shy" | "sigh" => Some(Sad),
        _ => None,
    };
    expression.map(|expression| {
        let (enter, hold_until) = match id {
            "startle-settle" | "poke-reaction" => (0.08, 0.72),
            "sigh" | "shy" => (0.22, 0.78),
            _ => (0.16, 0.82),
        };
        let keyframe = |progress, active: bool| RigPresentationKeyframe {
            progress,
            expression: Some(if active { expression } else { Neutral }),
        };
        RigClipPresentation {
            expression: Some(expression),
            keyframes: vec![
                keyframe(0.0, false),
                keyframe(enter, true),
                keyframe(hold_until, true),
                keyframe(1.0, false),
            ],
        }
    })
}

fn track(bone_id: &str, keyframes: &[(f32, RigTransform)]) -> RigTrack {
    RigTrack {
        bone_id: bone_id.to_string(),
        keyframes: keyframes
            .iter()
            .map(|(time, transform)| RigKeyframe {
                time: *time,
                transform: *transform,
            })
            .collect(),
    }
}

fn transform(x: f32, y: f32, rotation: f32, scale_x: f32, scale_y: f32) -> RigTransform {
    RigTransform {
        translation: RigPoint { x, y },
        rotation,
        scale: RigPoint {
            x: scale_x,
            y: scale_y,
        },
    }
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
) -> Result<HashSet<&'a str>, RigValidationError> {
    let mut unique = HashSet::new();
    for id in ids {
        if id.trim().is_empty() || !unique.insert(id) {
            return Err(RigValidationError::Identifier);
        }
    }
    Ok(unique)
}

fn validate_bone_cycles(bones: &[RigBone]) -> Result<(), RigValidationError> {
    let parents = bones
        .iter()
        .map(|bone| (bone.id.as_str(), bone.parent.as_deref()))
        .collect::<HashMap<_, _>>();
    for bone in bones {
        let mut seen = HashSet::new();
        let mut current = Some(bone.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(RigValidationError::BoneHierarchy);
            }
            current = parents.get(id).copied().flatten();
        }
    }
    Ok(())
}

fn point_is_finite(point: RigPoint) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn transform_is_valid(transform: RigTransform) -> bool {
    point_is_finite(transform.translation)
        && point_is_finite(transform.scale)
        && transform.rotation.is_finite()
        && transform.scale.x > 0.0
        && transform.scale.y > 0.0
}

fn is_rigid_arm_fragment(bone_id: &str) -> bool {
    matches!(
        bone_id,
        "a25d-handwear-left" | "a25d-handwear-right"
    )
}

fn clip_presentation_is_valid(presentation: &RigClipPresentation) -> bool {
    let owns_channel = presentation.expression.is_some();
    let keyframes = &presentation.keyframes;
    (owns_channel || !keyframes.is_empty())
        && (keyframes.is_empty()
            || (keyframes.len() >= 2
                && keyframes.len() <= 16
                && keyframes
                    .first()
                    .is_some_and(|keyframe| keyframe.progress == 0.0)
                && keyframes
                    .last()
                    .is_some_and(|keyframe| keyframe.progress == 1.0)
                && keyframes.iter().all(|keyframe| {
                    keyframe.progress.is_finite()
                        && (0.0..=1.0).contains(&keyframe.progress)
                        && keyframe.expression.is_some()
                })
                && keyframes
                    .windows(2)
                    .all(|pair| pair[0].progress < pair[1].progress)))
}

fn motion_profile_is_valid(profile: &RigMotionProfile) -> bool {
    profile.breath.min_frequency_hz.is_finite()
        && (0.05..=2.0).contains(&profile.breath.min_frequency_hz)
        && profile.breath.max_frequency_hz.is_finite()
        && (profile.breath.min_frequency_hz..=2.0).contains(&profile.breath.max_frequency_hz)
        && profile.breath.amplitude.is_finite()
        && (0.0..=0.05).contains(&profile.breath.amplitude)
        && profile.blink.min_interval_seconds.is_finite()
        && (0.5..=30.0).contains(&profile.blink.min_interval_seconds)
        && profile.blink.max_interval_seconds.is_finite()
        && (profile.blink.min_interval_seconds..=30.0).contains(&profile.blink.max_interval_seconds)
        && profile.blink.duration_seconds.is_finite()
        && (0.05..=1.0).contains(&profile.blink.duration_seconds)
        && profile.blink.double_chance.is_finite()
        && (0.0..=1.0).contains(&profile.blink.double_chance)
        && profile.secondary.frequency_hz.is_finite()
        && (0.1..=12.0).contains(&profile.secondary.frequency_hz)
        && profile.secondary.damping_ratio.is_finite()
        && (0.05..=3.0).contains(&profile.secondary.damping_ratio)
        && profile.secondary.response.is_finite()
        && (0.0..=2.0).contains(&profile.secondary.response)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let value = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_rig_is_valid_and_gpu_bounded() {
        let rig = build_portrait_fallback_rig("/portrait.png");
        rig.validate().unwrap();
        assert_eq!(rig.quality, RigQuality::PortraitFallback);
        assert_eq!(
            rig.canvas,
            RigSize {
                width: 1.0,
                height: 1.3333334
            }
        );
        assert_eq!(rig.textures[0].height, 1536);
        assert_eq!(
            rig.parts[0].vertices.last().unwrap().position.y,
            1.3333334
        );
        assert_eq!(rig.parts[0].vertices.last().unwrap().uv.y, 1.0);
        assert_eq!(rig.bones.len(), 3);
        assert_eq!(rig.parts[0].vertices.len(), 117);
        assert_eq!(rig.parts[0].indices.len(), 576);
        assert!(rig.clips.iter().any(|clip| clip.id == "idle-accent"));
        assert!(rig.clips.iter().any(|clip| clip.id == "nod"));
        assert!(!rig.clips.iter().any(|clip| clip.id == "wave"));
        assert!(!rig.clips.iter().any(|clip| clip.id == "weight-shift"));
    }

    #[test]
    fn generation_fingerprint_is_carried_and_validated() {
        let fingerprint = "a".repeat(64);
        let mut rig = build_portrait_fallback_rig_with_generation(
            "/portrait.png",
            Some(fingerprint.clone()),
        );
        rig.validate().unwrap();
        assert_eq!(rig.source_generation_fingerprint, Some(fingerprint));

        rig.source_generation_fingerprint = Some("not-a-sha256".to_string());
        assert_eq!(rig.validate(), Err(RigValidationError::AssetContract));
    }

    #[test]
    fn presentation_slots_require_known_variants_and_stable_fallbacks() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        rig.parts[0].slot = Some("mouth".to_string());
        rig.parts[0].variant = Some("open".to_string());
        assert_eq!(rig.validate(), Err(RigValidationError::Mesh));
        rig.parts[0].variant = Some("closed".to_string());
        rig.validate().unwrap();
        rig.parts[0].variant = Some("invented".to_string());
        assert_eq!(rig.validate(), Err(RigValidationError::Mesh));
        rig.parts[0].slot = Some("invented-slot".to_string());
        assert_eq!(rig.validate(), Err(RigValidationError::Mesh));
    }

    #[test]
    fn migration_enriches_legacy_rig_once_without_replacing_custom_clips() {
        let mut legacy = build_portrait_fallback_rig("/portrait.png");
        let canonical_idle = legacy
            .clips
            .iter()
            .find(|clip| clip.id == "idle")
            .unwrap()
            .clone();
        let mut stale_idle = canonical_idle.clone();
        stale_idle.tracks[0].keyframes[1].transform.rotation += 0.25;
        let mut custom = canonical_idle.clone();
        custom.id = "custom-dance".to_string();
        legacy.clips = vec![stale_idle, custom.clone()];
        legacy.default_clip = "idle".to_string();
        legacy.standard_clip_library_version = Some(1);
        legacy.motion_profile = None;
        let (migrated, changed) = migrate_rig_manifest(legacy, 42).unwrap();
        assert!(changed);
        assert_eq!(migrated.motion_profile.as_ref().unwrap().seed, 42);
        assert_eq!(
            migrated.clips.iter().find(|clip| clip.id == "idle"),
            Some(&canonical_idle)
        );
        assert_eq!(
            migrated.clips.iter().find(|clip| clip.id == "custom-dance"),
            Some(&custom)
        );
        assert!(migrated.clips.iter().any(|clip| clip.id == "talking"));
        assert_eq!(
            migrated.standard_clip_library_version,
            Some(STANDARD_CLIP_LIBRARY_VERSION)
        );
        let clip_count = migrated.clips.len();
        let (repeated, changed_again) = migrate_rig_manifest(migrated, 99).unwrap();
        assert!(!changed_again);
        assert_eq!(repeated.clips.len(), clip_count);
        assert_eq!(repeated.motion_profile.as_ref().unwrap().seed, 42);
    }

    #[test]
    fn migration_keeps_legacy_custom_presentation_slots_on_supported_ir() {
        let mut legacy = build_portrait_fallback_rig("/portrait.png");
        legacy.rig_ir_version = Some(MIN_SUPPORTED_RIG_IR_VERSION);
        legacy.parts[0].slot = Some("custom-emblem".to_string());
        legacy.parts[0].variant = Some("lit".to_string());
        let (migrated, _) = migrate_rig_manifest(legacy, 9).unwrap();
        assert_eq!(migrated.rig_ir_version, Some(MIN_SUPPORTED_RIG_IR_VERSION));
        assert_eq!(migrated.parts[0].slot.as_deref(), Some("custom-emblem"));
        assert_eq!(migrated.parts[0].variant.as_deref(), Some("lit"));
        migrated.validate().unwrap();
    }

    #[test]
    fn migration_does_not_claim_library_upgrade_when_clip_capacity_is_full() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        let idle = rig
            .clips
            .iter()
            .find(|clip| clip.id == "idle")
            .unwrap()
            .clone();
        rig.clips = vec![idle.clone()];
        for index in 1..MAX_RIG_CLIPS {
            let mut custom = idle.clone();
            custom.id = format!("custom-{index}");
            rig.clips.push(custom);
        }
        rig.standard_clip_library_version = Some(1);
        let (migrated, changed) = migrate_rig_manifest(rig, 7).unwrap();
        assert!(changed);
        assert_eq!(migrated.clips.len(), MAX_RIG_CLIPS);
        assert_eq!(migrated.standard_clip_library_version, Some(1));
        assert!(!migrated.clips.iter().any(|clip| clip.id == "talking"));
    }

    #[test]
    fn validates_optional_procedural_motion_profile() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        rig.motion_profile = Some(RigMotionProfile {
            seed: 42,
            breath: RigBreathMotionProfile {
                min_frequency_hz: 0.16,
                max_frequency_hz: 0.28,
                amplitude: 0.004,
            },
            blink: RigBlinkMotionProfile {
                min_interval_seconds: 2.5,
                max_interval_seconds: 7.0,
                duration_seconds: 0.24,
                double_chance: 0.15,
            },
            secondary: RigSecondaryMotionProfile {
                enabled: true,
                frequency_hz: 2.1,
                damping_ratio: 0.5,
                response: 0.6,
            },
        });
        rig.validate().unwrap();
        let encoded = serde_json::to_value(&rig).unwrap();
        assert_eq!(encoded["motionProfile"]["seed"], 42);
        assert_eq!(encoded["motionProfile"]["secondary"]["dampingRatio"], 0.5);
        rig.motion_profile.as_mut().unwrap().secondary.damping_ratio = 0.0;
        assert_eq!(rig.validate(), Err(RigValidationError::MotionProfile));
    }

    #[test]
    fn standard_actions_cover_upper_body_without_limb_clips() {
        let bones = [
            ("root", None),
            ("body", Some("root")),
            ("head", Some("body")),
            ("face", Some("head")),
            ("left-eye", Some("face")),
            ("right-eye", Some("face")),
            ("mouth", Some("face")),
            ("a25d-handwear", Some("body")),
        ]
        .into_iter()
        .map(|(id, parent)| RigBone {
            id: id.to_string(),
            parent: parent.map(str::to_string),
            pivot: RigPoint { x: 0.5, y: 0.5 },
        })
        .collect::<Vec<_>>();
        let clips = build_standard_face_rig_clips(&bones).unwrap();
        for expected in [
            "blink",
            "bow",
            "shake-head",
            "surprise",
            "shy",
            "proud",
            "sigh",
            "happy",
            "sad",
            "sleep",
            "observe",
            "pat-reaction",
            "poke-reaction",
            "look-around",
            "deep-breath",
            "startle-settle",
            "greet",
            "nod",
            "listen",
            "respond",
            "idle",
            "talking",
            "thinking",
            "idle-accent",
        ] {
            assert!(
                clips.iter().any(|clip| clip.id == expected),
                "{expected}"
            );
        }
        for retired in [
            "wave",
            "double-wave",
            "salute",
            "high-five",
            "beckon",
            "point-left",
            "self-introduce",
            "reassure",
            "facepalm",
            "hand-over-heart",
            "ponder",
            "gift-reaction",
            "shrug",
            "cheer",
            "clap",
            "present",
            "stretch",
            "step-left",
            "step-right",
            "bounce",
            "march-in-place",
            "weight-shift",
            "cautious-step",
            "sway",
        ] {
            assert!(
                !clips.iter().any(|clip| clip.id == retired),
                "{retired}"
            );
        }
        let talking = clips.iter().find(|clip| clip.id == "talking").unwrap();
        assert!(talking.tracks.iter().any(|track| track.bone_id == "mouth"));
        let happy = clips.iter().find(|clip| clip.id == "happy").unwrap();
        for bone_id in ["left-eye", "right-eye", "mouth"] {
            assert!(happy.tracks.iter().any(|track| track.bone_id == bone_id));
        }
        let bow = clips.iter().find(|clip| clip.id == "bow").unwrap();
        assert!(bow.tracks.iter().all(|track| track.keyframes.len() >= 7));
        assert!(bow.tracks.iter().all(|track| {
            let anticipation = &track.keyframes[1].transform;
            let peak = &track.keyframes[3].transform;
            anticipation.rotation * peak.rotation <= 0.0
                && anticipation.translation.x * peak.translation.x <= 0.0
                && anticipation.translation.y * peak.translation.y <= 0.0
        }));
        for expressive in ["look-around", "deep-breath"] {
            let clip = clips.iter().find(|clip| clip.id == expressive).unwrap();
            assert!(clip.tracks.iter().any(|track| track.keyframes.len() >= 6));
        }
        let shy = clips.iter().find(|clip| clip.id == "shy").unwrap();
        assert_eq!(
            shy.presentation.as_ref().unwrap().expression,
            Some(RigExpressionPresentation::Sad)
        );
    }

    #[test]
    fn standard_actions_reject_disconnected_face_rig_bones() {
        let bones = [
            ("root", None),
            ("body", Some("root")),
            ("head", Some("root")),
        ]
        .into_iter()
        .map(|(id, parent)| RigBone {
            id: id.to_string(),
            parent: parent.map(str::to_string),
            pivot: RigPoint { x: 0.5, y: 0.5 },
        })
        .collect::<Vec<_>>();
        assert_eq!(
            build_standard_face_rig_clips(&bones),
            Err(RigValidationError::BoneHierarchy)
        );
    }

    #[test]
    fn rejects_invalid_skin_weight_sum() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        rig.parts[0].vertices[0].weights = [0.2, 0.2, 0.2, 0.0];
        assert_eq!(rig.validate(), Err(RigValidationError::SkinWeights));
    }

    #[test]
    fn rejects_bone_cycles() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        rig.bones[0].parent = Some("head".to_string());
        assert_eq!(rig.validate(), Err(RigValidationError::BoneHierarchy));
    }

    #[test]
    fn character_asset_source_requires_real_face_layers_and_rigid_side_arms() {
        let bone = |id: &str, parent: Option<&str>| RigBone {
            id: id.to_string(),
            parent: parent.map(str::to_string),
            pivot: RigPoint { x: 0.5, y: 0.5 },
        };
        let layer = |id: &str, slot: Option<&str>, variant: Option<&str>, bone_id: &str| {
            RigLayerSource {
                id: id.to_string(),
                texture_id: "atlas".to_string(),
                texture_bounds: RigRect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                z_index: 0,
                opacity: 1.0,
                slot: slot.map(str::to_string),
                variant: variant.map(str::to_string),
                contours: Vec::new(),
                mesh: None,
                bone_handles: vec![RigBoneHandle {
                    bone_id: bone_id.to_string(),
                    start: RigPoint { x: 0.0, y: 0.0 },
                    end: RigPoint { x: 1.0, y: 1.0 },
                    falloff: 1.0,
                }],
            }
        };
        let mut bones = vec![
            bone("root", None),
            bone("body", Some("root")),
            bone("head", Some("body")),
            bone("face", Some("head")),
            bone("left-eye", Some("face")),
            bone("right-eye", Some("face")),
            bone("mouth", Some("face")),
            bone("a25d-handwear", Some("body")),
            bone("a25d-handwear-left", Some("a25d-handwear")),
            bone("a25d-handwear-right", Some("a25d-handwear")),
        ];
        let mut layers = vec![
            layer("a25d-face", None, None, "face"),
            layer("a25d-front-hair", None, None, "head"),
            layer("a25d-back-hair", None, None, "head"),
            layer("a25d-topwear", None, None, "body"),
            layer("a25d-eye-open-left", Some("eye-left"), Some("open"), "left-eye"),
            layer("a25d-eye-close-left", Some("eye-left"), Some("closed"), "left-eye"),
            layer("a25d-eye-open-right", Some("eye-right"), Some("open"), "right-eye"),
            layer("a25d-eye-close-right", Some("eye-right"), Some("closed"), "right-eye"),
            layer("a25d-mouth-open", Some("mouth"), Some("open"), "mouth"),
            layer("a25d-mouth-close", Some("mouth"), Some("closed"), "mouth"),
            layer("a25d-handwear-left", None, None, "a25d-handwear-left"),
            layer("a25d-handwear-right", None, None, "a25d-handwear-right"),
        ];
        validate_character_asset_source(&bones, &layers).unwrap();

        let right_arm = layers.pop().unwrap();
        assert_eq!(
            validate_character_asset_source(&bones, &layers),
            Err(RigValidationError::AssetContract)
        );
        layers.push(right_arm);
        bones.push(bone("left-shoulder", Some("body")));
        assert_eq!(
            validate_character_asset_source(&bones, &layers),
            Err(RigValidationError::AssetContract)
        );
    }

    #[test]
    fn compiles_contour_mesh_with_bounded_weights() {
        let bones = vec![
            RigBone {
                id: "left".to_string(),
                parent: None,
                pivot: RigPoint { x: 0.2, y: 0.5 },
            },
            RigBone {
                id: "right".to_string(),
                parent: None,
                pivot: RigPoint { x: 0.8, y: 0.5 },
            },
        ];
        let rig = compile_layered_rig(RigCompileSource {
            rig_ir_version: Some(RIG_IR_VERSION),
            character_asset_contract_version: Some(CHARACTER_ASSET_CONTRACT_VERSION),
            source_master_asset_id: Some("/portrait.png".to_string()),
            source_generation_fingerprint: None,
            canvas: RigSize {
                width: PORTRAIT_CANVAS_WIDTH,
                height: PORTRAIT_CANVAS_HEIGHT,
            },
            textures: vec![RigTexture {
                id: "atlas".to_string(),
                url: "/atlas.png".to_string(),
                width: 1024,
                height: 1024,
            }],
            bones,
            layers: vec![
                RigLayerSource {
                    id: "body".to_string(),
                    texture_id: "atlas".to_string(),
                    texture_bounds: RigRect {
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                    },
                    z_index: 0,
                    opacity: 1.0,
                    slot: None,
                    variant: None,
                    contours: vec![vec![
                        RigPoint { x: 0.1, y: 0.1 },
                        RigPoint { x: 0.9, y: 0.1 },
                        RigPoint { x: 0.9, y: 0.9 },
                        RigPoint { x: 0.1, y: 0.9 },
                    ]],
                    mesh: None,
                    bone_handles: vec![
                        RigBoneHandle {
                            bone_id: "left".to_string(),
                            start: RigPoint { x: 0.1, y: 0.2 },
                            end: RigPoint { x: 0.1, y: 0.8 },
                            falloff: 0.45,
                        },
                        RigBoneHandle {
                            bone_id: "right".to_string(),
                            start: RigPoint { x: 0.9, y: 0.2 },
                            end: RigPoint { x: 0.9, y: 0.8 },
                            falloff: 0.45,
                        },
                    ],
                },
                RigLayerSource {
                    id: "a25d-topwear".to_string(),
                    texture_id: "atlas".to_string(),
                    texture_bounds: RigRect {
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                    },
                    z_index: 1,
                    opacity: 1.0,
                    slot: None,
                    variant: None,
                    contours: vec![],
                    mesh: Some(RigLayerMeshSource {
                        vertices: vec![
                            RigPoint { x: 0.1, y: 0.1 },
                            RigPoint { x: 0.9, y: 0.1 },
                            RigPoint { x: 0.9, y: 0.9 },
                            RigPoint { x: 0.1, y: 0.9 },
                            RigPoint { x: 0.5, y: 0.5 },
                        ],
                        indices: vec![0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4],
                    }),
                    bone_handles: vec![
                        RigBoneHandle {
                            bone_id: "left".to_string(),
                            start: RigPoint { x: 0.1, y: 0.2 },
                            end: RigPoint { x: 0.1, y: 0.8 },
                            falloff: 0.45,
                        },
                        RigBoneHandle {
                            bone_id: "right".to_string(),
                            start: RigPoint { x: 0.9, y: 0.2 },
                            end: RigPoint { x: 0.9, y: 0.8 },
                            falloff: 0.45,
                        },
                    ],
                },
            ],
            clips: vec![clip(
                "idle",
                1.0,
                true,
                vec![track(
                    "left",
                    &[(0.0, RigTransform::IDENTITY), (1.0, RigTransform::IDENTITY)],
                )],
            )],
            default_clip: "idle".to_string(),
            motion_profile: None,
            outfit_profile: None,
            semantic_anchors: HashMap::new(),
            semantics: None,
            spatial_profile: None,
        })
        .unwrap();
        assert_eq!(rig.quality, RigQuality::Layered2d);
        assert_eq!(rig.parts[0].indices.len(), 6);
        assert_eq!(rig.parts[1].vertices.len(), 5);
        assert_eq!(rig.parts[1].indices.len(), 12);
        assert!(rig.parts[0].vertices.iter().any(|vertex| vertex
            .weights
            .iter()
            .filter(|weight| **weight > 0.02)
            .count()
            >= 2));
        assert!(rig.parts[0]
            .vertices
            .iter()
            .all(|vertex| (vertex.weights.iter().sum::<f32>() - 1.0).abs() < 0.002));
    }

    #[test]
    fn compile_entry_rejects_legacy_ir_even_though_stored_manifests_remain_readable() {
        let source = RigCompileSource {
            rig_ir_version: Some(MIN_SUPPORTED_RIG_IR_VERSION),
            character_asset_contract_version: None,
            source_master_asset_id: None,
            source_generation_fingerprint: None,
            canvas: RigSize {
                width: 1.0,
                height: 1.0,
            },
            textures: vec![],
            bones: vec![],
            layers: vec![],
            clips: vec![],
            default_clip: "idle".to_string(),
            motion_profile: None,
            outfit_profile: None,
            semantic_anchors: HashMap::new(),
            semantics: None,
            spatial_profile: None,
        };
        assert_eq!(compile_layered_rig(source), Err(RigCompileError::IrVersion));
    }

    #[test]
    fn outfit_profile_inference_limits_wide_sleeves_and_long_skirts() {
        let fallback = build_portrait_fallback_rig("/portrait.png");
        let mut sleeve = fallback.parts[0].clone();
        sleeve.id = "left-wide-sleeve".to_string();
        let mut skirt = fallback.parts[0].clone();
        skirt.id = "long-skirt-front".to_string();
        let profile = infer_outfit_profile(&[sleeve, skirt]);
        assert!(profile.topologies.contains(&RigOutfitTopology::WideSleeve));
        assert!(profile.topologies.contains(&RigOutfitTopology::LongSkirt));
        assert_eq!(profile.torso_twist_scale, 0.9);
        assert_eq!(profile.secondary_motion_scale, 0.78);
    }

    #[test]
    fn semantic_rig_adds_rigid_side_arm_tracks_bounded_to_fifteen_degrees() {
        let bones = [
            ("root", None),
            ("body", Some("root")),
            ("head", Some("body")),
            ("face", Some("head")),
            ("a25d-handwear", Some("body")),
            ("a25d-handwear-left", Some("a25d-handwear")),
            ("a25d-handwear-right", Some("a25d-handwear")),
        ]
        .into_iter()
        .map(|(id, parent)| RigBone {
            id: id.to_string(),
            parent: parent.map(str::to_string),
            pivot: RigPoint { x: 0.5, y: 0.5 },
        })
        .collect::<Vec<_>>();
        let semantics = default_rig_semantics(&bones, &[]);
        let clips = build_standard_face_rig_clips_for_semantics(&bones, Some(&semantics)).unwrap();
        let greet = clips.iter().find(|clip| clip.id == "greet").unwrap();
        for side in ["a25d-handwear-left", "a25d-handwear-right"] {
            let track = greet
                .tracks
                .iter()
                .find(|track| track.bone_id == side)
                .unwrap();
            assert!(track.keyframes.iter().all(|keyframe| {
                keyframe.transform.rotation.abs() <= MAX_RIGID_ARM_FRAGMENT_ROTATION
            }));
        }
        assert!(clips.iter().all(|clip| clip.tracks.iter().all(|track| {
            !is_rigid_arm_fragment(&track.bone_id)
                || track.keyframes.iter().all(|keyframe| {
                    keyframe.transform.rotation.abs() <= MAX_RIGID_ARM_FRAGMENT_ROTATION
                })
        })));
    }

    #[test]
    fn validation_rejects_rigid_side_arm_tracks_beyond_fifteen_degrees() {
        let mut rig = build_portrait_fallback_rig("/portrait.png");
        rig.bones.extend([
            RigBone {
                id: "a25d-handwear".to_string(),
                parent: Some("body".to_string()),
                pivot: RigPoint { x: 0.5, y: 0.8 },
            },
            RigBone {
                id: "a25d-handwear-left".to_string(),
                parent: Some("a25d-handwear".to_string()),
                pivot: RigPoint { x: 0.3, y: 0.7 },
            },
        ]);
        rig.semantics = Some(default_rig_semantics(&rig.bones, &[]));
        rig.clips = build_standard_face_rig_clips_for_semantics(
            &rig.bones,
            rig.semantics.as_ref(),
        )
        .unwrap();
        rig.validate().unwrap();
        let track = rig
            .clips
            .iter_mut()
            .find(|clip| clip.id == "greet")
            .unwrap()
            .tracks
            .iter_mut()
            .find(|track| track.bone_id == "a25d-handwear-left")
            .unwrap();
        track.keyframes[3].transform.rotation = MAX_RIGID_ARM_FRAGMENT_ROTATION + 0.01;
        assert_eq!(rig.validate(), Err(RigValidationError::Animation));
    }
}
