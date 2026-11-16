//! What the link reads of the contract the core was built with
//! (`myriad_her_link::CONTRACT_JSON`, `shared/her_link_contract.json`): the
//! protocol, the timing, the capabilities and the close codes. Read once.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// The contract's `timing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub heartbeat_ms: i64,
    pub missed_heartbeats: i64,
    pub hello_timeout_ms: i64,
    pub reconnect_initial_ms: i64,
    pub reconnect_max_ms: i64,
    pub reconnect_factor: i64,
}

impl Timing {
    /// Nothing heard for this long: the link is gone.
    pub fn silence_ms(&self) -> i64 {
        self.heartbeat_ms * self.missed_heartbeats.max(1)
    }

    /// The wait before the `attempt`th retry (0 is the first), doubling up to the cap.
    pub fn backoff_ms(&self, attempt: u32) -> i64 {
        let mut delay = self.reconnect_initial_ms;
        for _ in 0..attempt.min(32) {
            delay = (delay * self.reconnect_factor.max(1)).min(self.reconnect_max_ms);
            if delay >= self.reconnect_max_ms {
                break;
            }
        }
        delay
    }
}

/// The contract's `limits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_frame_bytes: u32,
    pub max_text_chars: u32,
}

/// The contract's `encoding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoding {
    pub frame_version: u32,
}

/// One of the contract's `closeCodes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseRule {
    pub code: u16,
    pub forget_credentials: bool,
    pub reconnect: bool,
}

/// What the link reads of one of the contract's `frames`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRule {
    /// `child`, `host`, `device`, `mind`, `server` or `either`.
    pub from: String,
    pub delivery: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// The contract's own version (`schemaVersion`), which `link.versions` says.
    pub schema_version: u32,
    pub protocol: u32,
    pub path: String,
    pub timing: Timing,
    pub limits: Limits,
    pub encoding: Encoding,
    pub capabilities: Vec<String>,
    pub surfaces: Vec<String>,
    /// By name (`revoked`, `host_moved`…).
    pub close_codes: BTreeMap<String, CloseRule>,
    /// Every frame type the contract names, and who sends it.
    pub frames: BTreeMap<String, FrameRule>,
}

impl Contract {
    /// The rule for a close frame's `code`, or else for the name the host gave as its reason.
    pub fn close(&self, code: Option<u16>, reason: Option<&str>) -> Option<(&str, &CloseRule)> {
        code.and_then(|code| self.close_codes.iter().find(|(_, rule)| rule.code == code))
            .or_else(|| reason.and_then(|name| self.close_codes.get_key_value(name)))
            .map(|(name, rule)| (name.as_str(), rule))
    }
}

static CONTRACT: LazyLock<Contract> = LazyLock::new(|| {
    serde_json::from_str(myriad_her_link::CONTRACT_JSON)
        .expect("the contract the core was built with reads")
});

/// The contract this end was built with: the same bytes the core checks frames against.
pub fn contract() -> &'static Contract {
    &CONTRACT
}
