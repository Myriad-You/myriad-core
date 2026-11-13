use serde_json::{Value, json};

use super::*;

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn site() -> Sync {
    Sync::Local("site".into())
}

#[test]
fn a_fact_on_a_node_of_its_own_still_reaches_the_node_it_changes() {
    // 「新工作」 is the fact's own node; what it brought to mind is on 「工作」.
    let nodes = nodes_it_may_change(
        &names(&["新工作"]),
        &[names(&["工作", "加班"]), names(&[]), names(&["工作"])],
    );
    assert_eq!(nodes, names(&["新工作", "工作", "加班"]));
    assert!(nodes_it_may_change(&[], &[]).is_empty());
}

#[test]
fn what_a_fact_starts_stands_where_it_was_asked_when_it_may() {
    let own = names(&["公司食堂"]);
    let may = names(&["公司食堂", "饮食口味"]);
    assert_eq!(where_it_starts("饮食口味", &may, &own), Some("饮食口味"));
    assert_eq!(where_it_starts("习惯", &may, &own), Some("公司食堂"));
    assert_eq!(where_it_starts("习惯", &may, &[]), None);
}

#[test]
fn whose_life_an_item_is_reads_back_from_its_id() {
    let them = item_id("them", "n1", "m1", 0);
    assert!(them.starts_with("them-") && them.len() == 15);
    assert_eq!(whose_of(&them), Some(("them".into(), "them".into())));
    let key = whose_key("苏禾").unwrap();
    let hers = item_id(&key, "n1", "m1", 1);
    assert_eq!(whose_of(&hers), Some((key, "苏禾".into())));
    let long = item_id(&whose_key("一二三四五六七八九十").unwrap(), "n", "m", 0);
    assert!(long.len() <= 64);
    assert_eq!(whose_of(&long).unwrap().1, "一二三四五六七八");
    assert_eq!(whose_key("Them"), Some("them".into()));
    // Them, said with whom: still theirs; a name that begins so is a name.
    assert_eq!(whose_key("them，与朋友"), Some("them".into()));
    assert_eq!(whose_key("them and Mei"), Some("them".into()));
    assert_eq!(whose_key("them（和妈妈）"), Some("them".into()));
    assert_ne!(whose_key("Themis"), Some("them".into()));
    assert_ne!(whose_key("them们"), Some("them".into()));
    assert_ne!(whose_key("them's mom"), Some("them".into()));
    assert_ne!(whose_key("them’s 妈妈"), Some("them".into()));
    assert_eq!(whose_key("  "), None);
    // A rule's item is not one of hers.
    assert_eq!(whose_of("now"), None);
    assert_eq!(whose_of("p-zz"), None);
    assert!(is_day("2026") && is_day("2026-10-18") && !is_day("2026-1") && !is_day("上周"));
}

/// A folded node holding `items` (item, text, ended) of 「现在」.
fn folded(items: &[(&str, &str, bool)]) -> Value {
    let mut standing = serde_json::Map::new();
    for (item, text, ended) in items {
        let mut value = json!({"text": text});
        if *ended {
            value["ended"] = json!(true);
        }
        standing.insert(
            item.to_string(),
            json!({"value": value, "base": [format!("u-{item}")]}),
        );
    }
    json!({"standing": standing, "entries": []})
}

fn node(id: &str, name: &str, folded: Value) -> NodeIn {
    NodeIn {
        id: id.into(),
        name: Some(name.into()),
        kind: Some("topic".into()),
        folded,
    }
}

fn fact(memory: &str, text: &str, on: &[&str]) -> FactIn {
    FactIn {
        memory: memory.into(),
        text: text.into(),
        happened: None,
        learned: Some("2026-11-12".into()),
        replaced: false,
        on: names(on),
    }
}

fn read(step: &StepInput, answer: Value) -> StepRead {
    step_read(StepReadInput {
        output: answer.to_string(),
        sync: site(),
        fact: step.fact.clone(),
        known: step.known.clone(),
        nodes: step.nodes.clone(),
    })
    .unwrap()
}

#[test]
fn a_change_is_written_where_the_item_stands_and_only_of_whose_life_it_is() {
    let job = item_id("them", "work", "m0", 0);
    let mom = item_id(&whose_key("妈妈").unwrap(), "mom", "m0", 0);
    let step = StepInput {
        fact: fact("m1", "我换工作了", &["own"]),
        known: vec![
            KnownIn {
                text: "在会展公司做策划".into(),
                on: names(&["work"]),
            },
            KnownIn {
                text: "妈妈每周日打电话".into(),
                on: names(&["mom", "food"]),
            },
        ],
        nodes: vec![
            node("own", "新工作", folded(&[])),
            node("work", "工作", folded(&[(&job, "在会展公司做策划", false)])),
            node("mom", "妈妈", folded(&[(&mom, "妈妈每周日打电话", false)])),
            node("food", "饮食口味", folded(&[])),
            node("far", "不相干", folded(&[])),
        ],
    };
    let asked = step_prepare(step.clone()).unwrap();
    assert!(!asked.skip);
    let shown: Value = serde_json::from_str(&asked.input).unwrap();
    assert_eq!(shown["list"].as_array().unwrap().len(), 2);
    assert_eq!(shown["list"][1]["whose"], "妈妈");
    assert_eq!(
        shown["topics"],
        json!(["新工作", "工作", "妈妈", "饮食口味"])
    );
    // Their own fact: their item changes on its node; 妈妈's is left.
    let got = read(
        &step,
        json!({"whose": "them", "set": [{"item": 1, "text": "换到游戏公司做策划"}],
            "end": [2], "add": [{"topic": "饮食口味", "text": "午饭在食堂"},
                                {"topic": "习惯", "text": "下班走路回家"}]}),
    );
    assert_eq!(
        got.left,
        vec![Left {
            item: mom.clone(),
            item_whose: whose_key("妈妈").unwrap(),
            fact_whose: "them".into(),
        }]
    );
    assert_eq!(got.rows.len(), 3);
    assert_eq!(got.rows[0].node, "work");
    assert_eq!(got.rows[0].body["item"], json!(job));
    assert_eq!(got.rows[0].body["base"], json!([format!("u-{job}")]));
    assert_eq!(got.rows[1].node, "food");
    assert_eq!(got.rows[2].node, "own");
    assert!(
        got.rows[1].body["item"]
            .as_str()
            .unwrap()
            .starts_with("them-")
    );
    // Hers, kept as the end tags it, checked by the core's rules, since the
    // day she was told when nothing says when it happened.
    for row in &got.rows {
        assert_eq!(row.kind, KIND);
        assert_eq!(row.body["sync"], json!({"local": "site"}));
        assert_eq!(row.body["saidBy"], json!(["her"]));
        assert_eq!(row.body["since"], json!("2026-11-12"));
        assert_eq!(row.body["because"], json!(["memory:m1"]));
        Body::read(&row.kind, &row.body).unwrap();
    }
    // Someone else's fact never ends their own item.
    let got = read(
        &step,
        json!({"whose": "男朋友", "set": [{"item": 1, "text": "去肯尼亚驻场"}], "end": [1], "add": []}),
    );
    assert!(got.rows.is_empty());
    assert_eq!(got.left.len(), 2);
    // An end stands on the fact's own words.
    let got = read(
        &step,
        json!({"whose": "妈妈", "set": [], "end": [2], "add": []}),
    );
    assert_eq!(got.rows[0].body["text"], json!("我换工作了"));
    assert_eq!(got.rows[0].body["ended"], json!(true));
    // No whose: nothing; no answer at all: unreadable.
    assert_eq!(
        read(&step, json!({"set": [{"item": 1, "text": "x"}]})),
        StepRead::default()
    );
    let got = step_read(StepReadInput {
        output: "not json".into(),
        sync: site(),
        fact: step.fact.clone(),
        known: step.known.clone(),
        nodes: step.nodes.clone(),
    })
    .unwrap();
    assert!(got.unreadable && got.rows.is_empty());
}

#[test]
fn a_change_the_rules_refuse_is_set_aside_and_the_rest_are_written() {
    let run = item_id("them", "run", "m0", 0);
    let step = StepInput {
        // A control character in the fact's words: an end in them is no row.
        fact: fact("m1", "膝盖伤了\u{7}不跑了", &["run"]),
        known: Vec::new(),
        nodes: vec![node("run", "跑步", folded(&[(&run, "每周三夜跑", false)]))],
    };
    let got = read(
        &step,
        json!({"whose": "them", "set": [], "end": [1],
            "add": [{"topic": "跑步", "text": "三个月内只散步"}]}),
    );
    assert_eq!(got.rows.len(), 1);
    assert_eq!(got.rows[0].body["text"], json!("三个月内只散步"));
    assert_eq!(got.set_aside, 1);
    // A name too long for an item's id starts nothing.
    let long = "👩‍👩‍👧‍👦👩‍👩‍👧‍👦👩‍👩‍👧‍👦";
    let got = read(
        &step,
        json!({"whose": long, "set": [], "end": [], "add": [{"topic": "跑步", "text": "x"}]}),
    );
    assert!(got.rows.is_empty());
    assert_eq!(got.set_aside, 1);
}

#[test]
fn since_is_when_it_happened_else_the_day_she_was_told() {
    let step = StepInput {
        fact: FactIn {
            happened: Some("2026-10".into()),
            ..fact("m1", "十月开始坐地铁", &["commute"])
        },
        known: Vec::new(),
        nodes: vec![node("commute", "通勤", folded(&[]))],
    };
    let got = read(
        &step,
        json!({"whose": "them", "set": [], "end": [], "add": [{"topic": "通勤", "text": "坐地铁上班"}]}),
    );
    assert_eq!(got.rows[0].body["since"], json!("2026-10"));
    let none = StepInput {
        fact: FactIn {
            learned: None,
            ..step.fact.clone()
        },
        ..step.clone()
    };
    let none = StepInput {
        fact: FactIn {
            happened: None,
            ..none.fact
        },
        ..none
    };
    assert!(
        read(
            &none,
            json!({"whose": "them", "set": [], "end": [], "add": [{"topic": "通勤", "text": "坐地铁上班"}]})
        )
        .rows[0]
            .body
            .get("since")
            .is_none()
    );
}

#[test]
fn what_replaced_something_stands_on_its_own_node_when_nothing_else_does() {
    let step = StepInput {
        fact: FactIn {
            replaced: true,
            ..fact("m1", "膝盖伤了，不跑了", &["own"])
        },
        known: Vec::new(),
        nodes: vec![node("own", "跑步", folded(&[]))],
    };
    let empty = json!({"whose": "them", "set": [], "end": [], "add": []});
    let got = read(&step, empty.clone());
    assert_eq!(got.rows.len(), 1);
    assert_eq!(got.rows[0].node, "own");
    assert_eq!(got.rows[0].body["text"], json!("膝盖伤了，不跑了"));
    let not_replaced = StepInput {
        fact: fact("m1", "f", &["own"]),
        ..step
    };
    assert!(read(&not_replaced, empty).rows.is_empty());
}

#[test]
fn a_fact_on_no_node_with_nothing_brought_to_mind_asks_nothing() {
    let step = StepInput {
        fact: fact("m1", "今天下雨", &[]),
        known: vec![KnownIn {
            text: "x".into(),
            on: Vec::new(),
        }],
        nodes: Vec::new(),
    };
    assert!(step_prepare(step).unwrap().skip);
}

#[test]
fn a_rule_s_now_is_not_shown_and_an_ended_item_is_not_either() {
    let them = item_id("them", "n", "m", 0);
    let shown = shown_on(
        "n",
        &folded(&[
            ("now", "working", false),
            (&them, "夜跑", false),
            (&item_id("them", "n", "m", 1), "旧的", true),
        ]),
    );
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].item, them);
}

#[test]
fn building_is_due_while_none_of_hers_stands_whatever_a_rule_wrote() {
    let rule = || StandingRowIn {
        author: "rule".into(),
        item: None,
    };
    let hers = |item: Option<&str>| StandingRowIn {
        author: "her".into(),
        item: item.map(str::to_string),
    };
    let due = |facts, standing| build_due(&BuildDueInput { facts, standing });
    assert!(due(1, vec![]));
    assert!(due(2, vec![rule(), rule()]));
    assert!(!due(0, vec![]));
    assert!(!due(1, vec![rule(), hers(None)]));
    // Her line on a repository is no one's life: it does not count.
    assert!(due(1, vec![hers(Some(crate::look_progress::ITEM))]));
    assert!(!due(
        1,
        vec![
            hers(Some(crate::look_progress::ITEM)),
            hers(Some("them-0123456789"))
        ]
    ));
}

fn entry(text: &str, memories: &[&str], happened: Option<&str>) -> Value {
    let mut entry = json!({
        "text": text,
        "refs": memories.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
        "at": 1794273967436000_i64,
    });
    if let Some(from) = happened {
        entry["happened"] = json!({"from": from});
    }
    entry
}

#[test]
fn what_stands_on_a_node_rests_on_the_memories_it_names_since_their_latest_day() {
    let on_node = json!({"standing": {}, "entries": [
        entry("每周三晚上夜跑", &["m1"], Some("2026-08-21")),
        entry("在跑团认识了阿Ken", &["m2", "m3"], None),
        entry("膝盖有点疼", &["m4"], Some("2026-10-19")),
        entry("已经忘了的", &["gone"], None),
        entry("  ", &["m5"], None),
    ]});
    let input = BuildInput {
        node: NodeIn {
            id: "run".into(),
            name: Some("跑步".into()),
            kind: Some("topic".into()),
            folded: on_node,
        },
        held: names(&["m1", "m2", "m3", "m4", "m5"]),
        learned: [("m2", "2026-07-02"), ("m3", "2026-07-05")]
            .into_iter()
            .map(|(id, day)| (id.to_string(), day.to_string()))
            .collect(),
    };
    let asked = build_prepare(input.clone()).unwrap();
    assert!(!asked.skip);
    let shown: Value = serde_json::from_str(&asked.input).unwrap();
    assert_eq!(shown["topic"], "跑步");
    // Each with when it happened, else when she learned it; not when it was
    // put on the node; what she no longer holds and empty words are left.
    let days: Vec<&str> = shown["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["day"].as_str().unwrap())
        .collect();
    assert_eq!(days, ["2026-08-21", "2026-07-02", "2026-10-19"]);
    let got = build_read(BuildReadInput {
        output: json!({"items": [
            {"whose": "them", "text": "每周三、周五晚上夜跑五公里", "because": [1, 3]},
            {"whose": "阿Ken", "text": "在跑团带新人", "because": [2]},
            {"whose": "them", "text": "  ", "because": [1]},
            {"whose": "", "text": "没人的", "because": [1]},
            {"whose": "them", "text": "没有依据", "because": [9]},
        ]})
        .to_string(),
        sync: site(),
        node: input.node.clone(),
        held: input.held.clone(),
        learned: input.learned.clone(),
    })
    .unwrap();
    assert_eq!(got.rows.len(), 2);
    assert_eq!(
        got.rows[0].body["because"],
        json!(["memory:m1", "memory:m4"])
    );
    assert_eq!(got.rows[0].body["since"], json!("2026-10-19"));
    assert_eq!(got.rows[1].body["since"], json!("2026-07-02"));
    assert_eq!(
        whose_of(got.rows[1].body["item"].as_str().unwrap())
            .unwrap()
            .1,
        "阿Ken"
    );
    for row in &got.rows {
        assert_eq!(row.node, "run");
        assert_eq!(row.body["base"], json!([]));
        Body::read(&row.kind, &row.body).unwrap();
    }
    // A node with something of hers on it, or not a topic or a person, or
    // with nothing she holds: nothing to ask.
    let hers = BuildInput {
        node: NodeIn {
            folded: folded(&[(&item_id("them", "run", "m1", 0), "夜跑", false)]),
            ..input.node.clone()
        },
        ..input.clone()
    };
    assert!(build_prepare(hers).unwrap().skip);
    let repo = BuildInput {
        node: NodeIn {
            kind: Some("repo".into()),
            ..input.node.clone()
        },
        ..input.clone()
    };
    assert!(build_prepare(repo).unwrap().skip);
    let nothing = BuildInput {
        held: Vec::new(),
        ..input
    };
    assert!(build_prepare(nothing).unwrap().skip);
}
