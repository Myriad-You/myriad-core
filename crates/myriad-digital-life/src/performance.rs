use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPerformancePlan {
    pub cues: Vec<ChatPerformanceCue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPerformanceCue {
    pub intent: String,
    pub at_ms: u32,
    pub intensity: f32,
    pub tempo: f32,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
    pub interrupt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatPerformance {
    pub reply: String,
    pub plan: Option<ChatPerformancePlan>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEnvelope {
    reply: String,
    #[serde(default)]
    performance: Option<RawPlan>,
}

#[derive(Debug, Deserialize)]
struct RawPlan {
    #[serde(default)]
    cues: Vec<RawCue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCue {
    intent: String,
    #[serde(default)]
    at_ms: u32,
    #[serde(default = "default_intensity")]
    intensity: f32,
    #[serde(default = "default_tempo")]
    tempo: f32,
    #[serde(default = "default_fade_in")]
    fade_in_ms: u32,
    #[serde(default = "default_fade_out")]
    fade_out_ms: u32,
    #[serde(default = "default_interrupt")]
    interrupt: String,
}

pub fn parse_chat_performance(raw: &str) -> ParsedChatPerformance {
    let trimmed = raw.trim();
    let json = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    let Ok(envelope) = serde_json::from_str::<RawEnvelope>(json) else {
        return ParsedChatPerformance {
            reply: trimmed.chars().take(2_000).collect(),
            plan: None,
        };
    };
    let reply = envelope.reply.trim().chars().take(2_000).collect::<String>();
    if reply.is_empty() {
        return ParsedChatPerformance {
            reply: trimmed.chars().take(2_000).collect(),
            plan: None,
        };
    }
    let cues = envelope
        .performance
        .map(|plan| {
            plan.cues
                .into_iter()
                .take(3)
                .filter_map(sanitize_cue)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ParsedChatPerformance {
        reply,
        plan: (!cues.is_empty()).then_some(ChatPerformancePlan { cues }),
    }
}

fn sanitize_cue(cue: RawCue) -> Option<ChatPerformanceCue> {
    const INTENTS: &[&str] = &[
        "greet",
        "respond",
        "question",
        "delight",
        "emphasize",
        "listen",
        "notify",
    ];
    const INTERRUPTS: &[&str] = &["replace", "queue", "if-lower"];
    if !INTENTS.contains(&cue.intent.as_str()) || !cue.intensity.is_finite() || !cue.tempo.is_finite()
    {
        return None;
    }
    Some(ChatPerformanceCue {
        intent: cue.intent,
        at_ms: cue.at_ms.min(5_000),
        intensity: cue.intensity.clamp(0.2, 1.4),
        tempo: cue.tempo.clamp(0.5, 1.6),
        fade_in_ms: cue.fade_in_ms.clamp(40, 600),
        fade_out_ms: cue.fade_out_ms.clamp(60, 800),
        interrupt: if INTERRUPTS.contains(&cue.interrupt.as_str()) {
            cue.interrupt
        } else {
            default_interrupt()
        },
    })
}

fn default_intensity() -> f32 {
    1.0
}

fn default_tempo() -> f32 {
    1.0
}

fn default_fade_in() -> u32 {
    150
}

fn default_fade_out() -> u32 {
    220
}

fn default_interrupt() -> String {
    "if-lower".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bounds_model_generated_performance() {
        let parsed = parse_chat_performance(
            r#"{"reply":"Hello!","performance":{"cues":[{"intent":"delight","atMs":9000,"intensity":9,"tempo":0.1,"interrupt":"unsafe"}]}}"#,
        );
        assert_eq!(parsed.reply, "Hello!");
        let cue = &parsed.plan.unwrap().cues[0];
        assert_eq!(cue.intent, "delight");
        assert_eq!(cue.at_ms, 5_000);
        assert_eq!(cue.intensity, 1.4);
        assert_eq!(cue.tempo, 0.5);
        assert_eq!(cue.interrupt, "if-lower");
    }

    #[test]
    fn preserves_plain_replies_and_discards_invalid_cues() {
        assert_eq!(parse_chat_performance("plain reply").reply, "plain reply");
        assert!(parse_chat_performance("plain reply").plan.is_none());
        let invalid = parse_chat_performance(
            r#"{"reply":"Hi","performance":{"cues":[{"intent":"execute-code"}]}}"#,
        );
        assert!(invalid.plan.is_none());
    }
}
