//! Her own fixed words through the key scan: an end scans what it sends a
//! model, the system prompt with it, so a fixed line written as a
//! credential's label and its value loses that word, and the native client
//! removes every wiped value everywhere in what it sends. Data in a prompt
//! (a puzzle's truth, names, what was said) is not her words, and is not
//! held to this.

use std::{fs, path::Path};

use myriad_merope::{
    avalon, group::Rolls, hosting, hosting::Rules as _, making, private_prompt, soup, undercover,
    werewolf,
};
use serde_json::json;

use super::{ok, text};

const SOUL: &str = "你是若泉。";

fn wiped(words: &str) -> bool {
    !myriad_perception_filter::secrets::scan(words)
        .wiped
        .is_empty()
}

/// What is wiped out of `words`, for the message of a failed assertion.
fn values(words: &str) -> Vec<String> {
    let found = ok("filter.secret_values", json!({ "text": words }));
    found["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one["value"].as_str().unwrap().to_string())
        .collect()
}

fn puzzle() -> soup::Game {
    soup::Game {
        surface: "面包店清晨排起长队，却没有人买面包。".into(),
        truth: "大家在等店主的猫醒来。".into(),
        keys: vec!["猫".into(), "等它醒".into()],
        asked: vec![soup::Asked {
            by: Some("阿七".into()),
            question: "和面包有关吗？".into(),
            verdict: soup::Verdict::No,
        }],
        found: vec![],
        ending: None,
        solver: None,
        started: "2026-11-23T11:00:00Z".parse().unwrap(),
        last: None,
        made: None,
    }
}

/// A seat that knows nothing yet: only the fixed words are left.
fn seated() -> hosting::SeatView {
    let seats = (0..werewolf::FEWEST)
        .map(|at| hosting::Seat {
            who: format!("qq:{at}"),
            name: format!("玩家{at}"),
            hers: at == 0,
        })
        .collect();
    let game = werewolf::Werewolf::deal(seats, &mut Rolls::new(7, Vec::new()));
    hosting::seat_view::<werewolf::Werewolf>(&game, 0)
}

fn soup_sections() -> Vec<String> {
    let mut sections = Vec::new();
    for ending in [None, Some(soup::Ending::Solved), Some(soup::Ending::GaveUp)] {
        for verdict in [
            None,
            Some(soup::Verdict::Yes),
            Some(soup::Verdict::NotAQuestion),
        ] {
            for group in [false, true] {
                let mut game = puzzle();
                game.ending = ending;
                sections.push(soup::section(&game, verdict, group, Some("阿七")));
            }
        }
    }
    sections
}

/// The prompts that hold what she must not tell: whole, as she is sent them.
fn rendered() -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = soup_sections()
        .into_iter()
        .map(|section| ("soup.section".to_string(), section))
        .collect();
    for (name, words) in [
        ("soup.start_system", soup::start_system(SOUL)),
        (
            "making.make_system",
            making::make_system(SOUL, "reading a part"),
        ),
        ("soup.judge_schema", soup::judge_schema(2).to_string()),
    ] {
        all.push((name.into(), words));
    }
    if let Some(asked) = hosting::SeatGame::prepare(undercover::KIND, Some(SOUL), &[]) {
        all.push(("undercover.prepare".into(), asked.system));
    }
    for kind in hosting::seat_kinds() {
        all.push((
            format!("{}.offer", kind.kind),
            hosting::group_offer(Some(kind.kind)),
        ));
        all.push((
            format!("{}.how", kind.kind),
            hosting::how_to_speak(kind.kind).into(),
        ));
    }
    for (name, rules, how) in [
        (
            werewolf::NAME,
            werewolf::Werewolf::RULES,
            werewolf::HOW_TO_SPEAK,
        ),
        (
            avalon::NAME,
            avalon::Avalon::RULES,
            hosting::how_to_speak(avalon::KIND),
        ),
        (
            undercover::NAME,
            undercover::Undercover::RULES,
            hosting::how_to_speak(undercover::KIND),
        ),
    ] {
        let view = seated();
        all.push((
            format!("{name} speaks"),
            hosting::speaker_prompt(SOUL, name, rules, how, &view).0,
        ));
        all.push((
            format!("{name} moves"),
            hosting::player_prompt(SOUL, name, rules, &view).0,
        ));
    }
    for name in [
        "chat.reply_instruction",
        "chat_days.pick_system",
        "chat_remember.already_system",
        "explore.compare_system",
        "heard.system",
        "life.concepts_system",
        "remembering.system",
        "speaking.chat_app_section",
        "onboarding.persona_system",
    ] {
        all.push((name.into(), text(name, json!({})).unwrap()));
    }
    for name in [
        "chat_days.day_system",
        "curiosity.wonder_system",
        "doing.choice_system",
        "explore.wonder_system",
        "inner.system",
        "inner.system_for_group",
        "life.own_day_system",
        "sharing.come_back_system",
        "sharing.system",
        "views.system",
        "wants.system",
    ] {
        all.push((name.into(), text(name, json!({ "soul": SOUL })).unwrap()));
    }
    all
}

#[test]
fn the_key_scan_leaves_her_prompts_whole() {
    let touched: Vec<_> = rendered()
        .into_iter()
        .filter(|(_, words)| wiped(words))
        .map(|(name, words)| (name, values(&words)))
        .collect();
    assert!(
        touched.is_empty(),
        "wiped out of her own words: {touched:?}"
    );
}

/// What holds her, wherever the words stand in her prompt: who she says she
/// is, what she must not make up, what she must not tell.
const HOLDS: &[&str] = &[
    "You are not a human and never claim to be one.",
    "Never make up something you saw or did in a physical place",
    "never make up having just read, listened to, watched or looked at something",
    "Never make up things they said or did",
    "if you do not remember, say so plainly or ask",
];

/// `words` with every value the scan would wipe gone from everywhere in
/// them, as the native client sends them (a value, not a place, is what it
/// takes out: one word taken for a key in one section goes from all).
fn as_sent(words: &str) -> String {
    values(words)
        .iter()
        .fold(words.to_string(), |words, value| words.replace(value, ""))
}

/// Hosting a puzzle, her prompt reaches the model as it is written, whole:
/// what she must not tell of it, and everything else that holds her.
#[test]
fn what_holds_her_stands_after_the_scan_while_she_hosts_a_puzzle() {
    for at in ["site", "mac"] {
        let home = ok("speaking.home", json!({ "at": at, "name": "星海" }))["home"].clone();
        let soul = text(
            "speaking.soul",
            json!({ "name": "若泉", "personality": "气质：想到就试\n\n她说话很短。", "home": home }),
        )
        .unwrap();
        for section in soup_sections() {
            let system =
                private_prompt::system(&soul, &[(private_prompt::Part::Game, section.clone())]);
            let wiped = ok("filter.scan_secrets", json!({ "text": system }));
            for sent in [
                wiped["text"].as_str().unwrap().to_string(),
                as_sent(&system),
            ] {
                assert!(sent.contains(&section), "{sent}");
                assert!(
                    sent.contains(
                        "never say it or hint past the answer, until it is solved or they give up"
                    ),
                    "{sent}"
                );
                for held in HOLDS {
                    assert!(sent.contains(held), "{held}\n\n{sent}");
                }
                assert_eq!(sent, system);
            }
        }
    }
    let section = soup::section(&puzzle(), Some(soup::Verdict::No), false, None);
    assert!(as_sent(&section).contains(soup::HOLD_BACK));
}

/// Playing a game of seats, what she may not know and how it goes are as
/// they are written.
#[test]
fn what_holds_her_stands_after_the_scan_while_she_plays_werewolf() {
    let (system, _) =
        hosting::player_prompt(SOUL, werewolf::NAME, werewolf::Werewolf::RULES, &seated());
    let sent = as_sent(&system);
    assert!(sent.contains(werewolf::Werewolf::RULES), "{sent}");
    assert!(
        sent.contains("whatever others were dealt or did in secret, you do not know"),
        "{sent}"
    );
    assert_eq!(sent, system);
}

/// The fixed words written in the crates her prompts come from, each string
/// on its own with its `{…}` left unfilled. Words put together only when a
/// prompt is made (a label ending one string, its value starting the next)
/// are past this; [`rendered`] covers the prompts that hold secrets whole.
/// Only [`NOT_FOR_A_MODEL`] is left out.
#[test]
fn no_fixed_words_in_the_prompt_crates_read_as_a_credential() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut touched = Vec::new();
    let mut read = 0;
    for name in ["myriad-merope", "myriad-nodes", "myriad-agent-rules"] {
        for file in sources(&crates.join(name).join("src")) {
            let source = fs::read_to_string(&file).unwrap();
            for (line, words) in literals(&source) {
                read += 1;
                if wiped(&words) && !NOT_FOR_A_MODEL.contains(&words.as_str()) {
                    let file = file.strip_prefix(&crates).unwrap().display().to_string();
                    touched.push((format!("{file}:{line}"), values(&words)));
                }
            }
        }
    }
    assert!(read > 5000, "only {read} strings read");
    assert!(touched.is_empty(), "wiped out of fixed words: {touched:?}");
}

/// Strings that look like keys and are no words of hers: no model is sent
/// any of them.
const NOT_FOR_A_MODEL: &[&str] = &[
    // A rig's layer names, looked up in the rig's own file (`rig.rs`).
    "a25d-lovestruck-heart-left",
    "a25d-lovestruck-heart-right",
    // The digests of the style references, and the field that holds one, in the contracts a portrait's
    // fingerprint is made of (`visual_contract.rs`, `sticker_avatar.rs`); the prompts that draw it have
    // neither.
    "1015f09392ee01823ce6dc1a78521d04bb734d0585864e7668f380420a4b106f",
    "fbeb294a1ec260274d7c562adf746c17bd349847f4fe3ece0b2f3f300106eb67",
    "styleReferenceSha256",
    // The alphabet a pairing code is written in (`channel.rs`).
    "0123456789ABCDEFGHJKMNPQRSTVWXYZ",
];

fn sources(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "tests" {
                found.extend(sources(&path));
            }
        } else if name.ends_with(".rs") && !name.ends_with("tests.rs") {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// A source file with its test modules left blank, what is below them kept
/// and on its own lines.
fn without_tests(source: &str) -> String {
    let mut kept = String::new();
    let mut rest = source;
    while let Some(begins) = rest.find("#[cfg(test)]\nmod tests {") {
        kept.push_str(&rest[..begins]);
        let block = &rest[begins..];
        let end = block.find("\n}\n").map_or(block.len(), |end| end + 3);
        kept.extend(block[..end].chars().filter(|one| *one == '\n'));
        rest = &block[end..];
    }
    kept.push_str(rest);
    kept
}

/// The string literals of a source file outside its tests, read as Rust
/// reads them, each with the line it begins on.
fn literals(source: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = without_tests(source).chars().collect();
    let at = |index: usize| chars.get(index).copied().unwrap_or('\0');
    let mut found = Vec::new();
    let (mut index, mut line) = (0, 1);
    while index < chars.len() {
        match at(index) {
            '\n' => {
                line += 1;
                index += 1;
            }
            '/' if at(index + 1) == '/' => {
                while index < chars.len() && at(index) != '\n' {
                    index += 1;
                }
            }
            // A character, not a lifetime.
            '\'' if at(index + 1) == '\\' => {
                index += 3;
                while index < chars.len() && at(index) != '\'' {
                    index += 1;
                }
                index += 1;
            }
            '\'' if at(index + 2) == '\'' => index += 3,
            'r' if (at(index + 1) == '"' || at(index + 1) == '#')
                && !(index > 0 && (at(index - 1).is_alphanumeric() || at(index - 1) == '_')) =>
            {
                let mut hashes = 0;
                while at(index + 1 + hashes) == '#' {
                    hashes += 1;
                }
                if at(index + 1 + hashes) != '"' {
                    index += 1;
                    continue;
                }
                let begins = line;
                let mut words = String::new();
                index += 2 + hashes;
                while index < chars.len()
                    && !(at(index) == '"' && (0..hashes).all(|n| at(index + 1 + n) == '#'))
                {
                    line += usize::from(at(index) == '\n');
                    words.push(at(index));
                    index += 1;
                }
                index += 1 + hashes;
                found.push((begins, words));
            }
            '"' => {
                let begins = line;
                let mut words = String::new();
                index += 1;
                while index < chars.len() && at(index) != '"' {
                    if at(index) != '\\' {
                        line += usize::from(at(index) == '\n');
                        words.push(at(index));
                        index += 1;
                        continue;
                    }
                    index += 2;
                    match at(index - 1) {
                        'n' => words.push('\n'),
                        't' => words.push('\t'),
                        'r' => words.push('\r'),
                        '0' => words.push('\0'),
                        'x' => index += 2,
                        'u' => {
                            while index < chars.len() && at(index) != '}' {
                                index += 1;
                            }
                            index += 1;
                        }
                        // The line goes on after the next one's indent.
                        '\n' => {
                            line += 1;
                            while at(index).is_whitespace() {
                                line += usize::from(at(index) == '\n');
                                index += 1;
                            }
                        }
                        other => words.push(other),
                    }
                }
                index += 1;
                found.push((begins, words));
            }
            _ => index += 1,
        }
    }
    found
}

#[test]
fn the_fixed_words_are_read_as_rust_reads_them() {
    let source = "// \"no\"\nlet a = \"one \\\n    two\\n\"; let b: &'static str = r#\"x\"y\"#;\nlet c = '\"'; let d = \"{z}\";";
    assert_eq!(
        literals(source),
        vec![
            (2, "one two\n".to_string()),
            (3, "x\"y".to_string()),
            (4, "{z}".to_string()),
        ]
    );
}

#[test]
fn words_below_a_test_module_are_read_on_their_own_lines() {
    let source = "let a = \"one\";\n#[cfg(test)]\nmod tests {\n    const B: &str = \"no\";\n}\n\nlet c = \"two\";\n";
    assert_eq!(
        literals(source),
        vec![(1, "one".to_string()), (7, "two".to_string())]
    );
    // Not read to the end: no waiting there.
    assert!(literals("let d = '\\").is_empty());
    assert_eq!(literals("\"\\u{1").len(), 1);
}
