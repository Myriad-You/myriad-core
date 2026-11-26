use std::collections::HashSet;

use serde_json::{Map, Value};

const PLAYBACK_KIND: &str = "anime-2.5d-rig";
const PLAYBACK_VERSION: u64 = 7;
const PROJECT_NAME: &str = "Anime2.5DRig";
const FIGURE_KIND: &str = "myriad-figure";
const FIGURE_VERSION: u64 = 8;
const MAX_CREDITS: usize = 16;
const MAX_CREDIT_CHARS: usize = 512;

/// What one package version fixes: its identity fields and the names of the
/// pose axes its corrections are keyed by.
struct PackageFormat {
    version: u64,
    kind: &'static str,
    /// The `engine` value the version requires, if it requires one.
    engine: Option<&'static str>,
    /// Pose axes in canonical order: head yaw, head pitch, left lid, right
    /// lid, jaw. The first two are the head axes.
    axes: [&'static str; 5],
    /// Whether the version carries an optional `credits` list.
    credits: bool,
}

impl PackageFormat {
    fn head_axes(&self) -> &[&'static str] {
        &self.axes[..2]
    }
}

const FORMATS: [PackageFormat; 2] = [
    PackageFormat {
        version: PLAYBACK_VERSION,
        kind: PLAYBACK_KIND,
        engine: Some(PROJECT_NAME),
        axes: ["angleX", "angleY", "eyeCloseL", "eyeCloseR", "mouthOpen"],
        credits: false,
    },
    PackageFormat {
        version: FIGURE_VERSION,
        kind: FIGURE_KIND,
        engine: None,
        axes: ["headYaw", "headPitch", "lidCloseL", "lidCloseR", "jaw"],
        credits: true,
    },
];

/// The format named by the package's `version`, if it is a known one.
fn package_format(playback: &Map<String, Value>) -> Option<&'static PackageFormat> {
    let version = playback.get("version").and_then(Value::as_u64)?;
    FORMATS.iter().find(|format| format.version == version)
}

/// The authoring endpoint may change only these residuals, not the rest of a
/// package. The corrections are checked against the axes of the package's own
/// version.
pub fn replace_anime25d_pose_corrections(playback: &mut Value, corrections: &Value) -> bool {
    if !playback_is_valid(playback) {
        return false;
    }
    let Some(format) = playback.as_object().and_then(package_format) else {
        return false;
    };
    if !pose_corrections_are_valid(corrections, format) {
        return false;
    }
    let profile = playback["shellProfile"].as_object_mut().unwrap();
    if corrections.as_array().is_some_and(Vec::is_empty) {
        profile.remove("poseCorrections");
    } else {
        profile.insert("poseCorrections".to_owned(), corrections.clone());
    }
    true
}

const MOUTH_MATERIALS: [&str; 6] = [
    "mouthClose",
    "mouthOpen",
    "mouthWide",
    "mouthRound",
    "mouthNarrow",
    "mouthManiac",
];

pub(crate) fn playback_is_valid(value: &Value) -> bool {
    let Some(playback) = value.as_object() else {
        return false;
    };
    let Some(canvas) = playback.get("pixelCanvas").and_then(Value::as_object) else {
        return false;
    };
    let Some(width) = finite_number(canvas.get("width")) else {
        return false;
    };
    let Some(height) = finite_number(canvas.get("height")) else {
        return false;
    };
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    let Some(format) = package_format(playback) else {
        return false;
    };
    playback.get("kind").and_then(Value::as_str) == Some(format.kind)
        && format
            .engine
            .is_none_or(|engine| playback.get("engine").and_then(Value::as_str) == Some(engine))
        && (!format.credits || playback.get("credits").is_none_or(credits_are_valid))
        && playback
            .get("layers")
            .and_then(Value::as_array)
            .is_some_and(|layers| !layers.is_empty())
        && playback.get("anchors").is_some_and(Value::is_object)
        && playback
            .get("mouthProfile")
            .is_some_and(|profile| mouth_profile_is_valid(profile, width, height))
        && playback
            .get("chestProfile")
            .is_some_and(|profile| chest_profile_is_valid(profile, width, height))
        && playback
            .get("shellProfile")
            .is_some_and(|profile| shell_profile_is_valid(profile, width, height, format))
}

fn credits_are_valid(value: &Value) -> bool {
    let Some(credits) = value.as_array() else {
        return false;
    };
    credits.len() <= MAX_CREDITS
        && credits.iter().all(|credit| {
            let Some(credit) = credit.as_object() else {
                return false;
            };
            credit
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty() && credit_text_fits(name))
                && ["url", "license", "copyright"].iter().all(|field| {
                    credit
                        .get(*field)
                        .is_none_or(|value| value.as_str().is_some_and(credit_text_fits))
                })
        })
}

fn credit_text_fits(text: &str) -> bool {
    text.chars().count() <= MAX_CREDIT_CHARS
}

fn mouth_profile_is_valid(value: &Value, width: f64, height: f64) -> bool {
    let Some(profile) = value.as_object() else {
        return false;
    };
    if profile.get("version").and_then(Value::as_u64) != Some(1)
        || !matches!(
            profile.get("source").and_then(Value::as_str),
            Some("alpha-contour" | "bounds-fallback")
        )
    {
        return false;
    }
    let Some(silhouettes) = profile.get("silhouettes").and_then(Value::as_array) else {
        return false;
    };
    let Some(bridges) = profile.get("bridges").and_then(Value::as_array) else {
        return false;
    };
    if silhouettes.len() != MOUTH_MATERIALS.len() || bridges.len() != 15 {
        return false;
    }

    let mut materials = HashSet::new();
    for silhouette in silhouettes {
        let Some(silhouette) = silhouette.as_object() else {
            return false;
        };
        let Some(material) = silhouette.get("material").and_then(Value::as_str) else {
            return false;
        };
        if !MOUTH_MATERIALS.contains(&material)
            || !materials.insert(material)
            || !number_in_range(silhouette.get("centerX"), -width, width * 2.0)
            || !number_in_range(silhouette.get("centerY"), -height, height * 2.0)
            || !number_in_range(silhouette.get("width"), 0.25, width)
            || !number_in_range(silhouette.get("height"), 0.25, height)
            || !number_in_range(silhouette.get("leftCornerY"), -height, height * 2.0)
            || !number_in_range(silhouette.get("rightCornerY"), -height, height * 2.0)
            || !number_in_range(silhouette.get("fillRatio"), 0.0, 1.0)
            || !number_in_range(silhouette.get("aperture"), 0.0, 4.0)
        {
            return false;
        }
    }

    let mut pairs = HashSet::new();
    for bridge in bridges {
        let Some(bridge) = bridge.as_object() else {
            return false;
        };
        let Some(first) = bridge.get("first").and_then(Value::as_str) else {
            return false;
        };
        let Some(second) = bridge.get("second").and_then(Value::as_str) else {
            return false;
        };
        let Some(first_index) = MOUTH_MATERIALS
            .iter()
            .position(|material| *material == first)
        else {
            return false;
        };
        let Some(second_index) = MOUTH_MATERIALS
            .iter()
            .position(|material| *material == second)
        else {
            return false;
        };
        let pair = if first_index < second_index {
            (first_index, second_index)
        } else {
            (second_index, first_index)
        };
        if first_index == second_index
            || !pairs.insert(pair)
            || !number_in_range(bridge.get("widthScale"), 0.75, 1.0)
            || !number_in_range(bridge.get("heightScale"), 0.65, 1.0)
            || !number_in_range(bridge.get("neutralization"), 0.0, 1.0)
            || !number_in_range(bridge.get("centerOffsetX"), -width / 4.0, width / 4.0)
            || !number_in_range(bridge.get("centerOffsetY"), -height / 4.0, height / 4.0)
        {
            return false;
        }
    }
    materials.len() == MOUTH_MATERIALS.len() && pairs.len() == 15
}

fn chest_profile_is_valid(value: &Value, width: f64, height: f64) -> bool {
    let Some(profile) = value.as_object() else {
        return false;
    };
    profile.get("version").and_then(Value::as_u64) == Some(2)
        && profile.get("enabled").is_some_and(Value::is_boolean)
        && matches!(
            profile.get("source").and_then(Value::as_str),
            Some("ai-vision" | "geometry-fallback" | "gender-policy")
        )
        && number_in_range(profile.get("centerX"), 0.0, width)
        && number_in_range(profile.get("centerY"), 0.0, height)
        && number_in_range(profile.get("radiusX"), 1.0, width / 2.0)
        && number_in_range(profile.get("radiusY"), 1.0, height / 2.0)
        && number_in_range(profile.get("visibleScale"), 0.0, 1.0)
        && number_in_range(profile.get("motionScale"), 0.0, 1.25)
        && number_in_range(profile.get("frequencyScale"), 0.75, 1.25)
        && number_in_range(profile.get("supportScale"), 0.0, 1.0)
        && number_in_range(profile.get("garmentMotionScale"), 0.0, 1.0)
        && number_in_range(profile.get("confidence"), 0.0, 1.0)
}

fn shell_profile_is_valid(value: &Value, width: f64, height: f64, format: &PackageFormat) -> bool {
    let Some(profile) = value.as_object() else {
        return false;
    };
    let Some(face_profile) = profile.get("faceProfile").and_then(Value::as_object) else {
        return false;
    };
    let Some(hair) = profile.get("hair").and_then(Value::as_object) else {
        return false;
    };
    let Some(pin) = hair.get("hairlinePin").and_then(Value::as_object) else {
        return false;
    };
    let Some(torso) = profile.get("torso").and_then(Value::as_object) else {
        return false;
    };
    let Some(points) = face_profile.get("points").and_then(Value::as_array) else {
        return false;
    };
    let Some(start_y) = finite_number(face_profile.get("startY")) else {
        return false;
    };
    let Some(end_y) = finite_number(face_profile.get("endY")) else {
        return false;
    };
    if profile.get("version").and_then(Value::as_u64) != Some(1)
        || !matches!(
            profile.get("source").and_then(Value::as_str),
            Some("anchor-derived" | "authored")
        )
        || !profile.get("enabled").is_some_and(Value::is_boolean)
        || !number_in_range(profile.get("blend"), 0.0, 1.0)
        || !shell_ellipsoid_is_valid(profile.get("head"), width, height)
        || !face_profile.get("enabled").is_some_and(Value::is_boolean)
        || !(0.0..=height).contains(&start_y)
        || !(0.0..=height * 1.2).contains(&end_y)
        || end_y <= start_y
        || points.len() != 5
        || !shell_ellipsoid_is_valid(profile.get("hair"), width, height)
        || !number_in_range(hair.get("frontGap"), 0.0, 0.6)
        || !number_in_range(hair.get("frontBulge"), 0.0, 1.5)
        || !number_in_range(hair.get("backDepth"), 0.0, 1.0)
        || !number_in_range(hair.get("crownRound"), 0.0, 1.0)
        || !pin.get("enabled").is_some_and(Value::is_boolean)
        || !matches!(
            pin.get("mode").and_then(Value::as_str),
            Some("rectangle" | "strand-roots")
        )
        || !number_in_range(pin.get("centerX"), -1.5, 1.5)
        || !number_in_range(pin.get("centerY"), -1.5, 1.5)
        || !number_in_range(pin.get("halfWidth"), 0.01, 2.0)
        || !number_in_range(pin.get("halfHeight"), 0.01, 2.0)
        || !number_in_range(pin.get("feather"), 0.0, 0.5)
        || !torso.get("enabled").is_some_and(Value::is_boolean)
        || !number_in_range(torso.get("blend"), 0.0, 1.0)
        || !number_in_range(torso.get("centerX"), 0.0, width)
        || !number_in_range(torso.get("radiusX"), 1.0, width)
        || !number_in_range(torso.get("radiusZ"), 1.0, width)
        // Optional: manifests compiled before the torso follow became
        // per-model carry no value, and the runtime reads those as a full
        // follow. Present but out of range is still a rejection.
        || torso
            .get("yawFollowScale")
            .is_some_and(|value| !number_in_range(Some(value), 0.0, 1.0))
        || profile
            .get("poseCorrections")
            .is_some_and(|value| !pose_corrections_are_valid(value, format))
    {
        return false;
    }

    let mut previous_v = -1.0;
    for point in points {
        let Some(point) = point.as_object() else {
            return false;
        };
        let Some(v) = finite_number(point.get("v")) else {
            return false;
        };
        if !(0.0..=1.0).contains(&v)
            || !number_in_range(point.get("z"), -0.4, 0.8)
            || v <= previous_v
        {
            return false;
        }
        previous_v = v;
    }
    true
}

fn pose_corrections_are_valid(value: &Value, format: &PackageFormat) -> bool {
    let head_axes = format.head_axes();
    let Some(corrections) = value.as_array() else {
        return false;
    };
    if corrections.len() > 16 {
        return false;
    }
    let mut corners = HashSet::new();
    for correction in corrections {
        let Some(surface @ ("head" | "front-hair" | "back-hair")) =
            correction.get("surface").and_then(Value::as_str)
        else {
            return false;
        };
        let Some(at) = correction.get("at").and_then(Value::as_object) else {
            return false;
        };
        let Some(patches) = correction.get("patches").and_then(Value::as_array) else {
            return false;
        };
        if !(2..=3).contains(&at.len())
            || !(1..=8).contains(&patches.len())
            || !head_axes.iter().any(|axis| at.contains_key(*axis))
        {
            return false;
        }
        for (axis, value) in at {
            let Some(value) = finite_number(Some(value)) else {
                return false;
            };
            if !format.axes.contains(&axis.as_str())
                || !(-1.0..=1.0).contains(&value)
                || if head_axes.contains(&axis.as_str()) {
                    value.abs() < 0.05
                } else {
                    value < 0.05
                }
            {
                return false;
            }
        }
        let signature = format
            .axes
            .iter()
            .filter_map(|axis| {
                finite_number(at.get(*axis)).map(|v| format!("{axis}:{}", v.signum()))
            })
            .collect::<Vec<_>>()
            .join(",");
        if !corners.insert((surface, signature)) {
            return false;
        }
        for patch in patches {
            if !number_in_range(patch.get("x"), -2.0, 2.0)
                || !number_in_range(patch.get("y"), -2.0, 2.0)
                || !number_in_range(patch.get("radiusX"), 0.1, 2.0)
                || !number_in_range(patch.get("radiusY"), 0.1, 2.0)
                || !number_in_range(patch.get("dx"), -0.25, 0.25)
                || !number_in_range(patch.get("dy"), -0.25, 0.25)
            {
                return false;
            }
        }
    }
    true
}

fn shell_ellipsoid_is_valid(value: Option<&Value>, width: f64, height: f64) -> bool {
    let Some(value) = value.and_then(Value::as_object) else {
        return false;
    };
    number_in_range(value.get("centerX"), 0.0, width)
        && number_in_range(value.get("centerY"), 0.0, height)
        && number_in_range(value.get("radiusX"), 1.0, width)
        && number_in_range(value.get("radiusY"), 1.0, height)
        && number_in_range(value.get("radiusZ"), 1.0, width)
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn number_in_range(value: Option<&Value>, minimum: f64, maximum: f64) -> bool {
    finite_number(value).is_some_and(|value| (minimum..=maximum).contains(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_playback() -> Value {
        let silhouettes = MOUTH_MATERIALS
            .iter()
            .map(|material| {
                serde_json::json!({
                    "material": material,
                    "centerX": 50,
                    "centerY": 54,
                    "width": 20,
                    "height": 12,
                    "leftCornerY": 54,
                    "rightCornerY": 54,
                    "fillRatio": 0.5,
                    "aperture": 0.5
                })
            })
            .collect::<Vec<_>>();
        let mut bridges = Vec::new();
        for (first_index, first) in MOUTH_MATERIALS.iter().enumerate() {
            for second in MOUTH_MATERIALS.iter().skip(first_index + 1) {
                bridges.push(serde_json::json!({
                    "first": first,
                    "second": second,
                    "widthScale": 1,
                    "heightScale": 1,
                    "neutralization": 0,
                    "centerOffsetX": 0,
                    "centerOffsetY": 0
                }));
            }
        }
        serde_json::json!({
            "kind": PLAYBACK_KIND,
            "version": PLAYBACK_VERSION,
            "engine": PROJECT_NAME,
            "pixelCanvas": { "width": 100, "height": 120 },
            "layers": [{ "role": "face" }],
            "anchors": {},
            "mouthProfile": {
                "version": 1,
                "source": "alpha-contour",
                "silhouettes": silhouettes,
                "bridges": bridges
            },
            "chestProfile": {
                "version": 2,
                "enabled": true,
                "source": "geometry-fallback",
                "centerX": 50,
                "centerY": 75,
                "radiusX": 20,
                "radiusY": 18,
                "visibleScale": 0.5,
                "motionScale": 1,
                "frequencyScale": 1,
                "supportScale": 0.45,
                "garmentMotionScale": 0.65,
                "confidence": 0
            },
            "shellProfile": {
                "version": 1,
                "source": "anchor-derived",
                "enabled": true,
                "blend": 0.5,
                "head": {
                    "centerX": 50,
                    "centerY": 35,
                    "radiusX": 25,
                    "radiusY": 30,
                    "radiusZ": 18
                },
                "faceProfile": {
                    "enabled": true,
                    "startY": 10,
                    "endY": 75,
                    "points": [
                        { "v": 0.06, "z": 0.1 },
                        { "v": 0.42, "z": 0.02 },
                        { "v": 0.62, "z": 0.3 },
                        { "v": 0.78, "z": 0.06 },
                        { "v": 0.97, "z": 0.14 }
                    ]
                },
                "hair": {
                    "centerX": 50,
                    "centerY": 33,
                    "radiusX": 28,
                    "radiusY": 33,
                    "radiusZ": 19,
                    "frontGap": 0.18,
                    "frontBulge": 1,
                    "backDepth": 0.35,
                    "crownRound": 0,
                    "hairlinePin": {
                        "enabled": true,
                        "mode": "strand-roots",
                        "centerX": 0,
                        "centerY": -0.45,
                        "halfWidth": 1.1,
                        "halfHeight": 0.32,
                        "feather": 0.06
                    }
                },
                "torso": {
                    "enabled": true,
                    "blend": 0.5,
                    "centerX": 50,
                    "radiusX": 40,
                    "radiusZ": 25
                }
            }
        })
    }

    #[test]
    fn accepts_complete_v7_playback() {
        assert!(playback_is_valid(&sample_playback()));
    }

    #[test]
    fn authoring_changes_only_residuals_and_rejection_is_atomic() {
        let original = sample_playback();
        let mut playback = original.clone();
        assert!(!replace_anime25d_pose_corrections(
            &mut playback,
            &Value::Null
        ));
        assert_eq!(playback, original);
        let corrections = serde_json::json!([{
            "surface": "head", "at": { "angleX": 0.8, "angleY": -0.6 },
            "patches": [{ "x": 0, "y": 0, "radiusX": 0.6, "radiusY": 0.4, "dx": 0.05, "dy": -0.04 }]
        }]);
        assert!(replace_anime25d_pose_corrections(
            &mut playback,
            &corrections
        ));
        assert_eq!(playback["shellProfile"]["poseCorrections"], corrections);
        assert!(replace_anime25d_pose_corrections(
            &mut playback,
            &serde_json::json!([])
        ));
        assert_eq!(
            playback, original,
            "removing corrections restores exact package content"
        );
    }

    #[test]
    fn validates_combination_pose_corrections_without_dropping_them() {
        let correction = serde_json::json!({
            "surface": "head", "at": { "angleX": 0.8, "angleY": -0.6 },
            "patches": [{ "x": 0, "y": 0, "radiusX": 0.6, "radiusY": 0.4, "dx": 0.05, "dy": -0.04 }]
        });
        let mut playback = sample_playback();
        playback["shellProfile"]["poseCorrections"] = serde_json::json!([correction]);
        assert!(playback_is_valid(&playback));
        let serialized = serde_json::to_vec(&playback).unwrap();
        let restored: Value = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored["shellProfile"]["poseCorrections"][0], correction);
        for bad in [
            Value::Null,
            serde_json::json!({}),
            serde_json::json!([null]),
            serde_json::json!([correction.clone(), correction.clone()]),
        ] {
            playback["shellProfile"]["poseCorrections"] = bad;
            assert!(!playback_is_valid(&playback));
        }
        for bad_at in [
            serde_json::json!({ "angleX": 1 }),
            serde_json::json!({ "angleX": 0, "angleY": 1 }),
            serde_json::json!({ "angleX": 1, "eyeCloseL": -1 }),
            serde_json::json!({ "angleX": 1, "angleY": 2 }),
            serde_json::json!({ "angleX": 1, "body": 1 }),
            serde_json::json!({ "eyeCloseL": 1, "mouthOpen": 1 }),
        ] {
            let mut bad = correction.clone();
            bad["at"] = bad_at;
            playback["shellProfile"]["poseCorrections"] = serde_json::json!([bad]);
            assert!(!playback_is_valid(&playback));
        }
        for (field, value) in [("radiusX", 0.0), ("dx", 0.3), ("y", 3.0)] {
            let mut bad = correction.clone();
            bad["patches"][0][field] = serde_json::json!(value);
            playback["shellProfile"]["poseCorrections"] = serde_json::json!([bad]);
            assert!(!playback_is_valid(&playback));
        }
        playback["shellProfile"]["poseCorrections"] = serde_json::json!([]);
        assert!(playback_is_valid(&playback));
    }

    #[test]
    fn rejects_obsolete_or_incomplete_playback() {
        let mut playback = sample_playback();
        playback["version"] = serde_json::json!(6);
        assert!(!playback_is_valid(&playback));

        let mut playback = sample_playback();
        playback.as_object_mut().unwrap().remove("mouthProfile");
        assert!(!playback_is_valid(&playback));

        let mut playback = sample_playback();
        playback["shellProfile"]
            .as_object_mut()
            .unwrap()
            .remove("torso");
        assert!(!playback_is_valid(&playback));

        let mut playback = sample_playback();
        playback["chestProfile"]
            .as_object_mut()
            .unwrap()
            .remove("garmentMotionScale");
        assert!(!playback_is_valid(&playback));
    }

    #[test]
    fn accepts_an_absent_or_bounded_per_model_torso_follow() {
        // Compiled before the follow became per-model: still live.
        let playback = sample_playback();
        assert!(
            playback["shellProfile"]["torso"]
                .get("yawFollowScale")
                .is_none()
        );
        assert!(playback_is_valid(&playback));

        for scale in [0.0, 0.4, 1.0] {
            let mut playback = sample_playback();
            playback["shellProfile"]["torso"]["yawFollowScale"] = serde_json::json!(scale);
            assert!(playback_is_valid(&playback), "{scale}");
        }

        for scale in [-0.1, 1.4] {
            let mut playback = sample_playback();
            playback["shellProfile"]["torso"]["yawFollowScale"] = serde_json::json!(scale);
            assert!(!playback_is_valid(&playback), "{scale}");
        }
    }

    fn sample_figure() -> Value {
        let mut figure = sample_playback();
        let package = figure.as_object_mut().unwrap();
        package.insert("kind".to_owned(), serde_json::json!(FIGURE_KIND));
        package.insert("version".to_owned(), serde_json::json!(FIGURE_VERSION));
        package.remove("engine");
        package.insert(
            "credits".to_owned(),
            serde_json::json!([
                { "name": "Rig author", "url": "https://example.com", "license": "MIT", "copyright": "2026" },
                { "name": "Painter" }
            ]),
        );
        figure
    }

    fn v7_correction() -> Value {
        serde_json::json!({
            "surface": "head", "at": { "angleX": 0.8, "angleY": -0.6 },
            "patches": [{ "x": 0, "y": 0, "radiusX": 0.6, "radiusY": 0.4, "dx": 0.05, "dy": -0.04 }]
        })
    }

    fn v8_correction() -> Value {
        serde_json::json!({
            "surface": "head", "at": { "headYaw": 0.8, "headPitch": -0.6 },
            "patches": [{ "x": 0, "y": 0, "radiusX": 0.6, "radiusY": 0.4, "dx": 0.05, "dy": -0.04 }]
        })
    }

    #[test]
    fn accepts_complete_v8_figure_with_or_without_engine_and_credits() {
        let figure = sample_figure();
        assert!(playback_is_valid(&figure));

        let mut bare = figure.clone();
        bare.as_object_mut().unwrap().remove("credits");
        assert!(playback_is_valid(&bare));

        let mut with_engine = figure.clone();
        with_engine["engine"] = serde_json::json!("anything");
        with_engine["engineUrl"] = serde_json::json!("https://example.com");
        assert!(playback_is_valid(&with_engine));

        let mut with_corrections = figure;
        with_corrections["shellProfile"]["poseCorrections"] = serde_json::json!([
            v8_correction(),
            {
                "surface": "front-hair", "at": { "headPitch": 0.5, "lidCloseL": 0.4, "jaw": 1 },
                "patches": [{ "x": 0, "y": 0, "radiusX": 0.6, "radiusY": 0.4, "dx": 0, "dy": 0 }]
            }
        ]);
        assert!(playback_is_valid(&with_corrections));
    }

    #[test]
    fn kind_and_axes_follow_the_version() {
        let mut figure = sample_figure();
        figure["shellProfile"]["poseCorrections"] = serde_json::json!([v7_correction()]);
        assert!(!playback_is_valid(&figure), "v8 with v7 axes");

        let mut playback = sample_playback();
        playback["shellProfile"]["poseCorrections"] = serde_json::json!([v8_correction()]);
        assert!(!playback_is_valid(&playback), "v7 with v8 axes");

        let mut figure = sample_figure();
        figure["kind"] = serde_json::json!(PLAYBACK_KIND);
        assert!(!playback_is_valid(&figure), "v8 with the v7 kind");

        let mut playback = sample_playback();
        playback["kind"] = serde_json::json!(FIGURE_KIND);
        assert!(!playback_is_valid(&playback), "v7 with the v8 kind");

        let mut playback = sample_playback();
        playback.as_object_mut().unwrap().remove("engine");
        assert!(
            !playback_is_valid(&playback),
            "v7 still requires its engine"
        );

        for version in [6, 9] {
            for mut package in [sample_playback(), sample_figure()] {
                package["version"] = serde_json::json!(version);
                assert!(!playback_is_valid(&package), "{version}");
            }
        }
    }

    #[test]
    fn v8_corrections_follow_the_same_rules_on_the_new_axes() {
        let mut figure = sample_figure();
        for bad_at in [
            serde_json::json!({ "headYaw": 1 }),
            serde_json::json!({ "headYaw": 0, "headPitch": 1 }),
            serde_json::json!({ "headYaw": 1, "lidCloseL": -1 }),
            serde_json::json!({ "headYaw": 1, "headPitch": 2 }),
            serde_json::json!({ "headYaw": 1, "angleY": 1 }),
            serde_json::json!({ "lidCloseL": 1, "jaw": 1 }),
            serde_json::json!({ "headYaw": 1, "lidCloseL": 1, "lidCloseR": 1, "jaw": 1 }),
        ] {
            let mut bad = v8_correction();
            bad["at"] = bad_at.clone();
            figure["shellProfile"]["poseCorrections"] = serde_json::json!([bad]);
            assert!(!playback_is_valid(&figure), "{bad_at}");
        }
        figure["shellProfile"]["poseCorrections"] =
            serde_json::json!([v8_correction(), v8_correction()]);
        assert!(!playback_is_valid(&figure), "duplicate corner");
    }

    #[test]
    fn rejects_badly_shaped_credits() {
        let long = "x".repeat(MAX_CREDIT_CHARS + 1);
        let many = vec![serde_json::json!({ "name": "a" }); MAX_CREDITS + 1];
        for bad in [
            serde_json::json!({}),
            serde_json::json!("Rig author"),
            serde_json::json!([null]),
            serde_json::json!(["Rig author"]),
            serde_json::json!([{}]),
            serde_json::json!([{ "name": "  " }]),
            serde_json::json!([{ "name": 1 }]),
            serde_json::json!([{ "name": long }]),
            serde_json::json!([{ "name": "a", "url": 1 }]),
            serde_json::json!([{ "name": "a", "license": null }]),
            serde_json::json!([{ "name": "a", "copyright": long }]),
            Value::Array(many),
        ] {
            let mut figure = sample_figure();
            figure["credits"] = bad.clone();
            assert!(!playback_is_valid(&figure), "{bad}");
        }
        let mut figure = sample_figure();
        figure["credits"] = Value::Array(vec![
            serde_json::json!({ "name": "x".repeat(MAX_CREDIT_CHARS) });
            MAX_CREDITS
        ]);
        assert!(playback_is_valid(&figure));
    }

    #[test]
    fn replacing_corrections_uses_the_package_own_axes_and_keeps_its_identity() {
        for (original, own, other) in [
            (sample_figure(), v8_correction(), v7_correction()),
            (sample_playback(), v7_correction(), v8_correction()),
        ] {
            let mut package = original.clone();
            assert!(!replace_anime25d_pose_corrections(
                &mut package,
                &serde_json::json!([other])
            ));
            assert_eq!(package, original, "rejection is atomic");

            let own = serde_json::json!([own]);
            assert!(replace_anime25d_pose_corrections(&mut package, &own));
            assert_eq!(package["shellProfile"]["poseCorrections"], own);
            assert_eq!(package["version"], original["version"]);
            assert_eq!(package["kind"], original["kind"]);
            assert!(playback_is_valid(&package));

            let before = package.clone();
            assert!(!replace_anime25d_pose_corrections(
                &mut package,
                &serde_json::json!([other])
            ));
            assert_eq!(package, before, "rejection is atomic");
        }
    }
}
