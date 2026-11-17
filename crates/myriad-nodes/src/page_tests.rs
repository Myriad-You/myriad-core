//! A side far behind takes what it lacks a page at a time (`log.pages`):
//! each page at most so many batches, the next one pulled once it holds
//! this one, nothing sent twice, what it holds past a gap (the forgets that
//! go first) never again, the rows it asked again for carried to a page
//! with room, and without a page everything at once, as before.

use std::collections::BTreeSet;

use super::*;
use crate::wire::{PAGE_BATCHES, Sent};

/// A line of a conversation (`segment.line`): its own payload, no node.
fn line(id: &str, origin: &str, dseq: u64) -> Row {
    let mut row = row(id, origin, dseq, T + dseq, MINE, KEY, "segment.line");
    row.object = "segment:s-1".into();
    row
}

/// The server, holding `lines` of its own and a few from another
/// computer, then a forget of a node no side has: it goes first, past the
/// count of a side that lacks the lines before it.
fn far_ahead(lines: u64) -> Side {
    let mut site = Side::new(Core::Server);
    for dseq in 1..=lines {
        site.write(
            line(&format!("l{dseq}"), "site", dseq),
            Some(json!({ "at": T + dseq, "text": format!("第 {dseq} 句") })),
        );
    }
    for dseq in 1..=40 {
        site.write(
            line(&format!("a{dseq}"), "dev_air", dseq),
            Some(json!({ "at": T + dseq, "text": format!("air {dseq}") })),
        );
    }
    site.write(
        row(
            "f1",
            "site",
            lines + 1,
            T + lines + 1,
            MINE,
            "minted:gone",
            "forget",
        ),
        None,
    );
    site
}

/// What `from` sends `to` in answer to the `log.vector` `to` would send
/// now, a page of at most `most` batches.
fn page_to(from: &Side, to: &Side, most: usize, ask: Vec<String>) -> Sent {
    let held = holdings(&to.of(MINE)).unwrap();
    send(SendInput {
        ledger: from.ledger.clone(),
        elsewhere: Vec::new(),
        audience: MINE.into(),
        rows: from.of(MINE),
        bodies: from.bodies.clone(),
        theirs: held.vector,
        ask,
        beyond: held.beyond,
        page: Some(most),
        holder: to.holder.clone(),
        here: from.holder.owner.clone(),
        peer_abi: to.abi,
    })
    .unwrap()
}

#[test]
fn a_side_far_behind_takes_it_a_page_at_a_time_and_ends_holding_all() {
    let site = far_ahead(300);
    let mut mac = Side::new(Core::Pc);
    let all = site.send_to(&mac, MINE).batches.len();
    assert!(all >= 6, "{all}");
    let mut sent = BTreeSet::new();
    let mut pages = 0;
    loop {
        let page = page_to(&site, &mac, 2, Vec::new());
        pages += 1;
        assert!(page.batches.len() <= 2);
        assert!(page.ask_left.is_empty());
        for batch in page.batches {
            assert_eq!(batch.more, None, "the end that sends the page names it");
            for row in &batch.rows {
                assert!(sent.insert(row.id.clone()), "{} went twice", row.id);
            }
            mac.take(batch);
        }
        if !page.more {
            break;
        }
        assert!(pages <= all, "a page brought nothing new");
    }
    assert_eq!(pages, all.div_ceil(2));
    assert_eq!(mac.shared(MINE), site.shared(MINE));
    // Once it holds everything, a page is empty and says nothing more.
    let after = page_to(&site, &mac, 2, Vec::new());
    assert!(after.batches.is_empty() && !after.more);
}

#[test]
fn a_page_that_did_not_arrive_goes_again_as_it_was() {
    let site = far_ahead(300);
    let mut mac = Side::new(Core::Pc);
    let first = page_to(&site, &mac, 2, Vec::new());
    for batch in first.batches {
        mac.take(batch);
    }
    // The link dropped before the second page arrived: on the next link the
    // side asks from what it holds, and gets the same page.
    let second = page_to(&site, &mac, 2, Vec::new());
    assert!(second.more);
    assert_eq!(page_to(&site, &mac, 2, Vec::new()), second);
}

#[test]
fn without_a_page_everything_goes_at_once_as_before() {
    let site = far_ahead(300);
    let mut mac = Side::new(Core::Pc);
    for batch in page_to(&site, &mac, 1, Vec::new()).batches {
        mac.take(batch);
    }
    let held = holdings(&mac.of(MINE)).unwrap();
    assert!(!held.beyond.is_empty(), "the forget is held past a gap");
    let at_once = |beyond| {
        send(SendInput {
            ledger: site.ledger.clone(),
            elsewhere: Vec::new(),
            audience: MINE.into(),
            rows: site.of(MINE),
            bodies: site.bodies.clone(),
            theirs: held.vector.clone(),
            ask: Vec::new(),
            beyond,
            page: None,
            holder: mac.holder.clone(),
            here: site.holder.owner.clone(),
            peer_abi: mac.abi,
        })
        .unwrap()
    };
    // What it holds past a gap is weighed only with a page.
    let sent = at_once(held.beyond.clone());
    assert_eq!(sent, at_once(BTreeMap::new()));
    assert!(!sent.more && sent.ask_left.is_empty());
    let written = serde_json::to_value(&sent).unwrap();
    assert!(written.get("more").is_none() && written.get("askLeft").is_none());
    for batch in written["batches"].as_array().unwrap() {
        assert!(batch.get("more").is_none());
    }
    // A page of none, or of more than the link lets, is refused.
    for most in [0, PAGE_BATCHES + 1] {
        let held = holdings(&mac.of(MINE)).unwrap();
        assert!(
            send(SendInput {
                ledger: site.ledger.clone(),
                elsewhere: Vec::new(),
                audience: MINE.into(),
                rows: site.of(MINE),
                bodies: site.bodies.clone(),
                theirs: held.vector,
                ask: Vec::new(),
                beyond: held.beyond,
                page: Some(most),
                holder: mac.holder.clone(),
                here: site.holder.owner.clone(),
                peer_abi: mac.abi,
            })
            .is_err()
        );
    }
}

#[test]
fn rows_asked_again_for_wait_for_a_page_with_room_and_then_come() {
    let mut site = far_ahead(120);
    let mut mac = Side::new(Core::Pc);
    // Everything reaches the Mac but two bodies: it asks for them again.
    let mut asked = Vec::new();
    for mut batch in site.send_to(&mac, MINE).batches {
        batch
            .payloads
            .retain(|one| one.reference != "doc:l5" && one.reference != "doc:l6");
        asked.extend(mac.take(batch).ask);
    }
    assert_eq!(asked, ["l5", "l6"]);
    // The server wrote a good deal since: a page fills with that first.
    for dseq in 122..=400 {
        site.write(
            line(&format!("l{dseq}"), "site", dseq),
            Some(json!({ "at": T + dseq, "text": format!("第 {dseq} 句") })),
        );
    }
    let held: Vec<Row> = mac
        .rows
        .iter()
        .filter(|row| asked.contains(&row.id))
        .cloned()
        .collect();
    let mut ask = asked.clone();
    let mut pages = 0;
    loop {
        let page = page_to(&site, &mac, 1, ask);
        pages += 1;
        assert!(page.batches.len() <= 1);
        for batch in page.batches {
            mac.take_held(batch, held.clone());
        }
        ask = page.ask_left;
        if !page.more {
            break;
        }
        assert!(pages <= 20);
    }
    assert!(pages > 1, "the asked rows waited for room");
    assert!(ask.is_empty());
    assert!(mac.bodies.contains_key("l5") && mac.bodies.contains_key("l6"));
    assert_eq!(mac.shared(MINE), site.shared(MINE));
}
