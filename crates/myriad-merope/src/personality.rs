//! Her personality as one text, from the fields her persona is set up in:
//! the text her soul carries (see `speaking::soul`). The labels are in her
//! own language (`persona.language`), not in the language of whoever is
//! looking at the setup: her personality does not change with the reader.

use serde_json::Value;

/// The fields of her persona that make her personality, read and tidied
/// (see [`Fields::from_saved`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fields {
    pub summary: String,
    pub temperament: Vec<String>,
    pub likes: Vec<String>,
    pub drives: Vec<String>,
    pub social_style: String,
    pub speech_style: String,
}

/// Traits a field may hold, at most.
pub const MAX_ITEMS: usize = 12;

/// Whether a value counts as given where the setup page reads `a || b`.
fn given(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(given) => *given,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// A list field as saved, tidied: its strings, split again where one holds
/// several (`、`, `;`, `/`, `|`), trimmed, the empty ones left out, at most
/// [`MAX_ITEMS`]. What is not a list of strings is no traits.
fn list(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("、")
        .split(['、', ';', '/', '|'])
        .map(trim)
        .filter(|item| !item.is_empty())
        .take(MAX_ITEMS)
        .map(str::to_string)
        .collect()
}

fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}

impl Fields {
    /// Her persona's fields as saved (`persona_json`, as it is), read the
    /// way the site's setup pages read them: older personas keep their
    /// traits in `traits` and their voice in `voice`.
    pub fn from_saved(saved: &Value) -> Self {
        let field = |name: &str| saved.get(name);
        let temperament = match field("temperament") {
            Some(temperament) if given(temperament) => Some(temperament),
            _ => field("traits"),
        };
        Self {
            summary: text(field("summary")).unwrap_or_default(),
            temperament: list(temperament),
            likes: list(field("likes")),
            drives: list(field("drives")),
            social_style: text(field("socialStyle")).unwrap_or_default(),
            speech_style: text(field("speechStyle"))
                .or_else(|| text(field("voice")))
                .unwrap_or_default(),
        }
    }
}

/// The labels of temperament, likes, drives, social and speech, by language
/// (the site's setup pages say the same).
const LABELS: [(&str, [&str; 5]); 7] = [
    (
        "de-DE",
        ["Temperament", "Vorlieben", "Antrieb", "Sozial", "Stimme"],
    ),
    (
        "en-US",
        ["Temperament", "Likes", "Drive", "Social", "Voice"],
    ),
    (
        "fr-FR",
        ["Tempérament", "Goûts", "Motivation", "Social", "Voix"],
    ),
    ("ja-JP", ["気質", "好み", "原動力", "社交", "話し方"]),
    ("ko-KR", ["기질", "좋아하는 것", "동기", "사교", "목소리"]),
    ("zh-CN", ["气质", "喜好", "驱动力", "社交", "表达"]),
    ("zh-TW", ["氣質", "喜好", "驅動力", "社交", "表達"]),
];

/// The labels for `language`; English for one that has none.
pub fn labels(language: &str) -> [&'static str; 5] {
    LABELS
        .iter()
        .find(|(known, _)| *known == language)
        .or_else(|| LABELS.iter().find(|(known, _)| *known == "en-US"))
        .map(|(_, labels)| *labels)
        .unwrap_or_default()
}

/// What `String.prototype.trim` takes off, so a personality flattened here
/// is the one the site's setup page flattened, to the byte.
fn trim(text: &str) -> &str {
    const SPACES: [char; 14] = [
        '\t', '\n', '\u{0B}', '\u{0C}', '\r', ' ', '\u{A0}', '\u{1680}', '\u{2028}', '\u{2029}',
        '\u{202F}', '\u{205F}', '\u{3000}', '\u{FEFF}',
    ];
    text.trim_matches(|c: char| SPACES.contains(&c) || ('\u{2000}'..='\u{200A}').contains(&c))
}

/// Her personality from its fields: a labelled line for each field that is
/// filled (traits run on, `、` between them, `; ` in English, where a trait
/// may hold a comma), then the summary after a blank line.
pub fn flatten(fields: &Fields, language: &str) -> String {
    let [temperament, likes, drives, social, speech] = labels(language);
    let joiner = if language == "en-US" { "; " } else { "、" };
    let mut lines: Vec<String> = Vec::new();
    for (label, values) in [
        (temperament, &fields.temperament),
        (likes, &fields.likes),
        (drives, &fields.drives),
    ] {
        if !values.is_empty() {
            lines.push(format!("{label}：{}", values.join(joiner)));
        }
    }
    for (label, value) in [
        (social, &fields.social_style),
        (speech, &fields.speech_style),
    ] {
        let value = trim(value);
        if !value.is_empty() {
            lines.push(format!("{label}：{value}"));
        }
    }
    let summary = trim(&fields.summary);
    if !summary.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(summary.to_string());
    }
    lines.join("\n")
}

/// [`flatten`] of her persona as saved (see [`Fields::from_saved`]).
pub fn flatten_saved(saved: &Value, language: &str) -> String {
    flatten(&Fields::from_saved(saved), language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn her_fields_read_as_one_personality_in_her_own_language() {
        let fields = Fields {
            summary: "  她说话很短，熟了以后会开玩笑。\u{3000}".into(),
            temperament: vec!["想到就试".into(), "沉得住气".into()],
            likes: vec!["听人讲下雨天的事".into()],
            drives: Vec::new(),
            social_style: " ".into(),
            speech_style: "句子短，先回答再问。".into(),
        };
        assert_eq!(
            flatten(&fields, "zh-CN"),
            "气质：想到就试、沉得住气\n喜好：听人讲下雨天的事\n表达：句子短，先回答再问。\n\n她说话很短，熟了以后会开玩笑。"
        );
        assert!(flatten(&fields, "en-US").starts_with("Temperament：想到就试; 沉得住气"));
        // A language with no labels of its own reads in English.
        assert!(flatten(&fields, "xx").starts_with("Temperament：想到就试、"));
        let only = Fields {
            summary: "只有一段。".into(),
            ..Fields::default()
        };
        assert_eq!(flatten(&only, "ja-JP"), "只有一段。");
        assert_eq!(flatten(&Fields::default(), "zh-CN"), "");
    }

    #[test]
    fn a_persona_as_saved_is_read_as_the_setup_page_reads_it() {
        let saved = serde_json::json!({
            "summary": "她说话很短。",
            "traits": [" 想到就试 ", "沉得住气、嘴硬心软", "", 3],
            "likes": "陪人打牌",
            "drives": ["a/b|c;d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n"],
            "voice": "句子短。",
            "visualIdentity": {}
        });
        let fields = Fields::from_saved(&saved);
        assert_eq!(fields.temperament, ["想到就试", "沉得住气", "嘴硬心软"]);
        assert!(fields.likes.is_empty());
        assert_eq!(fields.drives.len(), MAX_ITEMS);
        assert_eq!(fields.drives[..4], ["a", "b", "c", "d"]);
        assert_eq!(fields.speech_style, "句子短。");
        // An empty list is given: the older field is not read.
        let empty = serde_json::json!({ "temperament": [], "traits": ["x"], "speechStyle": "", "voice": "y" });
        let fields = Fields::from_saved(&empty);
        assert!(fields.temperament.is_empty());
        assert_eq!(fields.speech_style, "");
        assert_eq!(flatten_saved(&serde_json::json!("不是对象"), "zh-CN"), "");
    }
}
