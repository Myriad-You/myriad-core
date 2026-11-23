use super::*;

fn them(text: &str) -> Line<'_> {
    Line {
        who: Who::Them,
        text,
        in_game: false,
        before: false,
        at: None,
    }
}

fn you(text: &str) -> Line<'_> {
    Line {
        who: Who::You,
        ..them(text)
    }
}

#[test]
fn what_every_talk_is_asked_comes_before_what_she_knows() {
    for examples in [Examples::Chinese, Examples::English] {
        let one = raw_settle_system_prompt(&["喜欢喝茶".to_string()], &[], examples);
        let other = raw_settle_system_prompt(&[], &[], examples);
        let at = one.find("Known for good:\n").unwrap();
        assert_eq!(one[..at], other[..other.find("Known for good:\n").unwrap()]);
        assert!(one.ends_with("Known for good:\n- 喜欢喝茶\nAsked not to keep:\n(nothing)"));
        assert!(other.ends_with("Known for good:\n(nothing yet)\nAsked not to keep:\n(nothing)"));
        let asked = raw_settle_system_prompt(&[], &["以后不要记和房东吵架的事".into()], examples);
        assert!(asked.ends_with("Asked not to keep:\n- 以后不要记和房东吵架的事"));
        assert_eq!(asked[..asked.find("Known for good:\n").unwrap()], one[..at]);
        // Long enough for a provider to cache.
        assert!(at > 8000, "{at}");
    }
}

#[test]
fn going_over_gives_facts_what_she_said_and_what_they_suggested() {
    let schema = raw_settle_schema();
    let properties = &schema["properties"];
    assert_eq!(properties["lasting"]["maxItems"], MAX_RAW_LASTING);
    assert_eq!(properties["said"]["maxItems"], MAX_RAW_SAID);
    assert_eq!(properties["putOnto"]["maxItems"], MAX_RAW_PUT_ONTO);
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|key| key.as_str())
        .collect();
    for key in [
        "lasting",
        "tookBack",
        "moment",
        "known",
        "notToKeep",
        "said",
        "own",
        "putOnto",
    ] {
        assert!(required.contains(&key), "{key}");
    }
}

#[test]
fn twelve_waiting_lines_fill_the_box_and_nothing_is_judged_at_once_yet() {
    assert!(!box_is_full(FULL_BOX - 1));
    assert!(box_is_full(FULL_BOX));
    assert_eq!(FULL_BOX, 12);
    for line in ["你记住，我下周二体检", "remind me tonight", "忘れて"] {
        assert_eq!(at_once(line), None);
    }
}

#[test]
fn a_pass_ends_after_twelve_of_their_lines_and_her_answer_to_the_last() {
    let mut lines = Vec::new();
    for _ in 0..13 {
        lines.push(them("今天去公园散步了"));
        lines.push(you("听起来不错"));
    }
    // The 12th line of theirs is at 22, her answer at 23: the next pass
    // begins with their 13th.
    assert_eq!(pass_end(&lines), 24);
    assert_eq!(pass_end(&lines[..24]), 24);
    // Words passed on are not theirs and do not count.
    let mut passed = vec![
        Line {
            who: Who::Passed,
            ..them("弹幕说好可爱")
        };
        20
    ];
    passed.push(them("我养了一只猫"));
    assert_eq!(pass_end(&passed), passed.len());
    // Lines already gone over among them do not count either.
    let mut among: Vec<Line> = lines.clone();
    for line in among.iter_mut().step_by(4) {
        line.before = true;
    }
    assert_eq!(pass_end(&among), lines.len());
}

#[test]
fn what_she_is_given_marks_lines_before_games_and_what_to_answer_again() {
    let lines = [
        Line {
            before: true,
            ..you("你周末干嘛了")
        },
        Line {
            in_game: true,
            ..them("是动物吗")
        },
        Line {
            at: Some("2026-11-21 20:15"),
            ..them("周六去爬山了")
        },
    ];
    let input: serde_json::Value = serde_json::from_str(&raw_settle_input(
        &lines,
        "2026-11-23 (Monday)",
        &serde_json::Value::Null,
        &["我在读《白塔》".to_string()],
        Some(&[2]),
    ))
    .unwrap();
    let conversation = input["conversation"].as_array().unwrap();
    assert_eq!(conversation[0]["before"], true);
    assert_eq!(conversation[0]["who"], "you");
    assert_eq!(conversation[1]["inGame"], true);
    assert_eq!(conversation[2]["index"], 2);
    assert!(conversation[2].get("before").is_none());
    assert_eq!(conversation[2]["at"], "2026-11-21 20:15");
    assert!(conversation[1].get("at").is_none());
    assert_eq!(input["yours"][0], "我在读《白塔》");
    assert_eq!(input["onlyThese"], serde_json::json!([2]));
}

fn answer(lasting: serde_json::Value, rest: serde_json::Value) -> String {
    let mut answer = serde_json::json!({
        "lasting": lasting,
        "tookBack": [],
        "letGo": [],
        "moment": [],
        "known": [],
        "said": [],
        "own": [],
        "putOnto": [],
    });
    for (key, value) in rest.as_object().unwrap() {
        answer[key] = value.clone();
    }
    answer.to_string()
}

fn fact(text: &str, evidence: &str, from: &[usize]) -> serde_json::Value {
    serde_json::json!({
        "fact": text,
        "evidence": evidence,
        "from": from,
        "concepts": [],
        "whenFor": [],
        "topic": {"on": "", "name": ""},
        "happened": {"from": "", "to": "", "about": false},
        "state": "",
        "open": null,
        "replaces": [],
    })
}

#[test]
fn facts_hold_only_to_their_own_lines_and_what_she_said_to_hers() {
    let lines = [
        Line {
            before: true,
            ..them("我上周买了辆自行车")
        },
        them("周六去爬山了，爬的是香山"),
        you("香山我也去过，秋天的红叶特别好看"),
        them("好的"),
        them("推荐你看《漂流》这部电影"),
    ];
    let raw = answer(
        serde_json::json!([
            // Their line: kept, with only its own line in from.
            fact("周六爬了香山", "周六去爬山了，爬的是香山", &[1, 2, 9]),
            // Her line is not what they said.
            fact("去过香山", "香山我也去过", &[2]),
            // A line from before the stretch is context only.
            fact("上周买了辆自行车", "我上周买了辆自行车", &[0]),
        ]),
        serde_json::json!({
            "moment": [3],
            "own": [
                {"line": "我去过香山，觉得秋天红叶好看", "evidence": "香山我也去过，秋天的红叶特别好看", "concepts": []},
                {"line": "我爬过香山", "evidence": "周六去爬山了", "concepts": []}
            ],
            "putOnto": [
                {"thing": "电影《漂流》", "evidence": "推荐你看《漂流》这部电影"},
                {"thing": "香山", "evidence": "香山我也去过"}
            ]
        }),
    );
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    let facts: Vec<&str> = got
        .settled
        .lasting
        .iter()
        .map(|one| one.fact.as_str())
        .collect();
    assert_eq!(facts, ["周六爬了香山"]);
    assert_eq!(got.settled.lasting[0].from, [1]);
    assert_eq!(got.said.len(), 1);
    assert!(got.said[0].hers);
    assert_eq!(got.put_onto, ["电影《漂流》"]);
    // Their suggestion was accounted for nowhere; '好的' need not be, and
    // the line before the stretch is not theirs to place now.
    assert_eq!(got.left, [4]);
}

#[test]
fn asked_again_only_the_lines_asked_about_count() {
    let lines = [
        them("我在一家做游戏的公司上班"),
        you("做什么岗位呀"),
        them("做美术，主要画场景"),
        them("我家住在杭州西湖边上"),
    ];
    let raw = answer(
        serde_json::json!([
            fact("在游戏公司做场景美术", "做美术，主要画场景", &[0, 2]),
            fact("住在杭州西湖边", "我家住在杭州西湖边上", &[3]),
        ]),
        serde_json::json!({
            "own": [{"line": "我问了岗位", "evidence": "做什么岗位呀", "concepts": []}],
            "tookBack": [{"entry": "住在上海", "said": 3}]
        }),
    );
    let known = ["住在上海".to_string()];
    let got = parse_raw_settle(&raw, &lines, &known, &[], 0, Some(&[2])).unwrap();
    // Only the fact from the line asked about, and only that line in it.
    assert_eq!(got.settled.lasting.len(), 1);
    assert_eq!(got.settled.lasting[0].from, [2]);
    assert!(got.settled.took_back.is_empty());
    assert!(got.said.is_empty());
    assert!(got.left.is_empty());
    // The whole stretch: the line about where they live is a taking back
    // of what she knew, so it is accounted for.
    let whole = parse_raw_settle(&raw, &lines, &known, &[], 0, None).unwrap();
    assert_eq!(whole.settled.lasting.len(), 2);
    assert!(whole.left.is_empty());
}

/// What they took back within the stretch goes by its lines: a fact from
/// those lines alone is dropped even when the answer kept it.
#[test]
fn what_they_take_back_in_the_stretch_is_let_go_by_its_lines() {
    let lines = [
        them("我下个月要去东京出差一周"),
        you("好呀，要带伞"),
        them("出差的事别记了，还没定"),
        them("我最近在学做面包"),
    ];
    let raw = answer(
        serde_json::json!([
            fact("下个月去东京出差一周", "我下个月要去东京出差一周", &[0]),
            fact("最近在学做面包", "我最近在学做面包", &[3]),
        ]),
        serde_json::json!({"letGo": [0, 1, 9], "moment": [2]}),
    );
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    let facts: Vec<&str> = got
        .settled
        .lasting
        .iter()
        .map(|one| one.fact.as_str())
        .collect();
    assert_eq!(facts, ["最近在学做面包"]);
    // Only their lines, in the stretch.
    assert_eq!(got.let_go, [0]);
    assert!(got.left.is_empty());
}

#[test]
fn a_malformed_answer_gives_nothing() {
    assert_eq!(
        parse_raw_settle("not json", &[them("我养了一只猫")], &[], &[], 0, None),
        None
    );
}

/// A pass may keep as many facts as the prompt allows it, not as many as a
/// stretch of short notes.
#[test]
fn a_pass_keeps_as_many_facts_as_it_is_asked_for() {
    let texts: Vec<String> = (0..MAX_RAW_LASTING)
        .map(|n| format!("第{n}件事我买了东西"))
        .collect();
    let lines: Vec<Line> = texts.iter().map(|text| them(text)).collect();
    let facts: Vec<serde_json::Value> = texts
        .iter()
        .enumerate()
        .map(|(n, text)| fact(&format!("买了第{n}样东西"), text, &[n]))
        .collect();
    let raw = answer(serde_json::json!(facts), serde_json::json!({}));
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), MAX_RAW_LASTING);
    const { assert!(MAX_RAW_LASTING > crate::chat_remember::MAX_LASTING) };
    assert!(got.left.is_empty());
}

/// What they ask her not to keep from now on is kept itself, held to being
/// theirs, whatever else the same line takes back; it is not an answer to
/// account for a line with, and taking it back goes by what it says.
#[test]
fn what_they_ask_her_not_to_keep_is_kept_beside_what_they_take_back() {
    let lines = [
        them("跟房东又吵了一架，他把空调费算我头上"),
        you("太过分了"),
        them("我跟房东吵架的事你以后都别记了"),
        you("好，我不记了"),
    ];
    let asked_before = ["以后不要记和妹妹吵架的事".to_string()];
    let raw = answer(
        serde_json::json!([]),
        serde_json::json!({
            "letGo": [0],
            "moment": [],
            "tookBack": [
                {"entry": "和房东因为押金吵过", "said": 2},
                {"entry": "以后不要记和妹妹吵架的事", "said": 2}
            ],
            "notToKeep": [
                {"fact": "以后不要记和房东吵架的事", "evidence": "我跟房东吵架的事你以后都别记了", "said": 2, "replaces": ["以后不要记和妹妹吵架的事", "没有这条"]},
                // Her own promise is no request, nor is one quoting no line.
                {"fact": "以后不要记那件事", "evidence": "好，我不记了", "said": 3, "replaces": []},
                {"fact": "以后不要记空调费", "evidence": "没说过的话", "said": 0, "replaces": []},
                // Said twice, and one asked before.
                {"fact": "以后不要记和房东吵架的事", "evidence": "我跟房东吵架的事", "said": 2, "replaces": []},
                {"fact": "以后不要记和妹妹吵架的事", "evidence": "我跟房东吵架的事", "said": 2, "replaces": []}
            ]
        }),
    );
    let known = ["和房东因为押金吵过".to_string()];
    let got = parse_raw_settle(&raw, &lines, &known, &asked_before, 0, None).unwrap();
    assert_eq!(
        got.asked,
        [Asked {
            fact: "以后不要记和房东吵架的事".into(),
            evidence: "我跟房东吵架的事你以后都别记了".into(),
            said: Some(2),
            replaces: vec!["以后不要记和妹妹吵架的事".into()],
        }]
    );
    // The one it takes the place of is not taken back as well.
    let taken: Vec<&str> = got
        .settled
        .took_back
        .iter()
        .map(|one| one.entry.as_str())
        .collect();
    assert_eq!(taken, ["和房东因为押金吵过"]);
    assert!(got.left.is_empty(), "{:?}", got.left);
    // Asked again about some lines alone, it gives no requests.
    let again = parse_raw_settle(&raw, &lines, &known, &asked_before, 0, Some(&[2])).unwrap();
    assert!(again.asked.is_empty());
}

/// What every talk is asked about requests not to keep something is there
/// in both ways of keeping, and one answer gives so many at most.
#[test]
fn requests_not_to_keep_are_asked_for_and_held_to_one_line() {
    use crate::chat_remember::{
        MAX_ASKED, extract_schema, extract_schema_asked, extract_system_prompt,
        extract_system_prompt_asked, read_not_to_keep,
    };
    for examples in [Examples::Chinese, Examples::English] {
        let known = ["喜欢喝茶".to_string()];
        let asked = extract_system_prompt_asked(&known, examples);
        // Only added to: what is kept as ever is asked as ever.
        let plain = extract_system_prompt(&known, examples);
        let at = plain.rfind("Known:\n").unwrap();
        assert!(asked.starts_with(&plain[..at]));
        assert!(asked.ends_with(&plain[at..]));
        assert!(asked.contains("notToKeep"));
        assert!(!plain.contains("notToKeep"));
    }
    let item = &extract_schema_asked()["properties"]["notToKeep"]["items"];
    assert_eq!(item["required"], serde_json::json!(["fact", "evidence"]));
    assert!(extract_schema()["properties"]["notToKeep"].is_null());

    let theirs = "体重的事你以后都别记 对了我养了只龟";
    let answer = |count: usize| {
        serde_json::json!({"notToKeep": (0..count)
            .map(|one| serde_json::json!({"fact": format!("不要记第{one}件事"), "evidence": "体重的事你以后都别记"}))
            .collect::<Vec<_>>()})
    };
    let got = read_not_to_keep(&answer(1), theirs, None, &[]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].said, None);
    assert!(got[0].replaces.is_empty());
    assert_eq!(
        read_not_to_keep(&answer(9), theirs, None, &[]).len(),
        MAX_ASKED
    );
    // Asked already, or in words they did not say: not again.
    assert!(read_not_to_keep(&answer(1), theirs, None, &["不要记第0件事".into()]).is_empty());
    assert!(read_not_to_keep(&answer(1), "没说过", None, &[]).is_empty());
}
