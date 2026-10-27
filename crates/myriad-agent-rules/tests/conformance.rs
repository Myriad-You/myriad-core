//! The shared conformance cases for grants and audience
//! (`crates/conformance/v1/grants/`, `…/audience/`) through the crate, as
//! the site backend calls it. `myriad-core-ffi` runs the same files through
//! the C entry point. The outputs are written by hand from the backend's
//! rules, and from the Mac's for her own memories and approvals past the
//! declaration (the owner's choice, 2026-10-27); nothing here rewrites them.

use std::path::PathBuf;

use myriad_agent_rules::{
    grants::{self, Actor, AutonomyGrant, Layers, Source},
    memory::audience::{self, Audience},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

type Grant = AutonomyGrant<String>;

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1")
}

fn input<T: DeserializeOwned>(value: Value) -> Result<T, ()> {
    serde_json::from_value(value).map_err(|_| ())
}

fn written(result: Result<Grant, grants::WriteRefusal>) -> Value {
    match result {
        Ok(grant) => json!({ "grant": grant }),
        Err(refusal) => json!({ "refused": refusal }),
    }
}

/// One call as the core answers it, without the C ABI; `Err` is bad input.
fn run(call: &str, raw: Value) -> Result<Value, ()> {
    Ok(match call {
        "audience.admits" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                learned: Audience<String>,
                present: Audience<String>,
            }
            let req: In = input(raw)?;
            json!({ "admits": audience::admits(&req.learned, &req.present) })
        }
        "grants.granted" => {
            let layers: Layers = input(raw)?;
            json!({ "granted": grants::granted(&layers) })
        }
        "grants.effective" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                granted: Vec<String>,
                #[serde(default)]
                cap: Option<Vec<String>>,
            }
            let req: In = input(raw)?;
            json!({ "effective": grants::effective(&req.granted, req.cap.as_deref()) })
        }
        "grants.check" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                #[serde(default)]
                grant: Option<Grant>,
                granted: Vec<String>,
                #[serde(default)]
                cap: Option<Vec<String>>,
                required: Vec<String>,
            }
            let req: In = input(raw)?;
            json!(grants::check(
                &req.actor,
                req.grant.as_ref(),
                &req.granted,
                req.cap.as_deref(),
                &req.required
            ))
        }
        "grants.autonomy" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                #[serde(default)]
                grant: Option<Grant>,
                granted: Vec<String>,
            }
            let req: In = input(raw)?;
            json!(grants::evaluate(
                &req.actor,
                req.grant.as_ref(),
                &req.granted
            ))
        }
        "grants.may_enter_work" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                #[serde(default)]
                grant: Option<Grant>,
                granted: Vec<String>,
                source: Source,
            }
            let req: In = input(raw)?;
            let may =
                grants::may_enter_work(&req.actor, req.grant.as_ref(), &req.granted, req.source);
            json!({ "mayEnter": may })
        }
        "grants.cap_still_allows" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                #[serde(default)]
                grant: Option<Grant>,
                granted: Vec<String>,
                #[serde(default)]
                cap: Option<Vec<String>>,
            }
            let req: In = input(raw)?;
            let allows = grants::cap_still_allows(
                &req.actor,
                req.grant.as_ref(),
                &req.granted,
                req.cap.as_deref(),
            );
            json!({ "allows": allows })
        }
        "grants.claim" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                source: Source,
                #[serde(default)]
                grant: Option<Grant>,
                granted: Vec<String>,
            }
            let req: In = input(raw)?;
            json!({ "cap": grants::claim(&req.actor, req.source, req.grant.as_ref(), &req.granted) })
        }
        "grants.prepare" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                actor: Actor<String>,
                requested: Vec<String>,
                granted: Vec<String>,
                candidates: Vec<String>,
            }
            let req: In = input(raw)?;
            written(grants::prepare(
                &req.actor,
                &req.requested,
                &req.granted,
                &req.candidates,
            ))
        }
        "grants.revoke" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                grant: Grant,
                personal: bool,
            }
            let req: In = input(raw)?;
            written(grants::revoke(&req.grant, req.personal))
        }
        other => panic!("no conformance runner for {other}"),
    })
}

#[test]
fn every_grant_and_audience_case_answers_as_recorded() {
    let mut calls = Vec::new();
    let mut cases = 0;
    for area in ["audience", "grants"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(conformance().join(area))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        for path in files {
            let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(file["version"], 1, "{}", path.display());
            assert!(file.get("status").is_none(), "{}", path.display());
            let call = file["call"].as_str().unwrap();
            calls.push(call.to_string());
            for case in file["cases"].as_array().unwrap() {
                let got = run(call, case["input"].clone())
                    .unwrap_or_else(|()| json!({ "error": "bad_input" }));
                assert_eq!(got, case["output"], "{} › {}", path.display(), case["name"]);
                cases += 1;
            }
        }
    }
    calls.sort();
    assert_eq!(
        calls,
        [
            "audience.admits",
            "grants.autonomy",
            "grants.cap_still_allows",
            "grants.check",
            "grants.claim",
            "grants.effective",
            "grants.granted",
            "grants.may_enter_work",
            "grants.prepare",
            "grants.revoke",
        ]
    );
    assert!(cases >= 70, "{cases}");
}
