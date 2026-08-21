use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::rig_contract::{
    CHARACTER_ASSET_CONTRACT_VERSION, CHARACTER_ASSET_REQUIRED_CAPABILITIES,
    MAX_RIGID_ARM_ROTATION_DEGREES, PORTRAIT_ASPECT_HEIGHT, PORTRAIT_ASPECT_WIDTH,
    PORTRAIT_CANVAS_HEIGHT, PORTRAIT_CANVAS_WIDTH, PORTRAIT_GENERATION_HEIGHT,
    PORTRAIT_GENERATION_WIDTH,
};

/// Immutable input snapshot for the generated master portrait.
///
/// The portrait URL identifies pixels; this contract identifies what those
/// pixels were supposed to depict and which downstream rig contract they use.
/// `slot` is always `"master"` so existing generation fingerprints remain valid.
pub fn build_character_asset_contract(
    name: &str,
    visual_profile: &Value,
    additional_requirements: Option<&str>,
) -> Value {
    json!({
        "contractVersion": CHARACTER_ASSET_CONTRACT_VERSION,
        "slot": "master",
        "identity": {
            "name": bounded_text(name, 50),
            "visualProfile": visual_profile,
        },
        "output": {
            "width": PORTRAIT_GENERATION_WIDTH,
            "height": PORTRAIT_GENERATION_HEIGHT,
            "portraitAspect": {
                "width": PORTRAIT_ASPECT_WIDTH,
                "height": PORTRAIT_ASPECT_HEIGHT,
            },
            "rigCanvas": {
                "width": PORTRAIT_CANVAS_WIDTH,
                "height": PORTRAIT_CANVAS_HEIGHT,
            },
            "framing": "close-full-head-through-lower-chest-or-high-waist",
            "background": "clean-near-white",
        },
        "rig": {
            "maxRigidArmRotationDegrees": MAX_RIGID_ARM_ROTATION_DEGREES,
            "requiredCapabilities": CHARACTER_ASSET_REQUIRED_CAPABILITIES,
        },
        "additionalRequirements": additional_requirements
            .map(|value| bounded_text(value, 2_000))
            .filter(|value| !value.is_empty()),
    })
}

pub fn character_asset_contract_fingerprint(contract: &Value) -> String {
    let encoded = serde_json::to_vec(contract).expect("character asset contract is serializable");
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    hasher.finalize().iter().fold(
        String::with_capacity(64),
        |mut fingerprint, byte| {
            write!(&mut fingerprint, "{byte:02x}").expect("write fingerprint");
            fingerprint
        },
    )
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

    #[test]
    fn master_contract_carries_identity_output_and_rig_invariants() {
        let contract = build_character_asset_contract(
            " Nova ",
            &json!({ "gender": "nonbinary" }),
            Some(" gold eyes "),
        );
        assert_eq!(contract["slot"], "master");
        assert_eq!(contract["identity"]["name"], "Nova");
        assert_eq!(contract["output"]["width"], 1152);
        assert_eq!(contract["output"]["height"], 1536);
        assert_eq!(contract["output"]["portraitAspect"], json!({ "width": 3, "height": 4 }));
        assert_eq!(contract["output"]["rigCanvas"]["width"], 1.0);
        assert!(
            (contract["output"]["rigCanvas"]["height"]
                .as_f64()
                .unwrap()
                - (4.0 / 3.0))
                .abs()
                < 0.000_001
        );
        assert_eq!(contract["output"]["background"], "clean-near-white");
        assert_eq!(
            contract["output"]["framing"],
            "close-full-head-through-lower-chest-or-high-waist"
        );
        assert_eq!(contract["rig"]["maxRigidArmRotationDegrees"], 15.0);
        assert_eq!(character_asset_contract_fingerprint(&contract).len(), 64);
    }

    #[test]
    fn contract_fingerprint_changes_with_visual_identity() {
        let first = build_character_asset_contract(
            "Nova",
            &json!({ "visualIdentity": { "hairShape": "bob" } }),
            None,
        );
        let second = build_character_asset_contract(
            "Nova",
            &json!({ "visualIdentity": { "hairShape": "ponytail" } }),
            None,
        );
        assert_ne!(
            character_asset_contract_fingerprint(&first),
            character_asset_contract_fingerprint(&second)
        );
    }
}
