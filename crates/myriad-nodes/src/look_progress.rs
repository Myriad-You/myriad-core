//! A repository's progress (`progress`, scene `agents`): where its work
//! stands now as she sees it, written on the repository's node as hers
//! (`node.standing`, item [`ITEM`]), and the card of it put before the
//! owner. She says it when she looks at the changes of their sessions
//! ([`crate::session_look`]'s `repos`); its facts are the program's, counted
//! from the sessions of that repository the devices told today, and say no
//! word of any conversation.
//!
//! Pure: what time it is and when today began come in.

use myriad_her_link::now::{Change, CheckKind, CheckState, WaitingOn};
use myriad_persona_log::payloads::{CardFact, CardVersion, CardView};
use serde::{Deserialize, Serialize};

use crate::NodeError;
use crate::body::{BODY_VERSION, Body, Since, Standing};
use crate::cards;
use crate::entries::{RowOut, row_id_from};
use crate::key::node_id;
use crate::limits::{self, cut};
use crate::look_cards::SCENE;
use crate::place::clean;
use crate::session_look::{Told, repo_of};
use crate::sessions::{grant_ref, session_ref};
use crate::tags::{SaidBy, Sync};

/// The one thing on a repository her progress line is about.
pub const ITEM: &str = "progress";
/// The kind of its card.
pub const KIND: &str = "progress";
/// At most this many sessions a line rests on.
const BECAUSE: usize = 16;

/// What a progress card says, in one language. `{n}` is filled in.
struct Words {
    today: &'static str,
    done: &'static str,
    going: &'static str,
    waits: &'static str,
    stuck: &'static str,
    list: &'static str,
    none: &'static str,
    failing: &'static str,
    tests: &'static str,
    passed: &'static str,
    failed: &'static str,
}

const EN: Words = Words {
    today: "Today: {list}",
    done: "{n} finished",
    going: "{n} under way",
    waits: "{n} waiting on you",
    stuck: "{n} stuck",
    list: ", ",
    none: "No session has moved here today",
    failing: "Repeated errors today: {n}",
    tests: "Tests: {list}",
    passed: "{n} passed",
    failed: "{n} failed",
};

const ZH_CN: Words = Words {
    today: "今天：{list}",
    done: "做完 {n} 个",
    going: "在做 {n} 个",
    waits: "在等你 {n} 个",
    stuck: "卡住 {n} 个",
    list: "、",
    none: "今天这里还没有会话动过",
    failing: "今天反复出错 {n} 处",
    tests: "测试：{list}",
    passed: "{n} 个通过",
    failed: "{n} 个没过",
};

const ZH_TW: Words = Words {
    today: "今天：{list}",
    done: "做完 {n} 個",
    going: "在做 {n} 個",
    waits: "在等你 {n} 個",
    stuck: "卡住 {n} 個",
    list: "、",
    none: "今天這裡還沒有工作階段動過",
    failing: "今天反覆出錯 {n} 處",
    tests: "測試：{list}",
    passed: "{n} 個通過",
    failed: "{n} 個沒過",
};

const JA: Words = Words {
    today: "今日：{list}",
    done: "完了 {n}",
    going: "作業中 {n}",
    waits: "あなた待ち {n}",
    stuck: "行き詰まり {n}",
    list: "、",
    none: "今日はまだどのセッションも動いていない",
    failing: "今日くり返したエラー {n} 件",
    tests: "テスト：{list}",
    passed: "成功 {n}",
    failed: "失敗 {n}",
};

const KO: Words = Words {
    today: "오늘: {list}",
    done: "완료 {n}",
    going: "진행 중 {n}",
    waits: "당신을 기다림 {n}",
    stuck: "막힘 {n}",
    list: ", ",
    none: "오늘은 아직 움직인 세션이 없음",
    failing: "오늘 반복된 오류 {n}건",
    tests: "테스트: {list}",
    passed: "통과 {n}",
    failed: "실패 {n}",
};

const FR: Words = Words {
    today: "Aujourd'hui : {list}",
    done: "{n} terminée(s)",
    going: "{n} en cours",
    waits: "{n} vous attend(ent)",
    stuck: "{n} bloquée(s)",
    list: ", ",
    none: "Aucune session n'a bougé ici aujourd'hui",
    failing: "Erreurs répétées aujourd'hui : {n}",
    tests: "Tests : {list}",
    passed: "{n} réussi(s)",
    failed: "{n} en échec",
};

const DE: Words = Words {
    today: "Heute: {list}",
    done: "{n} fertig",
    going: "{n} in Arbeit",
    waits: "{n} warten auf dich",
    stuck: "{n} stecken fest",
    list: ", ",
    none: "Heute hat sich hier noch keine Sitzung bewegt",
    failing: "Wiederholte Fehler heute: {n}",
    tests: "Tests: {list}",
    passed: "{n} bestanden",
    failed: "{n} fehlgeschlagen",
};

/// The words of `language`, as the other cards of the scene choose them.
fn words(language: &str) -> &'static Words {
    let language = language.to_ascii_lowercase().replace('_', "-");
    match language.as_str() {
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => &ZH_TW,
        _ if language.starts_with("zh-hant") => &ZH_TW,
        _ if language == "zh" || language.starts_with("zh-") => &ZH_CN,
        _ if language == "ja" || language.starts_with("ja-") => &JA,
        _ if language == "ko" || language.starts_with("ko-") => &KO,
        _ if language == "fr" || language.starts_with("fr-") => &FR,
        _ if language == "de" || language.starts_with("de-") => &DE,
        _ => &EN,
    }
}

fn fill(template: &str, n: usize) -> String {
    template.replace("{n}", &n.to_string())
}

fn listed(template: &str, parts: &[String], words: &Words) -> String {
    template.replace("{list}", &parts.join(words.list))
}

/// `nodes.look_progress`: her line on one repository (`key` and `name`, as
/// `nodes.look_read`'s `repos` gave them), the owner's sessions as the
/// devices last told them (`told`: those of other repositories are passed
/// by), when the owner's day began (`since`) and now (`at`, both
/// milliseconds since the epoch).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressInput {
    /// The partner's private audience, where the repository's node is.
    pub audience: String,
    pub key: String,
    pub name: String,
    pub text: String,
    /// The row ids her line on it stands on now (the node's standing
    /// `progress`'s `base`), for this one to take their place.
    #[serde(default)]
    pub base: Vec<String>,
    /// The sync of the node as it was made: what is written on it keeps it.
    pub sync: Sync,
    #[serde(default)]
    pub told: Vec<Told>,
    pub since: i64,
    pub at: i64,
    pub language: String,
}

/// Her line as a row of hers on the repository, and the card of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub node: String,
    pub row: RowOut,
    pub card: CardVersion,
}

/// How the sessions of a repository stood today.
#[derive(Debug, Default, PartialEq, Eq)]
struct Today {
    done: usize,
    going: usize,
    waits: usize,
    stuck: usize,
    failures: usize,
    passed: usize,
    failed: usize,
}

fn today(told: &[&Told], since: i64) -> Today {
    let mut today = Today::default();
    for told in told {
        let session = &told.session;
        if session.occurred_at < since {
            continue;
        }
        match session.change {
            Change::Finished => today.done += 1,
            Change::Failed => today.stuck += 1,
            Change::Asked => today.waits += 1,
            _ if session.waiting_on == Some(WaitingOn::Owner) => today.waits += 1,
            _ => today.going += 1,
        }
        // The errors it kept running into today: how often each came
        // before today is not told apart, so each counts once.
        today.failures += session
            .errors
            .iter()
            .filter(|failure| failure.last_at >= since)
            .count();
        // Its latest tests today, as they ended.
        match session
            .checks
            .iter()
            .filter(|check| check.kind == CheckKind::Test && check.at >= since)
            .max_by_key(|check| check.at)
            .map(|check| check.state)
        {
            Some(CheckState::Passed) => today.passed += 1,
            Some(CheckState::Failed) => today.failed += 1,
            _ => {}
        }
    }
    today
}

fn facts(today: &Today, words: &Words, reference: &str) -> Vec<CardFact> {
    let mut texts = Vec::new();
    let counts: Vec<String> = [
        (today.done, words.done),
        (today.going, words.going),
        (today.waits, words.waits),
        (today.stuck, words.stuck),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, template)| fill(template, n))
    .collect();
    if counts.is_empty() {
        texts.push(words.none.to_string());
    } else {
        texts.push(listed(words.today, &counts, words));
    }
    if today.failures > 0 {
        texts.push(fill(words.failing, today.failures));
    }
    let tests: Vec<String> = [(today.passed, words.passed), (today.failed, words.failed)]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, template)| fill(template, n))
        .collect();
    if !tests.is_empty() {
        texts.push(listed(words.tests, &tests, words));
    }
    texts
        .into_iter()
        .map(|text| CardFact {
            text: cut(&text, cards::FACT_CHARS),
            reference: Some(reference.to_string()),
        })
        .collect()
}

/// `nodes.look_progress`: her line on the repository as a `node.standing`
/// row of hers (item [`ITEM`], over `base`), resting on the sessions of it
/// told today (all of them when none moved today); and its card, with the
/// day's counts for facts and her line for its view, asking nothing.
pub fn progress(input: ProgressInput) -> Result<Progress, NodeError> {
    let text = cut(&clean(&input.text, true), limits::TEXT_CHARS);
    if text.trim().is_empty() {
        return Err(NodeError::new("her line on the repository is empty"));
    }
    let node = node_id(&input.audience, &input.key)?;
    let subject = format!("node:{node}");
    let ofs: Vec<&Told> = input
        .told
        .iter()
        .filter(|told| repo_of(told).is_some_and(|repo| repo.key == input.key))
        .collect();
    let moved: Vec<&Told> = ofs
        .iter()
        .copied()
        .filter(|told| told.session.occurred_at >= input.since)
        .collect();
    if ofs.is_empty() {
        return Err(NodeError::new("no session of the repository was told"));
    }
    // Every grant whose sessions of it she was told of, whole: forgetting
    // any of them forgets her line (a grant's rows are found by its name).
    let mut grants: Vec<String> = ofs
        .iter()
        .map(|told| grant_ref(&told.device, &told.session.grant))
        .collect();
    grants.sort();
    grants.dedup();
    if grants.len() > limits::BECAUSE_IDS {
        return Err(NodeError::new(
            "the repository's sessions come under more grants than a row names",
        ));
    }
    let mut sessions: Vec<String> = (if moved.is_empty() { &ofs } else { &moved })
        .iter()
        .map(|told| {
            let session = &told.session;
            session_ref(&told.device, &session.grant, &session.session)
        })
        .collect();
    sessions.sort();
    sessions.dedup();
    sessions.truncate(BECAUSE.min(limits::BECAUSE_IDS - grants.len()));
    let mut because = grants;
    because.extend(sessions);
    because.sort();
    let body = Body::Standing(Standing {
        version: BODY_VERSION,
        sync: input.sync,
        said_by: vec![SaidBy::Her],
        base: input.base,
        item: ITEM.to_string(),
        text: text.clone(),
        since: Some(Since::At(input.at.saturating_mul(1000))),
        state: None,
        because,
        ended: false,
    });
    let body = Body::read("node.standing", &body.to_value())?;
    let row = RowOut {
        id: row_id_from("node.standing", &node, ITEM, &input.at.to_string()),
        kind: "node.standing".to_string(),
        body: body.to_value(),
    };
    let words = words(&input.language);
    let name = clean(&input.name, true);
    let card = CardVersion {
        at: input.at,
        id: cards::card_id(SCENE, KIND, &subject)?,
        kind: KIND.to_string(),
        scene: SCENE.to_string(),
        subject: Some(subject.clone()),
        title: cut(
            if name.is_empty() { &input.key } else { &name },
            cards::TITLE_CHARS,
        ),
        state: None,
        facts: facts(&today(&moved, input.since), words, &subject),
        view: Some(CardView {
            text: cut(&text, cards::VIEW_CHARS),
            because: Vec::new(),
            leans: None,
        }),
        actions: Vec::new(),
    };
    cards::check(&card)?;
    Ok(Progress { node, row, card })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const AT: i64 = 1_760_000_000_000;
    const SINCE: i64 = AT - 3_600_000;

    fn told(session: &str, change: &str, extra: serde_json::Value) -> Told {
        let mut body = json!({
            "grant": "g_0123456789", "session": session, "change": change,
            "occurredAt": AT - 60_000, "tool": "coding-agent",
            "remote": "github.com/demo/sample-repo",
        });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        Told {
            device: "dev_mac".into(),
            session: serde_json::from_value(body).unwrap(),
        }
    }

    fn input(told: Vec<Told>) -> ProgressInput {
        ProgressInput {
            audience: "personal:7".into(),
            key: "repo:github.com/demo/sample-repo".into(),
            name: "sample-repo".into(),
            text: "拆仓库做完一半，剩下的在等你定推不推".into(),
            base: vec!["u_before".into()],
            sync: Sync::Shared,
            told,
            since: SINCE,
            at: AT,
            language: "zh-CN".into(),
        }
    }

    #[test]
    fn the_day_is_counted_from_the_sessions_of_the_repository() {
        let made = progress(input(vec![
            told("s-1", "finished", json!({"checks": [{"kind": "test", "state": "passed", "at": AT - 70_000}]})),
            // Tests that ended yesterday are not today's.
            told("s-2", "asked", json!({"waitingOn": "owner", "errors": [{"sig": "0123456789abcdef", "count": 3, "firstAt": AT - 90_000, "lastAt": AT - 80_000}], "checks": [{"kind": "test", "state": "failed", "at": SINCE - 1}]})),
            // An error it ran into until yesterday is not today's.
            told("s-3", "started", json!({"checks": [{"kind": "test", "state": "failed", "at": AT - 70_000}], "errors": [{"sig": "fedcba9876543210", "count": 5, "firstAt": SINCE - 9_000, "lastAt": SINCE - 5_000}]})),
            // Yesterday's, and another repository's: not counted.
            told("s-4", "failed", json!({"occurredAt": SINCE - 1})),
            told("s-5", "failed", json!({"remote": "github.com/demo/other"})),
        ]))
        .unwrap();
        let facts: Vec<&str> = made
            .card
            .facts
            .iter()
            .map(|fact| fact.text.as_str())
            .collect();
        assert_eq!(
            facts,
            [
                "今天：做完 1 个、在做 1 个、在等你 1 个",
                "今天反复出错 1 处",
                "测试：1 个通过、1 个没过"
            ]
        );
        assert_eq!(made.card.kind, KIND);
        assert!(made.card.actions.is_empty() && !cards::asks(&made.card));
        assert_eq!(
            made.card.view.as_ref().unwrap().text,
            "拆仓库做完一半，剩下的在等你定推不推"
        );
        // Her row on the repository, over the line before it, on the
        // sessions that moved today.
        assert_eq!(made.row.kind, "node.standing");
        assert_eq!(made.row.body["saidBy"], json!(["her"]));
        assert_eq!(made.row.body["item"], json!(ITEM));
        assert_eq!(made.row.body["base"], json!(["u_before"]));
        // The grant whole, so forgetting it forgets her line.
        let because = made.row.body["because"].as_array().unwrap();
        assert_eq!(because.len(), 4);
        assert!(because.contains(&json!(crate::sessions::grant_ref(
            "dev_mac",
            "g_0123456789"
        ))));
        assert_eq!(
            made.card.subject.as_deref(),
            Some(format!("node:{}", made.node).as_str())
        );
    }

    #[test]
    fn a_quiet_day_says_so_and_rests_on_what_was_told() {
        let made = progress(input(vec![told(
            "s-4",
            "finished",
            json!({"occurredAt": SINCE - 1}),
        )]))
        .unwrap();
        assert_eq!(made.card.facts.len(), 1);
        assert_eq!(made.card.facts[0].text, "今天这里还没有会话动过");
        assert_eq!(made.row.body["because"].as_array().unwrap().len(), 2);
        // No session of it at all, or nothing she said: none.
        assert!(progress(input(Vec::new())).is_err());
        let mut empty = input(vec![told("s-1", "finished", json!({}))]);
        empty.text = "  ".into();
        assert!(progress(empty).is_err());
    }

    #[test]
    fn a_folder_of_one_device_is_its_own_repository() {
        let mut local = told("s-1", "finished", json!({"folder": "notes"}));
        local.session.remote = None;
        let key = crate::sessions::folder_key("dev_mac", "notes");
        let mut made = input(vec![local]);
        made.key = key;
        made.name = "notes".into();
        let made = progress(made).unwrap();
        assert_eq!(made.card.title, "notes");
        assert_eq!(made.card.facts[0].text, "今天：做完 1 个");
    }
}
