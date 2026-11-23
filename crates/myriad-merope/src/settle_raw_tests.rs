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

/// What they took back within the stretch goes by its lines: a fact that is
/// all those lines say is dropped even when the answer kept it.
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

/// A long line is gone over in parts, all of it, each ending where a
/// sentence does when one does near there.
#[test]
fn a_long_line_is_gone_over_in_parts_none_of_it_left_out() {
    assert_eq!(line_parts("短的"), ["短的"]);
    let long = format!("{}。{}", "说".repeat(550), "完".repeat(700));
    let parts = line_parts(&long);
    assert_eq!(parts.concat(), long);
    assert_eq!(parts.len(), 3);
    assert!(parts[0].ends_with('。'));
    assert!(parts.iter().all(|part| part.chars().count() <= PART_CHARS));
    // With nowhere to end, at the length; the whole of it shown.
    assert_eq!(line_parts(&"说".repeat(600)).len(), 1);
    assert_eq!(line_parts(&"说".repeat(601)).len(), 2);
    let shown = raw_settle_input(
        &[them(&long)],
        "2026-11-23 (Monday)",
        &serde_json::Value::Null,
        &[],
        None,
    );
    assert!(shown.contains(&long));
}

/// What they took back in a line asked about again is taken back; in one
/// not asked about, it is as the first answer had it.
#[test]
fn asked_again_a_line_may_take_back_what_she_knew() {
    let lines = [them("在学吉他"), them("吉他那个当我没说过吧")];
    let raw = answer(
        serde_json::json!([]),
        serde_json::json!({"tookBack": [{"entry": "会弹吉他", "said": 1}], "letGo": [0]}),
    );
    let known = ["会弹吉他".to_string()];
    let again = parse_raw_settle(&raw, &lines, &known, &[], 0, Some(&[1])).unwrap();
    assert_eq!(again.settled.took_back.len(), 1);
    assert!(again.left.is_empty());
    // An earlier line of the stretch it takes back too; not a later one.
    assert_eq!(again.let_go, [0]);
    let other = parse_raw_settle(&raw, &lines, &known, &[], 0, Some(&[0])).unwrap();
    assert!(other.settled.took_back.is_empty());
    assert_eq!(other.let_go, [0]);
    let later = answer(serde_json::json!([]), serde_json::json!({"letGo": [0, 1]}));
    let other = parse_raw_settle(&later, &lines, &known, &[], 0, Some(&[0])).unwrap();
    assert_eq!(other.let_go, [0]);
}

/// One line says something they take back later and something else: the
/// other is kept, and the line is not let go whole. Said of what they ask her
/// not to keep too.
#[test]
fn a_line_taken_back_in_part_keeps_the_rest() {
    let lines = [
        them("下个月去东京出差，另外我养了只猫"),
        you("猫叫什么"),
        them("叫年糕。出差的事当我没说"),
    ];
    for end in [
        serde_json::json!({"letGo": [0]}),
        serde_json::json!({"letGo": [0], "notToKeep": [{"fact": "不要记出差的事", "evidence": "出差的事当我没说", "said": 2, "replaces": []}]}),
    ] {
        for (evidence, from) in [("我养了只猫", &[0][..]), ("我养了只猫 / 叫年糕", &[0, 2])]
        {
            let raw = answer(
                serde_json::json!([fact("养了只猫", evidence, from)]),
                end.clone(),
            );
            let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
            assert_eq!(got.settled.lasting.len(), 1);
            assert!(got.let_go.is_empty());
        }
    }
    // Nothing else is taken from it: it is let go whole.
    let raw = answer(serde_json::json!([]), serde_json::json!({"letGo": [0]}));
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    assert_eq!(got.let_go, [0]);
}

/// What they say again another way takes the place of what she knew; what
/// they took back of it does not keep it from that.
#[test]
fn a_fact_let_go_replaces_nothing() {
    let lines = [them("我搬到上海了"), you("哇"), them("说错了，是杭州")];
    let known = ["住在北京".to_string()];
    let moved = |to: &str, evidence: &str, from: usize| {
        let mut fact = fact(to, evidence, &[from]);
        fact["replaces"] = serde_json::json!(["住在北京"]);
        fact
    };
    let raw = answer(
        serde_json::json!([
            moved("搬到了上海", "我搬到上海了", 0),
            moved("从北京搬到了杭州", "是杭州", 2),
        ]),
        serde_json::json!({"letGo": [0]}),
    );
    let got = parse_raw_settle(&raw, &lines, &known, &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), 1);
    assert_eq!(got.settled.lasting[0].replaces, ["住在北京"]);
    assert_eq!(got.settled.unkept_replaces, 0);
    assert_eq!(got.let_go, [0]);
}

/// What is quoted is in the lines it is taken from; a single character is
/// all a line says.
#[test]
fn what_is_quoted_is_in_the_lines_it_is_from() {
    let lines = [
        you("你吃素？"),
        them("对。"),
        them("我是在成都长大的"),
        them("分手了"),
    ];
    let kept = |evidence: &str, from: &[usize]| {
        let raw = answer(
            serde_json::json!([fact("一件事", evidence, from)]),
            serde_json::json!({"moment": [2]}),
        );
        let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
        let mut from: Vec<_> = got
            .settled
            .lasting
            .into_iter()
            .map(|one| one.from)
            .collect();
        assert!(from.len() <= 1);
        from.pop()
    };
    assert_eq!(kept("对", &[1]), Some(vec![1]));
    assert_eq!(kept("我", &[2]), None);
    assert_eq!(kept("我 / 是", &[2]), Some(vec![2]));
    assert_eq!(kept("我搬家了", &[2]), None);
    assert_eq!(kept("我是在成都长大的 / 分手了", &[2, 3]), Some(vec![2, 3]));
    // Those it quotes, when those it names do not say it.
    assert_eq!(kept("我是在成都长大的", &[1]), Some(vec![2]));
    assert_eq!(kept("我是在成都长大的 / 分手了", &[2]), Some(vec![2, 3]));
    // With none named, the lines it quotes; a single character of a line
    // names none.
    assert!(quoted_lines("我 / 是", &lines, &[false, true, true, true], &[]).is_empty());
}

/// What is quoted is of one line for each part of it.
#[test]
fn a_part_of_what_is_quoted_is_of_one_line() {
    let lines = [
        them("不是吧，又下雨"),
        them("我周五考试，不是周四"),
        them("不是"),
        them("那就不是我的问题"),
        them("我下周三去看牙医"),
        them("累"),
    ];
    let of = |evidence: &str, from: &[usize]| {
        let raw = answer(
            serde_json::json!([fact("一件事", evidence, from)]),
            serde_json::json!({}),
        );
        let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
        got.settled.lasting[0].from.clone()
    };
    for from in [&[][..], &[5]] {
        // All of it in one line.
        assert_eq!(of("我周五考试 / 不是", from), [1]);
        // The line that says only that, before one it is in.
        assert_eq!(of("不是", from), [2]);
        assert_eq!(of("我下周三去看牙医 / 不是", from), [2, 4]);
        // Each part, the first line it is in.
        assert_eq!(of("下周三去看牙医 / 又下雨", from), [0, 4]);
        assert_eq!(of("就不是", from), [3]);
        assert_eq!(of("下雨 / 我的问题", from), [0, 3]);
    }
    // Those named, for the parts they say.
    assert_eq!(of("不是", &[3]), [3]);
    assert_eq!(of("就不是 / 看牙医", &[3]), [3, 4]);
    assert_eq!(of("不是 / 看牙医", &[0]), [0, 4]);
    let lines = [them("不是"), them("是不是要下雨"), them("我不是很喜欢")];
    let all = [true; 3];
    assert_eq!(quoted_lines("不是", &lines, &all, &[]), [0]);
    let lines = [them("我体重的事别记"), them("今天又胖了"), them("今天")];
    assert_eq!(
        quoted_lines("我体重的事 / 今天", &lines, &all, &[0]),
        [0, 2]
    );
    assert_eq!(
        quoted_lines("我体重的事 / 今天", &lines, &all, &[0, 1]),
        [0, 1]
    );
}

/// A line of a game says nothing of them, whatever the answer takes from it.
#[test]
fn a_line_of_a_game_is_never_taken_from() {
    let mut guess = them("他是被房东推下去的吗，以后别记这个");
    guess.in_game = true;
    let lines = [guess, you("不是"), them("我养了只猫叫年糕")];
    let raw = answer(
        serde_json::json!([
            fact("被房东推下去", "他是被房东推下去的吗", &[0]),
            fact("养了只猫叫年糕", "我养了只猫叫年糕", &[2]),
        ]),
        serde_json::json!({
            "letGo": [0],
            "putOnto": [{"thing": "有人说的", "evidence": "他是被房东推下去的吗"}],
            "notToKeep": [{"fact": "不要记这个", "evidence": "以后别记这个", "said": 0, "replaces": []}],
        }),
    );
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), 1);
    assert!(got.put_onto.is_empty() && got.asked.is_empty() && got.let_go.is_empty());
}

/// Asked again what they asked already, word for word, its line is accounted
/// for; one quoting another line than it names is in the one it quotes.
#[test]
fn a_request_is_in_the_line_it_quotes() {
    let lines = [them("我的体重你别记"), them("说过了，体重别记啊")];
    let raw = answer(
        serde_json::json!([]),
        serde_json::json!({"notToKeep": [
            {"fact": "不要记自己的体重", "evidence": "体重别记啊", "said": 1, "replaces": []},
            {"fact": "不要记自己的身高", "evidence": "体重别记啊", "said": 0, "replaces": []},
        ]}),
    );
    let before = ["不要记自己的体重".to_string()];
    let got = parse_raw_settle(&raw, &lines, &[], &before, 0, None).unwrap();
    assert_eq!(got.asked.len(), 1);
    assert_eq!(got.asked[0].fact, "不要记自己的身高");
    assert_eq!(got.asked[0].said, Some(1));
    assert_eq!(got.left, [0]);
    assert_eq!(
        raw_settle_schema()["properties"]["tookBack"]["maxItems"],
        80
    );
}

#[test]
fn a_request_may_say_what_it_is_of_in_another_line() {
    let lines = [them("我体重的事"), them("以后都别记了"), them("我养了只猫")];
    for (evidence, said, asked) in [
        ("我体重的事 / 以后都别记了", 1, 1),
        ("我体重的事 / 以后都别记了", 2, 0),
        ("我身高的事 / 以后都别记了", 1, 0),
        // Told in two lines, neither the one it names; in one, the one it is in.
        ("以后都别记了", 2, 1),
        ("以后都别提了", 2, 0),
    ] {
        let raw = answer(
            serde_json::json!([]),
            serde_json::json!({"notToKeep": [
                {"fact": "不要记自己的体重", "evidence": evidence, "said": said, "replaces": []},
            ]}),
        );
        let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
        assert_eq!(got.asked.len(), asked, "{evidence} {said}");
    }
}

#[test]
fn what_is_left_of_a_line_is_counted_as_it_is_written() {
    for (line, evidence, left) in [
        ("下个月去东京出差，另外我养了只猫", "我养了只猫", 10),
        ("我下个月要去东京出差一周", "我下个月要去东京出差", 2),
        // Its punctuation, and what two parts both quote, count once at most.
        ("我去东京出差一周。", "我去东京出差 / 出差一周", 0),
        ("我养猫。我养猫了", "我养猫", 1),
        // Words, not letters (an apostrophe does not part one).
        ("I am going to Tokyo well", "I am going to Tokyo", 1),
        (
            "I'm off to Tokyo, and I have a cat now",
            "I have a cat now",
            5,
        ),
        ("东京 business trip 下周", "东京", 4),
    ] {
        assert_eq!(unquoted(line, evidence), (left, true), "{line}");
    }
    assert_eq!(unquoted("分手了", "猫"), (3, false));
}

#[test]
fn a_fact_of_a_line_let_go_stays_only_beside_enough_of_it() {
    for (line, evidence, kept) in [
        (
            "I have a trip to Tokyo next week",
            "I have a trip to Tokyo next",
            0,
        ),
        ("I am going to Tokyo well", "I am going to Tokyo", 0),
        ("I have a trip to Tokyo, and I got a cat", "I got a cat", 1),
        ("我去东京出差一周。", "我去东京出差 / 出差一周", 0),
    ] {
        let lines = [them(line), them("当我没说")];
        let raw = answer(
            serde_json::json!([fact("一件事", evidence, &[0])]),
            serde_json::json!({"letGo": [0]}),
        );
        let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
        assert_eq!(got.settled.lasting.len(), kept, "{line}");
        assert_eq!(got.doubted.len(), kept, "{line}");
        assert_eq!(got.let_go.is_empty(), kept > 0, "{line}");
    }
}

#[test]
fn a_fact_of_lines_let_go_is_looked_at_again_before_it_replaces_anything() {
    let lines = [
        them("我下个月要去东京出差一周"),
        them("我养了只猫"),
        them("出差的事当我没说"),
    ];
    let known = ["下个月要去大阪出差".to_string()];
    let mut trip = fact("下个月要去东京出差", "我下个月要去东京出差一", &[0]);
    trip["evidence"] = "下个月要去东京".into();
    trip["replaces"] = serde_json::json!(known);
    let raw = answer(
        serde_json::json!([trip, fact("养了只猫", "我养了只猫", &[1])]),
        serde_json::json!({"letGo": [0]}),
    );
    let got = || parse_raw_settle(&raw, &lines, &known, &[], 1, None).unwrap();
    let kept = |got: &RawSettled| -> Vec<String> {
        got.settled
            .lasting
            .iter()
            .map(|one| one.fact.clone())
            .collect()
    };
    let first = got();
    // Those no one doubts come first.
    assert_eq!(kept(&first), ["养了只猫", "下个月要去东京出差"]);
    assert_eq!(
        first.doubted,
        [Doubted {
            fact: 1,
            replaces: known.to_vec()
        }]
    );
    assert!(first.settled.lasting[1].replaces.is_empty());
    assert_eq!(first.settled.unkept_replaces, 1);
    assert!(first.let_go.is_empty());

    let mut taken = got();
    taken.doubts_told(&[Some(true)]);
    assert_eq!(kept(&taken), ["养了只猫"]);
    assert_eq!(taken.let_go, [0]);
    assert!(taken.doubted.is_empty());

    let mut said = got();
    said.doubts_told(&[Some(false)]);
    assert_eq!(said.settled.lasting[1].replaces, known);
    assert_eq!(said.settled.unkept_replaces, 0);
    assert!(said.let_go.is_empty());

    let mut untold = got();
    untold.doubts_told(&[None]);
    assert_eq!(kept(&untold).len(), 2);
    assert!(untold.settled.lasting[1].replaces.is_empty());
    assert!(untold.let_go.is_empty());
}

#[test]
fn a_fact_no_one_doubts_is_never_the_one_left_out() {
    let mut lines = vec![them("下个月去东京出差，另外我养了只猫")];
    let texts: Vec<String> = (0..MAX_RAW_LASTING)
        .map(|at| format!("第{at}件事情"))
        .collect();
    lines.extend(texts.iter().map(|text| them(text)));
    let mut facts = vec![fact("养了只猫", "我养了只猫", &[0])];
    facts.extend(
        texts
            .iter()
            .enumerate()
            .map(|(at, text)| fact(text, text, &[at + 1])),
    );
    // As many as may be: the doubted one gives way.
    for more in [0, 1] {
        let raw = answer(
            serde_json::json!(facts[..MAX_RAW_LASTING + more]),
            serde_json::json!({"letGo": [0]}),
        );
        let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
        assert_eq!(got.settled.lasting.len(), MAX_RAW_LASTING);
        assert_eq!(got.settled.lasting[0].fact, texts[0]);
        assert_eq!(got.doubted.len(), 1 - more);
    }
}

#[test]
fn a_short_line_of_letters_need_not_be_accounted_for() {
    let lines = [them("lol"), them("well"), them("嗯嗯"), them("分手了")];
    let raw = answer(serde_json::json!([]), serde_json::json!({}));
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    assert_eq!(got.left, [1, 3]);
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
    // Asked again about some lines alone, only those lines' requests.
    let again = parse_raw_settle(&raw, &lines, &known, &asked_before, 0, Some(&[2])).unwrap();
    assert_eq!(again.asked, got.asked);
    let other = parse_raw_settle(&raw, &lines, &known, &asked_before, 0, Some(&[0])).unwrap();
    assert!(other.asked.is_empty());
}

/// What they asked her not to keep is taken back only in a line of theirs,
/// and not when the answer has them asking it all the same.
#[test]
fn what_they_asked_not_to_keep_is_taken_back_only_in_a_line() {
    let lines = [them("体重现在可以记了"), them("算了还是别记体重吧")];
    let asked_before = ["不要记体重".to_string()];
    let taken = |rest: serde_json::Value| -> Vec<Option<usize>> {
        let raw = answer(serde_json::json!([]), rest);
        parse_raw_settle(&raw, &lines, &[], &asked_before, 0, None)
            .unwrap()
            .settled
            .took_back
            .iter()
            .map(|one| one.said)
            .collect()
    };
    let back = serde_json::json!([{"entry": "不要记体重", "said": 0}]);
    assert_eq!(taken(serde_json::json!({"tookBack": back})), [Some(0)]);
    for said in [
        serde_json::json!("不要记体重"),
        serde_json::json!({"entry": "不要记体重", "said": 7}),
    ] {
        assert!(taken(serde_json::json!({"tookBack": [said]})).is_empty());
    }
    let again = serde_json::json!([
        {"fact": "不要记体重", "evidence": "算了还是别记体重吧", "said": 1, "replaces": []}
    ]);
    assert!(taken(serde_json::json!({"tookBack": back, "notToKeep": again})).is_empty());
}

/// A part of what she knew taken back, the rest kept by a fact: when that
/// fact goes after all, the whole of it is taken back, not left as it was.
#[test]
fn a_part_taken_back_whose_rest_is_not_kept_is_taken_back() {
    let lines = [
        them("我在杭州上班"),
        them("公司名字别记了"),
        them("我现在换到上海上班了"),
    ];
    let mut rest = fact("在杭州上班", "我在杭州上班", &[0]);
    rest["replaces"] = serde_json::json!(["在杭州的云上公司上班"]);
    let raw = answer(
        serde_json::json!([rest]),
        serde_json::json!({
            "tookBack": [{"entry": "在杭州的云上公司上班", "said": 1}],
            "letGo": [0], "moment": [2]
        }),
    );
    let known = ["在杭州的云上公司上班".to_string()];
    let got = parse_raw_settle(&raw, &lines, &known, &[], 0, None).unwrap();
    assert!(got.settled.lasting.is_empty());
    assert_eq!(got.settled.took_back[0].said, Some(1));
    // Kept, the fact replaces it.
    let kept = raw.replace("\"letGo\":[0]", "\"letGo\":[]");
    let got = parse_raw_settle(&kept, &lines, &known, &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), 1);
    assert!(got.settled.took_back.is_empty());
    // Asked again about the other line, the fact is none of its.
    let again = parse_raw_settle(&kept, &lines, &known, &[], 0, Some(&[1])).unwrap();
    assert!(again.settled.lasting.is_empty());
    assert_eq!(again.settled.took_back.len(), 1);
}

/// A fact naming no line is of the lines it quotes, and goes with them.
#[test]
fn a_fact_naming_no_line_is_of_those_it_quotes() {
    let lines = [them("我下个月要去东京出差"), them("我在学做面包")];
    let raw = answer(
        serde_json::json!([
            fact("下个月去东京出差", "下个月要去东京出差", &[]),
            fact("在学做面包", "我在学做面包", &[]),
        ]),
        serde_json::json!({"letGo": [0]}),
    );
    let got = parse_raw_settle(&raw, &lines, &[], &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), 1);
    assert_eq!(got.settled.lasting[0].from, [1]);
}

/// A line is shown as it was said, its line breaks and wide spaces too:
/// quoted with them, it still holds.
#[test]
fn a_quote_holds_with_the_spaces_a_line_was_shown_with() {
    let line = "周六去爬山了\n爬的是　香山";
    let raw = answer(
        serde_json::json!([fact("周六爬了香山", line, &[0])]),
        serde_json::json!({"notToKeep": [
            {"fact": "不要记爬山", "evidence": "爬的是　香山", "said": 0, "replaces": []}
        ]}),
    );
    let got = parse_raw_settle(&raw, &[them(line)], &[], &[], 0, None).unwrap();
    assert_eq!(got.settled.lasting.len(), 1);
    assert_eq!(got.asked.len(), 1);
}

/// A pass holds so many characters, theirs and hers: a line of theirs
/// longer than that is a pass of its own, and none is cut.
#[test]
fn a_pass_holds_so_many_characters() {
    let (long, short) = ("说".repeat(PASS_CHARS - 10), "说".repeat(20));
    let lines = [them(&long), you(&short), them(&short), them("嗯")];
    assert_eq!(pass_end(&lines), 2);
    assert_eq!(pass_end(&lines[2..]), 2);
    let alone = "说".repeat(PASS_CHARS * 2);
    assert_eq!(pass_end(&[them(&alone), them("嗯")]), 1);
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

/// Asked for with when each fact happened as well: both said in full, when
/// it happened first, and nothing else of it changed.
#[test]
fn requests_not_to_keep_are_asked_for_beside_when_it_happened() {
    use crate::chat_remember::{
        extract_schema_asked, extract_schema_dated, extract_schema_dated_asked,
        extract_system_prompt, extract_system_prompt_asked, extract_system_prompt_dated,
        extract_system_prompt_dated_asked,
    };
    for examples in [Examples::Chinese, Examples::English] {
        // A known fact that reads like where they begin parts nothing.
        for known in [vec![], vec!["a note titled Known:\nfoo".to_string()]] {
            let plain = extract_system_prompt(&known, examples);
            let asked = extract_system_prompt_asked(&known, examples);
            let both = extract_system_prompt_dated_asked(&known, examples);
            let at = plain.find("Known:\n").unwrap();
            let (before, after) = plain.split_at(at);
            let added = |prompt: &str| {
                prompt
                    .strip_prefix(before)
                    .and_then(|rest| rest.strip_suffix(after))
                    .unwrap()
                    .to_string()
            };
            let happened = format!("{} ", crate::happened::happened_only());
            assert_eq!(added(&both), format!("{happened}{}", added(&asked)));
            if known.is_empty() {
                let dated = extract_system_prompt_dated(&known, examples);
                assert_eq!(added(&dated), happened);
            }
        }
    }
    let both = extract_schema_dated_asked();
    let (mut dated, mut asked) = (extract_schema_dated(), extract_schema_asked());
    assert_eq!(both["properties"]["facts"], dated["properties"]["facts"]);
    assert_eq!(
        both["properties"]["notToKeep"],
        asked["properties"]["notToKeep"]
    );
    assert_eq!(both["required"], asked["required"]);
    for schema in [&mut dated, &mut asked] {
        let properties = schema["properties"].as_object_mut().unwrap();
        properties.remove("facts");
        properties.remove("notToKeep");
    }
    assert_eq!(dated["properties"], asked["properties"]);
    assert_eq!(
        both["properties"].as_object().unwrap().len(),
        dated["properties"].as_object().unwrap().len() + 2
    );
}

/// An answer that asks nothing is told apart from one that cannot be read,
/// and one bad request takes no other with it.
#[test]
fn an_answer_that_cannot_be_read_is_not_one_that_asks_nothing() {
    use crate::chat_remember::{not_to_keep_unread, read_not_to_keep};
    let theirs = "体重的事你以后都别记";
    let good = serde_json::json!({"fact": "不要记体重的事", "evidence": theirs});
    let bad = serde_json::json!({"fact": "不要记工作的事"});
    let unsaid = serde_json::json!({"fact": "不要记工作的事", "evidence": "工作的事别记"});
    for (answer, unread, read) in [
        (serde_json::json!({}), false, 0),
        (serde_json::json!({"notToKeep": []}), false, 0),
        (serde_json::json!({"notToKeep": [good]}), false, 1),
        (
            serde_json::json!({"notToKeep": [bad, good, unsaid, 3]}),
            false,
            1,
        ),
        (serde_json::json!({"notToKeep": [bad]}), true, 0),
        (serde_json::json!({"notToKeep": [unsaid, null]}), true, 0),
        (serde_json::json!({"notToKeep": null}), true, 0),
        (serde_json::json!({"notToKeep": good}), true, 0),
        (serde_json::json!([good]), true, 0),
        (serde_json::json!("notToKeep"), true, 0),
    ] {
        assert_eq!(
            not_to_keep_unread(&answer, theirs, None),
            unread,
            "{answer}"
        );
        assert_eq!(
            read_not_to_keep(&answer, theirs, None, &[]).len(),
            read,
            "{answer}"
        );
    }
    // Of lines, asked already: read, though there is nothing new in it.
    let answer = serde_json::json!({"notToKeep": [{"fact": "不要记体重的事", "evidence": theirs, "said": 0}]});
    let (by, before) = ([true], ["不要记体重的事".to_string()]);
    assert!(read_not_to_keep(&answer, theirs, Some(&by), &before).is_empty());
    assert!(!not_to_keep_unread(&answer, theirs, Some(&by)));
    // Of lines, one she said is none of theirs.
    let said = serde_json::json!({"notToKeep": [{"fact": "不要记体重的事", "evidence": theirs, "said": 1}]});
    assert!(not_to_keep_unread(&said, theirs, Some(&[true, false])));
    assert!(!not_to_keep_unread(&said, theirs, Some(&[false, true])));
}

/// What they sent in two goes is there over two lines, however the answer
/// copies it; words they did not say one after the other are not.
#[test]
fn a_request_sent_in_two_goes_is_in_their_words() {
    use crate::chat_remember::{not_to_keep_unread, read_not_to_keep};
    let theirs = "我的体重\n以后别记了\r\n  今天去爬山";
    for (evidence, holds) in [
        ("我的体重\n以后别记了", true),
        ("我的体重 以后别记了", true),
        ("以后别记了\n今天去爬山", true),
        ("我的体重", true),
        ("我的体重 / 以后别记了", true),
        ("我的体重以后别记了", false),
        ("我的体重\n今天去爬山", false),
        ("以后别记了\n我的体重", false),
    ] {
        let one = serde_json::json!({"fact": "不要记体重的事", "evidence": evidence, "said": 0});
        let answer = serde_json::json!({"notToKeep": [one]});
        for by in [None, Some(&[true][..])] {
            let read = read_not_to_keep(&answer, theirs, by, &[]);
            assert_eq!(read.len(), usize::from(holds), "{evidence:?}");
            assert_eq!(
                not_to_keep_unread(&answer, theirs, by),
                !holds,
                "{evidence:?}"
            );
        }
    }
}
