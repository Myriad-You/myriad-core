//! Her day and the language she writes in, as the caller gives them to
//! her upkeep: shown as given, refused when they are not a day or a tag;
//! and what an inbox item rests on besides itself.

use super::*;

fn input(today: Option<&str>, language: Option<&str>) -> UpkeepInput {
    UpkeepInput {
        rows: Vec::new(),
        bodies: BTreeMap::new(),
        ledger: Vec::new(),
        audience: "personal:7".into(),
        node: None,
        candidates: Vec::new(),
        inbox: vec![InboxItem {
            id: "message:a1".into(),
            said_by: vec![SaidBy::Partner],
            text: "周四给你图标".into(),
            audience: None,
            refs: Vec::new(),
        }],
        today: today.map(str::to_string),
        language: language.map(str::to_string),
    }
}

#[test]
fn her_day_and_language_are_shown_as_given() {
    let prepared = prepare(input(Some("2026-11-21"), Some("zh-CN"))).unwrap();
    assert_eq!(prepared.today.as_deref(), Some("2026-11-21"));
    assert_eq!(prepared.language.as_deref(), Some("zh-CN"));
    let none = prepare(input(None, None)).unwrap();
    assert_eq!((none.today, none.language), (None, None));
}

#[test]
fn a_day_that_is_not_one_or_a_language_that_is_no_tag_is_refused() {
    for today in ["明天", "2026-11", "2026-11-21T00:00", ""] {
        assert!(prepare(input(Some(today), None)).is_err(), "{today}");
    }
    for language in ["", "zh CN", "中文", &"a".repeat(36)] {
        assert!(prepare(input(None, Some(language))).is_err(), "{language}");
    }
}

fn resting_on(refs: &[&str]) -> UpkeepInput {
    let mut input = input(None, None);
    input.inbox[0].refs = refs.iter().map(|rest| rest.to_string()).collect();
    input
}

#[test]
fn what_an_inbox_item_rests_on_is_taken_when_it_is_a_few_ids() {
    assert!(prepare(resting_on(&["grant:g0123"])).is_ok());
    assert!(
        prepare(resting_on(&[
            "grant:g1",
            "session:s1",
            "record:r1",
            "file:f1"
        ]))
        .is_ok()
    );
    for refs in [
        &["grant:g1", "session:s1", "record:r1", "file:f1", "file:f2"][..],
        &["grant"][..],
        &["grant:g1", "grant:g1"][..],
        &["message:a1"][..],
        &[""][..],
    ] {
        assert!(prepare(resting_on(refs)).is_err(), "{refs:?}");
    }
}
