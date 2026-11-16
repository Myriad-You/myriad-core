//! What a session gives back, as JSON: the one form the conformance scripts
//! (`crates/conformance/v1/child`) compare and an end across the C entry is
//! handed. Field names are camelCase, kinds and reasons snake_case; a field
//! with nothing to say is `null`, never left out, so two ends compare equal.

use serde_json::{Value, json};

use crate::session::{After, Delivery, Down, Output, Phase, Removal, WardrobeItem};

impl Phase {
    /// `closed`, `greeting` or `open`.
    pub fn name(self) -> &'static str {
        match self {
            Phase::Closed => "closed",
            Phase::Greeting => "greeting",
            Phase::Open => "open",
        }
    }
}

impl Removal {
    /// `revoked`, `owner_changed` or `token_unknown`.
    pub fn name(self) -> &'static str {
        match self {
            Removal::Revoked => "revoked",
            Removal::OwnerChanged => "owner_changed",
            Removal::TokenUnknown => "token_unknown",
        }
    }
}

impl Down {
    /// `{why}`, with `device` (the device she lives on, or null) for `host_moved` and
    /// `removal` for `removed`.
    pub fn json(&self) -> Value {
        let why = match self {
            Down::Connecting => "connecting",
            Down::Network => "network",
            Down::Silent => "silent",
            Down::HostUnavailable => "host_unavailable",
            Down::NoOwner => "no_owner",
            Down::Replaced => "replaced",
            Down::UnsupportedProtocol => "unsupported_protocol",
            Down::RefusedHandshake => "refused_handshake",
            Down::HostMoved(device) => return json!({ "why": "host_moved", "device": device }),
            Down::HostFrozen => "host_frozen",
            Down::Redirected => "redirected",
            Down::UnsupportedSite => "unsupported_site",
            Down::CredentialMissing => "credential_missing",
            Down::Stopped => "stopped",
            Down::Removed(removal) => {
                return json!({ "why": "removed", "removal": removal.name() });
            }
        };
        json!({ "why": why })
    }
}

impl After {
    /// `{then: "reconnect" | "stop", down}` or `{then: "forget", removal}`.
    pub fn json(&self) -> Value {
        match self {
            After::Reconnect(down) => json!({ "then": "reconnect", "down": down.json() }),
            After::Stop(down) => json!({ "then": "stop", "down": down.json() }),
            After::Forget(removal) => json!({ "then": "forget", "removal": removal.name() }),
        }
    }
}

impl WardrobeItem {
    pub fn json(&self) -> Value {
        json!({
            "outfitId": self.outfit_id,
            "name": self.name,
            "fullBody": self.full_body,
            "wearable": self.wearable,
            "rigAssetId": self.rig_asset_id,
            "portraitAssetId": self.portrait_asset_id,
        })
    }
}

impl Delivery {
    /// `{kind, …}`: the kind in snake_case and the delivery's own fields.
    pub fn json(&self) -> Value {
        match self {
            Delivery::Welcome {
                device_id,
                host,
                lineage,
                host_epoch,
            } => json!({
                "kind": "welcome",
                "deviceId": device_id,
                "host": host,
                "lineage": lineage,
                "hostEpoch": host_epoch,
            }),
            Delivery::Accepted {
                turn_id,
                run_id,
                session_id,
                duplicate,
            } => json!({
                "kind": "accepted",
                "turnId": turn_id,
                "runId": run_id,
                "sessionId": session_id,
                "duplicate": duplicate,
            }),
            Delivery::Rejected { turn_id, code } => {
                json!({ "kind": "rejected", "turnId": turn_id, "code": code })
            }
            Delivery::Event {
                turn_id,
                run_id,
                sequence,
                event,
            } => json!({
                "kind": "event",
                "turnId": turn_id,
                "runId": run_id,
                "sequence": sequence,
                "event": event,
            }),
            Delivery::End { turn_id, run_id } => {
                json!({ "kind": "end", "turnId": turn_id, "runId": run_id })
            }
            Delivery::Perform {
                turn_id,
                run_id,
                directive,
                live,
            } => json!({
                "kind": "perform",
                "turnId": turn_id,
                "runId": run_id,
                "directive": directive,
                "live": live,
            }),
            Delivery::PerformSay { say_id, directive } => {
                json!({ "kind": "perform_say", "sayId": say_id, "directive": directive })
            }
            Delivery::TouchReaction { touch_id, reaction } => {
                json!({ "kind": "touch_reaction", "touchId": touch_id, "reaction": reaction })
            }
            Delivery::Outfit {
                outfit_id,
                rig_asset_id,
                portrait_asset_id,
            } => json!({
                "kind": "outfit",
                "outfitId": outfit_id,
                "rigAssetId": rig_asset_id,
                "portraitAssetId": portrait_asset_id,
            }),
            Delivery::Wardrobe(outfits) => json!({
                "kind": "wardrobe",
                "outfits": outfits.iter().map(WardrobeItem::json).collect::<Vec<_>>(),
            }),
            Delivery::Mood(body) => json!({ "kind": "mood", "body": body }),
            Delivery::Say { id, body } => json!({ "kind": "say", "id": id, "body": body }),
            Delivery::Conversations(sessions) => {
                json!({ "kind": "conversations", "sessions": sessions })
            }
            Delivery::Conversation(session) => {
                json!({ "kind": "conversation", "session": session })
            }
            Delivery::Notice(body) => json!({ "kind": "notice", "body": body }),
            Delivery::HostError { code } => json!({ "kind": "host_error", "code": code }),
            Delivery::Unsent { t, code } => json!({ "kind": "unsent", "t": t, "code": code }),
            Delivery::Unread { t, code } => json!({ "kind": "unread", "t": t, "code": code }),
        }
    }
}

impl Output {
    /// `{send: text}`, `{deliver: {kind, …}}` or `{drop: {why, …}}`.
    pub fn json(&self) -> Value {
        match self {
            Output::Send(text) => json!({ "send": text }),
            Output::Deliver(delivery) => json!({ "deliver": delivery.json() }),
            Output::Drop(down) => json!({ "drop": down.json() }),
        }
    }
}
