use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::rig_contract::CharacterAssetProfile;

const APPEARANCE_VISUAL_PROFILE_KEYS: &[&str] = &["gender", "visualIdentity"];

/// Keep only the fields that actually change generated pixels.
/// Gender is repeated as a hard pixel constraint. Raw onboarding requirements,
/// language, tag seeds, and persona extras stay on the profile for editing;
/// the confirmed visual identity is the sole resolved appearance source.
pub fn appearance_visual_profile(visual_profile: &Value) -> Value {
    let Some(source) = visual_profile.as_object() else {
        return visual_profile.clone();
    };
    let mut appearance = Map::new();
    for key in APPEARANCE_VISUAL_PROFILE_KEYS {
        if let Some(value) = source.get(*key) {
            let value = if *key == "visualIdentity" && !value.is_null() {
                if let Some(gender) = source.get("gender").and_then(Value::as_str)
                    && !crate::visual_prompt::visual_identity_matches_gender_presentation(
                        value, gender,
                    )
                {
                    continue;
                }
                let Some(normalized) =
                    crate::visual_prompt::normalize_visual_identity_for_prompt(value)
                else {
                    continue;
                };
                normalized
            } else {
                value.clone()
            };
            appearance.insert((*key).to_string(), value);
        }
    }
    Value::Object(appearance)
}

/// Immutable input snapshot for the generated bust master portrait.
///
/// The portrait URL identifies pixels; this contract identifies what those
/// pixels were supposed to depict and which downstream rig contract they use.
/// `slot` is always `"master"` so existing generation fingerprints remain valid.
pub fn build_character_asset_contract(
    name: &str,
    visual_profile: &Value,
    additional_requirements: Option<&str>,
) -> Value {
    build_character_asset_contract_with(
        name,
        visual_profile,
        additional_requirements,
        &RenderingProvenance::builtin(),
    )
}

/// [`build_character_asset_contract`] for a portrait drawn under `rendering`:
/// this build's own recipe, or one a recipe service supplied.
pub fn build_character_asset_contract_with(
    name: &str,
    visual_profile: &Value,
    additional_requirements: Option<&str>,
    rendering: &RenderingProvenance,
) -> Value {
    let bust = CharacterAssetProfile::Bust.contract();
    json!({
        "contractVersion": bust.contract_version,
        "slot": "master",
        "identity": {
            "name": bounded_text(name, 50),
            "visualProfile": appearance_visual_profile(visual_profile),
        },
        "output": {
            "width": bust.generation_width,
            "height": bust.generation_height,
            "portraitAspect": {
                "width": bust.aspect_width,
                "height": bust.aspect_height,
            },
            "rigCanvas": {
                "width": bust.canvas_width,
                "height": bust.canvas_height,
            },
            "framing": bust.framing,
            "view": "strict-centered-eye-level-zero-yaw-front",
            "background": bust.background,
        },
        "rendering": rendering.bust_block(),
        "rig": {
            "maxRigidArmRotationDegrees": bust.max_rigid_arm_rotation_degrees,
            "requiredCapabilities": bust.required_capabilities,
        },
        "additionalRequirements": additional_requirements
            .map(|value| bounded_text(value, 2_000))
            .filter(|value| !value.is_empty()),
    })
}

/// How a picture was rendered, as its generation contract records it in the
/// `rendering` block: which recipe drew it, the visual school version, the
/// style reference image (by hash) and the full-body instruction version.
///
/// `recipe: None` is this build's own recipe; its blocks carry no `recipe`
/// key. `recipe: Some(id)` names a recipe
/// service that supplied its own prompt, versions and reference; its blocks
/// carry `"recipe": id` as well, so they never collide with this build's.
/// The host writes a contract only from what it drew itself and keeps it
/// server-side; a `rendering` block is never taken from a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderingProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    pub visual_school_version: String,
    /// `None` when no style reference image was attached.
    #[serde(default)]
    pub style_reference_sha256: Option<String>,
    pub full_body_instruction_version: String,
}

/// Which part of a [`RenderingProvenance`] is not usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RenderingProvenanceIssue {
    #[error(
        "recipe id must be 1-40 lower-case letters, digits or '-', starting with a letter or digit, and not `builtin`"
    )]
    Recipe,
    #[error("visual school version must be 1-80 letters, digits or '.', '_', ':', '-'")]
    VisualSchoolVersion,
    #[error("style reference hash must be 64 lower-case hex digits")]
    StyleReferenceSha256,
    #[error("full-body instruction version must be 1-80 letters, digits or '.', '_', ':', '-'")]
    FullBodyInstructionVersion,
}

const RENDERING_TECHNIQUE_ONLY: &str = "rendering-technique-only";
/// The name of this build's own recipe; a recipe service may not take it.
pub const BUILTIN_RECIPE_ID: &str = "builtin";
const NO_STYLE_REFERENCE: &str = "none";

impl RenderingProvenance {
    /// This build's own recipe: the current recognised rendering.
    pub fn builtin() -> Self {
        Self::recognised(&crate::visual_prompt::MEROPE_RECOGNISED_PORTRAIT_RENDERINGS[0])
    }

    fn recognised(rendering: &crate::visual_prompt::PortraitRendering) -> Self {
        Self {
            recipe: None,
            visual_school_version: rendering.visual_school_version.to_string(),
            style_reference_sha256: Some(rendering.style_reference_sha256.to_string()),
            full_body_instruction_version:
                crate::visual_prompt::FULL_BODY_PORTRAIT_INSTRUCTION_VERSION.to_string(),
        }
    }

    /// Every field within its bounds. A host checks this on a provenance a
    /// recipe service sent before it draws anything under it.
    pub fn check(&self) -> Result<(), RenderingProvenanceIssue> {
        if let Some(recipe) = &self.recipe
            && !recipe_id_is_valid(recipe)
        {
            return Err(RenderingProvenanceIssue::Recipe);
        }
        if !version_is_valid(&self.visual_school_version) {
            return Err(RenderingProvenanceIssue::VisualSchoolVersion);
        }
        if let Some(sha) = &self.style_reference_sha256
            && !sha256_is_valid(sha)
        {
            return Err(RenderingProvenanceIssue::StyleReferenceSha256);
        }
        if !version_is_valid(&self.full_body_instruction_version) {
            return Err(RenderingProvenanceIssue::FullBodyInstructionVersion);
        }
        Ok(())
    }

    fn style_reference_role(&self) -> &'static str {
        style_reference_role(self.style_reference_sha256.as_deref())
    }

    fn with_recipe(&self, mut block: Value) -> Value {
        if let (Some(recipe), Some(object)) = (&self.recipe, block.as_object_mut()) {
            object.insert("recipe".into(), json!(recipe));
        }
        block
    }

    /// The `rendering` block of a bust master portrait's contract.
    pub fn bust_block(&self) -> Value {
        bust_block_of(
            self.recipe.as_deref(),
            &self.visual_school_version,
            self.style_reference_sha256.as_deref(),
        )
    }

    /// The `rendering` block of a full-body set drawn from the design.
    pub fn full_body_design_block(&self) -> Value {
        self.with_recipe(json!({
            "instructionVersion": self.full_body_instruction_version,
            "visualSchoolVersion": self.visual_school_version,
            "styleReferenceSha256": self.style_reference_sha256,
            "styleReferenceRole": self.style_reference_role(),
        }))
    }

    /// The `rendering` block of a full-body set redrawn from a bust portrait.
    pub fn full_body_from_bust_block(&self) -> Value {
        self.with_recipe(json!({
            "instructionVersion": self.full_body_instruction_version,
            "referenceRole": "bust-portrait-identity-anchor",
        }))
    }
}

fn style_reference_role(sha256: Option<&str>) -> &'static str {
    if sha256.is_some() {
        RENDERING_TECHNIQUE_ONLY
    } else {
        NO_STYLE_REFERENCE
    }
}

/// A bust contract's `rendering` block from its parts.
fn bust_block_of(recipe: Option<&str>, school_version: &str, sha256: Option<&str>) -> Value {
    let mut block = json!({
        "visualSchoolVersion": school_version,
        "styleReferenceSha256": sha256,
        "styleReferenceRole": style_reference_role(sha256),
    });
    if let (Some(recipe), Some(object)) = (recipe, block.as_object_mut()) {
        object.insert("recipe".into(), json!(recipe));
    }
    block
}

fn recipe_id_is_valid(value: &str) -> bool {
    value != BUILTIN_RECIPE_ID
        && (1..=40).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
}

fn version_is_valid(value: &str) -> bool {
    (1..=80).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        && value.as_bytes()[0].is_ascii_alphanumeric()
}

fn sha256_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether a stored bust contract's `rendering` block is exactly one this
/// build recognises (`MEROPE_RECOGNISED_PORTRAIT_RENDERINGS`). A missing,
/// malformed, or unlisted block is not.
pub fn is_recognised_portrait_rendering(rendering: &Value) -> bool {
    crate::visual_prompt::MEROPE_RECOGNISED_PORTRAIT_RENDERINGS
        .iter()
        .any(|known| RenderingProvenance::recognised(known).bust_block() == *rendering)
}

/// Whether a stored bust contract's `rendering` block is a well-formed block
/// a recipe service's provenance gives: exactly the keys
/// [`RenderingProvenance::bust_block`] writes, with `recipe` set, and every
/// value within bounds. Any version is accepted, so a recipe service moving
/// to a new version does not make its earlier portraits unrecognisable; the
/// identity the portrait depicts is still compared outside this block.
pub fn is_recipe_portrait_rendering(rendering: &Value) -> bool {
    let Some(object) = rendering.as_object() else {
        return false;
    };
    let Some(recipe) = object.get("recipe").and_then(Value::as_str) else {
        return false;
    };
    let Some(school_version) = object.get("visualSchoolVersion").and_then(Value::as_str) else {
        return false;
    };
    let sha256 = match object.get("styleReferenceSha256") {
        Some(Value::Null) => None,
        Some(Value::String(sha)) => Some(sha.as_str()),
        _ => return false,
    };
    recipe_id_is_valid(recipe)
        && version_is_valid(school_version)
        && sha256.is_none_or(sha256_is_valid)
        && bust_block_of(Some(recipe), school_version, sha256) == *rendering
}

/// Whether a stored bust contract's `rendering` block lets the portrait keep
/// its lineage: one this build recognises, or a well-formed recipe-service
/// block. Only for contracts the host wrote itself.
pub fn portrait_rendering_keeps_lineage(rendering: &Value) -> bool {
    is_recognised_portrait_rendering(rendering) || is_recipe_portrait_rendering(rendering)
}

/// What a full-body set is drawn from.
#[derive(Debug, Clone, Copy)]
pub enum FullBodySource<'a> {
    /// Redrawn from a bust set's master portrait, the identity anchor.
    Bust {
        portrait: &'a str,
        generation_fingerprint: Option<&'a str>,
    },
    /// Drawn from the confirmed design with this set's outfit on, like a bust
    /// master portrait.
    Design {
        name: &'a str,
        visual_profile: &'a Value,
    },
}

/// Immutable input snapshot for a full-body set's picture: another source, or
/// a new instruction, is a new figure.
pub fn build_full_body_asset_contract(source: FullBodySource<'_>) -> Value {
    build_full_body_asset_contract_with(source, &RenderingProvenance::builtin())
}

/// [`build_full_body_asset_contract`] for a figure drawn under `rendering`.
pub fn build_full_body_asset_contract_with(
    source: FullBodySource<'_>,
    rendering: &RenderingProvenance,
) -> Value {
    let full = CharacterAssetProfile::FullBody.contract();
    let (source, rendering) = match source {
        FullBodySource::Bust {
            portrait,
            generation_fingerprint,
        } => (
            json!({
                "bustPortrait": portrait,
                "bustGenerationFingerprint": generation_fingerprint,
            }),
            rendering.full_body_from_bust_block(),
        ),
        FullBodySource::Design {
            name,
            visual_profile,
        } => (
            json!({
                "identity": {
                    "name": bounded_text(name, 50),
                    "visualProfile": appearance_visual_profile(visual_profile),
                },
            }),
            rendering.full_body_design_block(),
        ),
    };
    json!({
        "contractVersion": full.contract_version,
        "slot": "fullBody",
        "source": source,
        "output": {
            "width": full.generation_width,
            "height": full.generation_height,
            "portraitAspect": {
                "width": full.aspect_width,
                "height": full.aspect_height,
            },
            "rigCanvas": {
                "width": full.canvas_width,
                "height": full.canvas_height,
            },
            "framing": full.framing,
            "background": full.background,
        },
        "rendering": rendering,
        "rig": {
            "maxRigidArmRotationDegrees": full.max_rigid_arm_rotation_degrees,
            "requiredCapabilities": full.required_capabilities,
        },
    })
}

pub fn character_asset_contract_fingerprint(contract: &Value) -> String {
    let encoded = serde_json::to_vec(contract).expect("character asset contract is serializable");
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut fingerprint, byte| {
            write!(&mut fingerprint, "{byte:02x}").expect("write fingerprint");
            fingerprint
        })
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

    fn complete_profile(face_design: &str, material_plan: &str) -> Value {
        json!({
            "gender": "female",
            "visualIdentity": {
                "faceDesign": format!("女性化读取，{face_design}"),
                "eyeDesign": "中等偏大的紫色宝石眼，视线坚定",
                "hairShape": "银灰齐颌短发与偏分刘海",
                "hairLayerPlan": "后发、刘海和左右侧发形成独立轮廓",
                "upperBodySilhouette": "紧凑肩线、清楚领口与胸前焦点",
                "outfitConstruction": "敞开领口内搭叠短外套并止于高腰",
                "sleeveArmDesign": "左右袖片携局部前臂进入画面",
                "materialPlan": material_plan,
                "heroAccessory": "左胸星轨扣饰",
                "paletteHint": "雾蓝为主、银白为辅、金色点缀",
                "motif": "单一星轨弧线集中在胸前"
            }
        })
    }

    #[test]
    fn master_contract_carries_identity_output_and_rig_invariants() {
        let contract = build_character_asset_contract(
            " Nova ",
            &json!({ "gender": "nonbinary" }),
            Some(" gold eyes "),
        );
        assert_eq!(contract["slot"], "master");
        assert_eq!(contract["identity"]["name"], "Nova");
        assert_eq!(contract["identity"]["visualProfile"]["gender"], "nonbinary");
        assert_eq!(contract["output"]["width"], 1152);
        assert_eq!(contract["output"]["height"], 1536);
        assert_eq!(
            contract["output"]["portraitAspect"],
            json!({ "width": 3, "height": 4 })
        );
        assert_eq!(contract["output"]["rigCanvas"]["width"], 1.0);
        assert!(
            (contract["output"]["rigCanvas"]["height"].as_f64().unwrap() - (4.0 / 3.0)).abs()
                < 0.000_001
        );
        assert_eq!(contract["output"]["background"], "clean-near-white");
        assert_eq!(
            contract["rendering"]["visualSchoolVersion"],
            crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION
        );
        assert_eq!(
            contract["rendering"]["styleReferenceSha256"],
            crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256
        );
        assert_eq!(
            contract["rendering"]["styleReferenceRole"],
            "rendering-technique-only"
        );
        assert_eq!(
            contract["output"]["view"],
            "strict-centered-eye-level-zero-yaw-front"
        );
        assert_eq!(
            contract["output"]["framing"],
            "close-full-head-through-lower-chest"
        );
        assert_eq!(contract["rig"]["maxRigidArmRotationDegrees"], 15.0);
        assert_eq!(character_asset_contract_fingerprint(&contract).len(), 64);
    }

    /// Stored fingerprints are hashes of this exact contract layout. Outside
    /// the `rendering` block (whose recognised values are listed in
    /// `MEROPE_RECOGNISED_PORTRAIT_RENDERINGS`), any changed byte gives the
    /// same identity a different fingerprint. The school version is pinned
    /// here so a rendering bump does not move this hash.
    #[test]
    fn bust_contract_fingerprint_is_unchanged_by_the_profile_split() {
        let mut contract = build_character_asset_contract(
            "Nova",
            &json!({ "gender": "female", "visualIdentity": { "hairShape": "bob" } }),
            Some("extra"),
        );
        contract["rendering"]["visualSchoolVersion"] = json!("pinned-school");
        assert_eq!(
            character_asset_contract_fingerprint(&contract),
            "9f1d7a72466d522c577ad42f472ef51dcc70250a921092817b0eb02bf46bd7aa"
        );
    }

    #[test]
    fn a_new_bust_contract_carries_the_current_recognised_rendering() {
        let contract = build_character_asset_contract("Nova", &json!({ "gender": "female" }), None);
        let current = crate::visual_prompt::MEROPE_RECOGNISED_PORTRAIT_RENDERINGS[0];
        assert_eq!(
            current.visual_school_version,
            crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION
        );
        assert_eq!(
            current.style_reference_sha256,
            crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256
        );
        assert!(is_recognised_portrait_rendering(&contract["rendering"]));
    }

    #[test]
    fn the_rendering_before_this_school_version_is_recognised() {
        assert!(is_recognised_portrait_rendering(&json!({
            "visualSchoolVersion": "mihoyo-rpg-reference-v4",
            "styleReferenceSha256": crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256,
            "styleReferenceRole": "rendering-technique-only",
        })));
    }

    #[test]
    fn an_unlisted_rendering_is_not_recognised() {
        assert!(!is_recognised_portrait_rendering(&json!({
            "visualSchoolVersion": "mihoyo-rpg-reference-v3",
            "styleReferenceSha256": crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256,
            "styleReferenceRole": "rendering-technique-only",
        })));
        assert!(!is_recognised_portrait_rendering(&json!({
            "visualSchoolVersion": crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION,
            "styleReferenceSha256": "0".repeat(64),
            "styleReferenceRole": "rendering-technique-only",
        })));
    }

    #[test]
    fn a_malformed_rendering_is_not_recognised() {
        let current = build_character_asset_contract("Nova", &json!({}), None)["rendering"].clone();
        let mut with_extra_key = current.clone();
        with_extra_key["note"] = json!("x");
        let mut without_role = current;
        without_role
            .as_object_mut()
            .unwrap()
            .remove("styleReferenceRole");
        for malformed in [
            Value::Null,
            json!("anime-cel-gradient-reference-v5"),
            json!({}),
            with_extra_key,
            without_role,
        ] {
            assert!(!is_recognised_portrait_rendering(&malformed), "{malformed}");
        }
    }

    /// This build's own blocks: the current school version and reference,
    /// no `recipe` key, and the wrappers equal the `_with` builders given
    /// [`RenderingProvenance::builtin`].
    #[test]
    fn builtin_rendering_blocks_carry_the_current_school_and_no_recipe() {
        let builtin = RenderingProvenance::builtin();
        let sha = crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256;
        let school = crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION;
        let instruction = crate::visual_prompt::FULL_BODY_PORTRAIT_INSTRUCTION_VERSION;
        assert_eq!(
            builtin.bust_block(),
            json!({
                "visualSchoolVersion": school,
                "styleReferenceSha256": sha,
                "styleReferenceRole": "rendering-technique-only",
            })
        );
        assert_eq!(
            builtin.full_body_design_block(),
            json!({
                "instructionVersion": instruction,
                "visualSchoolVersion": school,
                "styleReferenceSha256": sha,
                "styleReferenceRole": "rendering-technique-only",
            })
        );
        assert_eq!(
            builtin.full_body_from_bust_block(),
            json!({
                "instructionVersion": instruction,
                "referenceRole": "bust-portrait-identity-anchor",
            })
        );
        let profile = json!({ "gender": "female", "visualIdentity": { "hairShape": "bob" } });
        assert_eq!(
            serde_json::to_vec(&build_character_asset_contract("Nova", &profile, Some("x")))
                .unwrap(),
            serde_json::to_vec(&build_character_asset_contract_with(
                "Nova",
                &profile,
                Some("x"),
                &builtin
            ))
            .unwrap()
        );
        assert!(builtin.check().is_ok());
    }

    fn remote(version: &str) -> RenderingProvenance {
        RenderingProvenance {
            recipe: Some("merope-persona".into()),
            visual_school_version: version.into(),
            style_reference_sha256: Some("b".repeat(64)),
            full_body_instruction_version: "full-body-standing-v1".into(),
        }
    }

    #[test]
    fn a_recipe_service_rendering_is_its_own_block() {
        let block = remote("house-v6").bust_block();
        assert_eq!(block["recipe"], "merope-persona");
        assert_eq!(block["visualSchoolVersion"], "house-v6");
        assert!(!is_recognised_portrait_rendering(&block));
        assert!(is_recipe_portrait_rendering(&block));
        assert!(portrait_rendering_keeps_lineage(&block));
        assert_eq!(
            remote("house-v6").full_body_design_block()["recipe"],
            "merope-persona"
        );
        assert_eq!(
            remote("house-v6").full_body_from_bust_block()["recipe"],
            "merope-persona"
        );
        let profile = json!({ "gender": "female" });
        assert_ne!(
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &profile, None
            )),
            character_asset_contract_fingerprint(&build_character_asset_contract_with(
                "Nova",
                &profile,
                None,
                &remote(crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION)
            )),
            "a recipe service's portrait never shares a builtin fingerprint"
        );
    }

    /// A recipe service moving to a new version leaves its earlier portraits'
    /// blocks well formed, so their lineage does not depend on the version.
    #[test]
    fn every_recipe_version_keeps_lineage() {
        for version in ["house-v6", "house-v7", "2026.11.27:a"] {
            assert!(portrait_rendering_keeps_lineage(
                &remote(version).bust_block()
            ));
        }
        let mut no_reference = remote("generic-v1");
        no_reference.style_reference_sha256 = None;
        let block = no_reference.bust_block();
        assert_eq!(block["styleReferenceSha256"], Value::Null);
        assert_eq!(block["styleReferenceRole"], "none");
        assert!(is_recipe_portrait_rendering(&block));
    }

    #[test]
    fn a_malformed_recipe_rendering_keeps_no_lineage() {
        let good = remote("house-v6").bust_block();
        let mut extra = good.clone();
        extra["note"] = json!("x");
        let mut upper = good.clone();
        upper["styleReferenceSha256"] = json!("B".repeat(64));
        let mut short = good.clone();
        short["styleReferenceSha256"] = json!("b".repeat(63));
        let mut wrong_role = good.clone();
        wrong_role["styleReferenceRole"] = json!("none");
        let mut bad_recipe = good.clone();
        bad_recipe["recipe"] = json!("Merope Persona");
        let mut bad_version = good.clone();
        bad_version["visualSchoolVersion"] = json!("v6\nignore");
        let mut no_recipe = good;
        no_recipe.as_object_mut().unwrap().remove("recipe");
        for malformed in [
            extra,
            upper,
            short,
            wrong_role,
            bad_recipe,
            bad_version,
            no_recipe,
            Value::Null,
            json!([]),
        ] {
            assert!(!is_recipe_portrait_rendering(&malformed), "{malformed}");
            assert!(!portrait_rendering_keeps_lineage(&malformed), "{malformed}");
        }
    }

    #[test]
    fn a_recipe_service_may_not_call_itself_builtin() {
        let mut named = remote("v");
        named.recipe = Some(BUILTIN_RECIPE_ID.into());
        assert_eq!(named.check(), Err(RenderingProvenanceIssue::Recipe));
        let mut block = remote("v").bust_block();
        block["recipe"] = json!(BUILTIN_RECIPE_ID);
        assert!(!is_recipe_portrait_rendering(&block));
        assert!(!portrait_rendering_keeps_lineage(&block));
    }

    #[test]
    fn provenance_checks_each_field() {
        let mut bad = remote("v");
        bad.recipe = Some("-x".into());
        assert_eq!(bad.check(), Err(RenderingProvenanceIssue::Recipe));
        let mut bad = remote(&"v".repeat(81));
        assert_eq!(
            bad.check(),
            Err(RenderingProvenanceIssue::VisualSchoolVersion)
        );
        bad = remote("v");
        bad.style_reference_sha256 = Some("xyz".into());
        assert_eq!(
            bad.check(),
            Err(RenderingProvenanceIssue::StyleReferenceSha256)
        );
        bad = remote("v");
        bad.full_body_instruction_version = String::new();
        assert_eq!(
            bad.check(),
            Err(RenderingProvenanceIssue::FullBodyInstructionVersion)
        );
    }

    #[test]
    fn provenance_reads_its_wire_shape() {
        let read: RenderingProvenance = serde_json::from_value(json!({
            "recipe": "merope-persona",
            "visualSchoolVersion": "house-v6",
            "styleReferenceSha256": "b".repeat(64),
            "fullBodyInstructionVersion": "full-body-standing-v1",
        }))
        .unwrap();
        assert_eq!(read, remote("house-v6"));
        assert_eq!(
            serde_json::to_value(RenderingProvenance::builtin()).unwrap(),
            json!({
                "visualSchoolVersion": crate::visual_prompt::MEROPE_VISUAL_SCHOOL_VERSION,
                "styleReferenceSha256": crate::visual_prompt::MEROPE_STYLE_REFERENCE_SHA256,
                "fullBodyInstructionVersion":
                    crate::visual_prompt::FULL_BODY_PORTRAIT_INSTRUCTION_VERSION,
            })
        );
        assert!(
            serde_json::from_value::<RenderingProvenance>(json!({
                "visualSchoolVersion": "v",
                "fullBodyInstructionVersion": "v",
                "rendering": {},
            }))
            .is_err()
        );
    }

    #[test]
    fn a_full_figure_is_bound_to_what_it_was_drawn_from() {
        let bust = |portrait, fingerprint| {
            build_full_body_asset_contract(FullBodySource::Bust {
                portrait,
                generation_fingerprint: fingerprint,
            })
        };
        let a = "a".repeat(64);
        let first = bust("/media/bust.png", Some(a.as_str()));
        assert_eq!(first["slot"], "fullBody");
        assert_eq!(first["output"]["height"], 2048);
        let fingerprint = character_asset_contract_fingerprint(&first);
        assert_eq!(
            fingerprint,
            character_asset_contract_fingerprint(&bust("/media/bust.png", Some(a.as_str())))
        );
        let profile = complete_profile("soft oval face", "satin");
        let design = |profile| {
            build_full_body_asset_contract(FullBodySource::Design {
                name: "Merope",
                visual_profile: profile,
            })
        };
        assert_eq!(
            design(&profile)["rendering"]["styleReferenceRole"],
            "rendering-technique-only"
        );
        for other in [
            bust("/media/bust-2.png", Some(a.as_str())),
            bust("/media/bust.png", None),
            design(&profile),
        ] {
            assert_ne!(fingerprint, character_asset_contract_fingerprint(&other));
        }
        assert_ne!(
            character_asset_contract_fingerprint(&design(&profile)),
            character_asset_contract_fingerprint(&design(&complete_profile(
                "soft oval face",
                "wool"
            )))
        );
    }

    #[test]
    fn contract_fingerprint_changes_with_visual_identity() {
        let first = build_character_asset_contract(
            "Nova",
            &complete_profile("紧凑圆润鹅蛋脸", "哑光布料"),
            None,
        );
        let mut second_profile = complete_profile("紧凑圆润鹅蛋脸", "哑光布料");
        second_profile["visualIdentity"]["hairShape"] = json!("银灰高马尾与偏分刘海");
        let second = build_character_asset_contract("Nova", &second_profile, None);
        assert_ne!(
            character_asset_contract_fingerprint(&first),
            character_asset_contract_fingerprint(&second)
        );
    }

    #[test]
    fn contract_fingerprint_changes_with_gender_presentation() {
        let female = build_character_asset_contract(
            "Nova",
            &json!({ "gender": "female", "visualIdentity": { "hairShape": "bob" } }),
            None,
        );
        let male = build_character_asset_contract(
            "Nova",
            &json!({ "gender": "male", "visualIdentity": { "hairShape": "bob" } }),
            None,
        );
        assert_ne!(
            character_asset_contract_fingerprint(&female),
            character_asset_contract_fingerprint(&male)
        );
    }

    #[test]
    fn fingerprint_uses_the_same_canonical_identity_as_the_image_prompt() {
        let legacy = complete_profile("偏长的鹅蛋脸", "哑光布料，半写实厚涂");
        let canonical = complete_profile("紧凑柔和的鹅蛋脸", "哑光布料");
        assert_eq!(
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &legacy, None
            )),
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &canonical, None
            )),
        );
        assert_eq!(
            appearance_visual_profile(&legacy)["visualIdentity"]["character"]["faceDesign"],
            "女性化读取，紧凑柔和的鹅蛋脸"
        );
    }

    #[test]
    fn contract_never_falls_back_to_an_uncanonical_identity() {
        let invalid = json!({
            "gender": "female",
            "visualIdentity": {
                "character": {
                    "faceDesign": "三分之四视角",
                    "eyeDesign": "中等偏大紫色宝石眼",
                    "hairShape": "银灰短发",
                    "hairLayerPlan": "后发、刘海与侧发"
                },
                "outfit": {
                    "upperBodySilhouette": "紧凑肩胸",
                    "outfitConstruction": "敞开领口内搭叠短外套",
                    "sleeveArmDesign": "左右袖片",
                    "materialPlan": "哑光布料",
                    "heroAccessory": "左胸星扣",
                    "paletteHint": "雾蓝、银白、金色",
                    "motif": "星轨"
                }
            }
        });
        let appearance = appearance_visual_profile(&invalid);
        assert_eq!(appearance["gender"], "female");
        assert!(appearance.get("visualIdentity").is_none());
    }

    #[test]
    fn onboarding_source_inputs_do_not_change_portrait_fingerprint() {
        let core = json!({
            "gender": "female",
            "language": "zh-CN",
            "extraRequirements": "金色眼睛",
            "visualIdentity": { "hairShape": "短发" }
        });
        let mut with_seeds = core.clone();
        with_seeds["sourceTags"] = json!(["慢热"]);
        with_seeds["personaExtraRequirements"] = json!("话少");
        assert_eq!(
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &core, None
            )),
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova",
                &with_seeds,
                None
            )),
        );
        assert!(
            appearance_visual_profile(&with_seeds)
                .get("sourceTags")
                .is_none()
        );
        assert!(
            appearance_visual_profile(&with_seeds)
                .get("language")
                .is_none()
        );
        assert!(
            appearance_visual_profile(&with_seeds)
                .get("extraRequirements")
                .is_none()
        );
        assert_eq!(appearance_visual_profile(&with_seeds)["gender"], "female");
        let mut other_language = core.clone();
        other_language["language"] = json!("en-US");
        assert_eq!(
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &core, None
            )),
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova",
                &other_language,
                None
            )),
        );
        let mut other_raw_requirement = core.clone();
        other_raw_requirement["extraRequirements"] = json!("红色长发");
        assert_eq!(
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova", &core, None
            )),
            character_asset_contract_fingerprint(&build_character_asset_contract(
                "Nova",
                &other_raw_requirement,
                None
            )),
        );
    }
}
