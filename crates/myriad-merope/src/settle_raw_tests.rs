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
        let one = raw_settle_system_prompt(&["喜欢喝茶".to_string()], examples);
        let other = raw_settle_system_prompt(&[], examples);
        let at = one.find("Known for good:\n").unwrap();
        assert_eq!(one[..at], other[..other.find("Known for good:\n").unwrap()]);
        assert!(one.ends_with("Known for good:\n- 喜欢喝茶"));
        assert!(other.ends_with("Known for good:\n(nothing yet)"));
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
        "lasting", "tookBack", "moment", "known", "said", "own", "putOnto",
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
    let got = parse_raw_settle(&raw, &lines, &[], 0, None).unwrap();
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
    let got = parse_raw_settle(&raw, &lines, &known, 0, Some(&[2])).unwrap();
    // Only the fact from the line asked about, and only that line in it.
    assert_eq!(got.settled.lasting.len(), 1);
    assert_eq!(got.settled.lasting[0].from, [2]);
    assert!(got.settled.took_back.is_empty());
    assert!(got.said.is_empty());
    assert!(got.left.is_empty());
    // The whole stretch: the line about where they live is a taking back
    // of what she knew, so it is accounted for.
    let whole = parse_raw_settle(&raw, &lines, &known, 0, None).unwrap();
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
    let got = parse_raw_settle(&raw, &lines, &[], 0, None).unwrap();
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
        parse_raw_settle("not json", &[them("我养了一只猫")], &[], 0, None),
        None
    );
}
