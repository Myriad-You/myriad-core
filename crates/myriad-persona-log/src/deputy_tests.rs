//! An interim host: a side with the server's deputy thinks while the holder
//! is out of its reach and the owner is with it, and the rows it writes
//! then are kept and marked concurrent, one by one as the plan for interim
//! hosts lists them.

use myriad_persona_sync::Rev;
use serde_json::json;

use crate::{
    Author, Grant, Here, Holder, How, LOG_VERSION, Payload, Replicate, Row,
    admit::{AdmitInput, Epochs, admit},
    fold::{FoldInput, fold},
    holder::Core,
    lease::{self, DEPUTY_MAX_MICROS, Deputy, HereInput, Stand},
};

const MINUTE: i64 = 60_000_000;
const HOUR: i64 = 60 * MINUTE;
const OWNER: &str = "personal:o1";
const PARTNER: &str = "partner_0199d5c0b00044444444444444444444";
/// When the deputies below were issued, in milliseconds.
const ISSUED_MS: u64 = 1_760_000_000_000;
const ISSUED: i64 = ISSUED_MS as i64 * 1000;

fn rev(wall_ms: u64, node: &str) -> Rev {
    Rev::new(wall_ms, 0, node).unwrap()
}

/// The site holds epoch 58.
fn site_holds(epoch: u64) -> Grant {
    Grant {
        epoch,
        holder: Some("site".into()),
        issued: rev(ISSUED_MS - 60_000 * epoch, "site"),
        how: How::Handover,
        from: Some("dev_mac".into()),
        through: None,
        moving: None,
    }
}

fn deputy(holder: &str, order: u32) -> Deputy {
    Deputy {
        holder: holder.into(),
        under: 58,
        issued: rev(ISSUED_MS, "site"),
        until: ISSUED + 24 * HOUR,
        order,
    }
}

/// One of her rows by `origin` at epoch 58, written `at` microseconds.
fn her(id: &str, origin: &str, dseq: u64, at: i64, object: &str) -> Row {
    Row {
        version: LOG_VERSION,
        id: id.into(),
        origin: origin.into(),
        author: Author::Her,
        epoch: Some(58),
        dseq: Some(dseq),
        hlc: rev(u64::try_from(at / 1000).unwrap(), origin),
        audience: OWNER.into(),
        subject: None,
        object: object.into(),
        kind: "node.noted".into(),
        payload: Some(Payload {
            reference: format!("blob:{id}"),
            key: None,
        }),
        replicate: Replicate::Shared,
        forgotten: false,
        to: None,
        takes: Vec::new(),
    }
}

struct At {
    deputy: Option<Deputy>,
    now: i64,
    unreachable_since: Option<i64>,
    owner_here_at: Option<i64>,
}

fn here_at(me: &str, at: At) -> Here {
    lease::here(HereInput {
        me: me.into(),
        lease: Some(site_holds(58)),
        now: at.now,
        disconnected_since: at.unreachable_since,
        owner_here_at: at.owner_here_at,
        stop_after: None,
        stopped_at: None,
        deputy: at.deputy,
        host_unreachable_since: at.unreachable_since,
    })
    .unwrap()
}

/// The site out of reach since an hour after the deputy, the owner at the
/// Mac a minute ago.
fn cut_off(now: i64) -> At {
    At {
        deputy: Some(deputy("dev_mac", 1)),
        now,
        unreachable_since: Some(ISSUED + HOUR),
        owner_here_at: Some(now - MINUTE),
    }
}

fn admitted(rows: Vec<Row>, epochs: Epochs) -> (Vec<Row>, Epochs) {
    let out = admit(AdmitInput {
        rows,
        now: ISSUED + 30 * HOUR,
        known: Default::default(),
        members: Default::default(),
        ledger: None,
        epochs,
        holder: Holder {
            core: Core::Server,
            owner: PARTNER.into(),
        },
        peer: PARTNER.into(),
    })
    .unwrap();
    assert!(out.refused.is_empty(), "{:?}", out.refused);
    (out.accepted, out.epochs)
}

// 1. A deputy, the holder out of reach past the stop time, the owner here:
//    interim; the holder back in reach, or the deputy run out: a child.
#[test]
fn a_deputy_thinks_while_the_holder_is_out_of_reach_until_it_comes_back_or_runs_out() {
    let now = ISSUED + HOUR + 3 * MINUTE;
    let until = ISSUED + 24 * HOUR;
    assert_eq!(here_at("dev_mac", cut_off(now)), Here::Interim { until });
    // Back in reach: a child at once.
    let back = At {
        unreachable_since: None,
        ..cut_off(now)
    };
    assert_eq!(here_at("dev_mac", back), Here::Child);
    // Run out: a child, the holder still out of reach.
    assert_eq!(here_at("dev_mac", cut_off(until)), Here::Child);
    assert_eq!(
        here_at("dev_mac", cut_off(until - 1)),
        Here::Interim { until }
    );
    // Out of reach for less than the stop time: still a child.
    assert_eq!(here_at("dev_mac", cut_off(now - 1)), Here::Child);
    // Another side's deputy, or one for an epoch that no longer stands.
    assert_eq!(here_at("dev_win", cut_off(now)), Here::Child);
    let stale = At {
        deputy: Some(Deputy {
            under: 57,
            ..deputy("dev_mac", 1)
        }),
        ..cut_off(now)
    };
    assert_eq!(here_at("dev_mac", stale), Here::Child);
    // It reads as the contract says.
    assert_eq!(
        serde_json::to_value(Here::Interim { until }).unwrap(),
        json!({ "state": "interim", "until": until })
    );
}

// 2. The owner not here: no interim host.
#[test]
fn with_no_owner_here_no_interim_host_starts() {
    let now = ISSUED + 2 * HOUR;
    let gone = |owner_here_at| At {
        owner_here_at,
        ..cut_off(now)
    };
    assert_eq!(here_at("dev_mac", gone(None)), Here::Child);
    assert_eq!(
        here_at("dev_mac", gone(Some(now - 3 * MINUTE))),
        Here::Child
    );
    assert_eq!(
        here_at("dev_mac", gone(Some(now - 3 * MINUTE + 1))),
        Here::Interim {
            until: ISSUED + 24 * HOUR
        }
    );
}

// 3. What an interim host writes: taken, concurrent with the epoch its
//    deputy stands under.
#[test]
fn rows_written_under_a_deputy_are_taken_and_concurrent() {
    let rows = vec![
        her("m1", "dev_mac", 1, ISSUED + 2 * HOUR, "node:a"),
        her("m2", "dev_mac", 2, ISSUED + 3 * HOUR, "node:a"),
    ];
    let mut epochs = Epochs::new();
    epochs.insert(OWNER.into(), [("dev_mac".to_string(), 57)].into());
    let (accepted, _) = admitted(rows, epochs);
    let standings = lease::standings(&accepted, &[site_holds(58)], &[deputy("dev_mac", 1)]);
    assert_eq!(standings.len(), 2);
    for standing in &standings {
        assert_eq!(standing.standing, Stand::Concurrent);
        assert_eq!(standing.with, Some(58));
    }
    // Without the deputy, as before there were any: not the holder's.
    let none = lease::standings(&accepted, &[site_holds(58)], &[]);
    assert!(none.iter().all(|s| s.standing == Stand::NotHolder));
}

// 4. Written after the deputy ran out: not the holder's, as ever.
#[test]
fn rows_written_after_the_deputy_ran_out_are_not_the_holders() {
    let until = ISSUED + 24 * HOUR;
    let grants = [site_holds(58)];
    let deputies = [deputy("dev_mac", 1)];
    let late = her("m3", "dev_mac", 3, until, "node:a");
    assert_eq!(lease::standing(&late, &grants, &deputies), Stand::NotHolder);
    let last = her("m2", "dev_mac", 2, until - 1000, "node:a");
    assert_eq!(
        lease::standing(&last, &grants, &deputies),
        Stand::Concurrent
    );
    // Nor before it was issued, by another side, or at another epoch.
    let early = her("m0", "dev_mac", 1, ISSUED - 1000, "node:a");
    assert_eq!(
        lease::standing(&early, &grants, &deputies),
        Stand::NotHolder
    );
    let other = her("w1", "dev_win", 1, ISSUED + HOUR, "node:a");
    assert_eq!(
        lease::standing(&other, &grants, &deputies),
        Stand::NotHolder
    );
}

// 5. Two interim hosts, out of each other's reach: both sides' rows taken,
//    all concurrent; folded, every object they touch is marked and no row
//    is lost.
#[test]
fn two_interim_hosts_both_keep_every_row_and_mark_what_they_touched() {
    let mac = vec![
        her("m1", "dev_mac", 1, ISSUED + 2 * HOUR, "node:a"),
        her("m2", "dev_mac", 2, ISSUED + 3 * HOUR, "node:b"),
    ];
    let win = vec![
        her("w1", "dev_win", 1, ISSUED + 2 * HOUR + 5, "node:a"),
        her("w2", "dev_win", 2, ISSUED + 4 * HOUR, "node:c"),
    ];
    let site = vec![her("s1", "site", 9, ISSUED + 2 * HOUR + 7, "node:d")];
    let (from_mac, epochs) = admitted(mac, Epochs::new());
    let (from_win, _) = admitted(win, epochs);
    let mut rows = site;
    rows.extend(from_mac);
    rows.extend(from_win);
    let folded = fold(FoldInput {
        rows,
        leases: vec![site_holds(58)],
        deputies: vec![deputy("dev_mac", 1), deputy("dev_win", 2)],
        ledger: Vec::new(),
    })
    .unwrap();
    let concurrent: Vec<&str> = folded
        .standing
        .iter()
        .inspect(|s| assert_eq!((s.standing, s.with), (Stand::Concurrent, Some(58))))
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(concurrent, ["m1", "w1", "m2", "w2"]);
    let objects: Vec<(&str, bool, usize)> = folded
        .objects
        .iter()
        .map(|o| (o.object.as_str(), o.concurrent, o.rows.len()))
        .collect();
    assert_eq!(
        objects,
        [
            ("node:a", true, 2),
            ("node:b", true, 1),
            ("node:c", true, 1),
            ("node:d", false, 1),
        ]
    );
}

// 6. The first back, the server issued a new epoch; the second back later:
//    its rows are still taken (its own epoch did not go down, though the
//    Mac has rows of the new one) and concurrent, and folded, the object
//    is marked and keeps every row.
#[test]
fn the_second_interim_host_back_after_a_new_epoch_still_has_its_rows_taken() {
    // Before the server went out of reach, the Windows side had sent one row
    // under its deputy (it was interim earlier the same day).
    let (first, epochs) = admitted(
        vec![
            her("w1", "dev_win", 1, ISSUED + HOUR, "node:a"),
            her("m1", "dev_mac", 1, ISSUED + 2 * HOUR, "node:a"),
        ],
        Epochs::new(),
    );
    assert_eq!(epochs[OWNER]["dev_win"], 58);
    // The Mac back first: the server hands it 59, and it writes at it.
    let next = Grant {
        epoch: 59,
        holder: Some("dev_mac".into()),
        issued: rev(ISSUED_MS + 5 * 3_600_000, "site"),
        how: How::Handover,
        from: Some("site".into()),
        through: None,
        moving: None,
    };
    let mut at_59 = her("m2", "dev_mac", 2, ISSUED + 6 * HOUR, "node:a");
    at_59.epoch = Some(59);
    let (mac, epochs) = admitted(vec![at_59], epochs);
    // The Windows side back later, with what it wrote meanwhile at 58.
    let (late, epochs) = admitted(
        vec![her("w2", "dev_win", 2, ISSUED + 3 * HOUR, "node:a")],
        epochs,
    );
    assert_eq!(late.len(), 1, "its own epoch did not go down");
    assert_eq!(epochs[OWNER]["dev_win"], 58);
    let mut rows = first;
    rows.extend(mac);
    rows.extend(late);
    let folded = fold(FoldInput {
        rows,
        leases: vec![site_holds(58), next],
        deputies: vec![deputy("dev_mac", 1), deputy("dev_win", 2)],
        ledger: Vec::new(),
    })
    .unwrap();
    let marked: Vec<(&str, Stand, Option<u64>)> = folded
        .standing
        .iter()
        .map(|s| (s.id.as_str(), s.standing, s.with))
        .collect();
    assert_eq!(
        marked,
        [
            ("w1", Stand::Concurrent, Some(58)),
            ("m1", Stand::Concurrent, Some(58)),
            ("w2", Stand::Concurrent, Some(58)),
        ]
    );
    assert_eq!(folded.objects.len(), 1);
    assert!(folded.objects[0].concurrent);
    assert_eq!(folded.objects[0].rows, ["w1", "m1", "w2", "m2"]);
}

#[test]
fn a_deputy_is_for_a_side_under_an_epoch_and_runs_a_day_at_most() {
    assert_eq!(deputy("dev_mac", 1).check(), Ok(()));
    let wrong = [
        Deputy {
            holder: String::new(),
            ..deputy("dev_mac", 1)
        },
        Deputy {
            under: 0,
            ..deputy("dev_mac", 1)
        },
        Deputy {
            order: 0,
            ..deputy("dev_mac", 1)
        },
        Deputy {
            until: ISSUED,
            ..deputy("dev_mac", 1)
        },
        Deputy {
            until: ISSUED + DEPUTY_MAX_MICROS + 1,
            ..deputy("dev_mac", 1)
        },
    ];
    for deputy in wrong {
        assert!(deputy.check().is_err(), "{deputy:?}");
    }
    let longest = Deputy {
        until: ISSUED + DEPUTY_MAX_MICROS,
        ..deputy("dev_mac", 1)
    };
    assert_eq!(longest.check(), Ok(()));
    // One that does not check (a newer server's, say) is as none: a child,
    // never an error that leaves the side not knowing what it is.
    let bad = At {
        deputy: Some(Deputy {
            order: 0,
            ..deputy("dev_mac", 1)
        }),
        ..cut_off(ISSUED + 2 * HOUR)
    };
    assert_eq!(here_at("dev_mac", bad), Here::Child);
}

/// Without a deputy, `here` answers exactly as before deputies: a child
/// stays a child whatever else it is given, a bad stop time included.
#[test]
fn without_a_deputy_a_child_reads_as_before() {
    let here = |stop_after| {
        lease::here(HereInput {
            me: "dev_mac".into(),
            lease: Some(site_holds(58)),
            now: ISSUED + 2 * HOUR,
            disconnected_since: Some(ISSUED),
            owner_here_at: Some(ISSUED + 2 * HOUR),
            stop_after,
            stopped_at: None,
            deputy: None,
            host_unreachable_since: Some(ISSUED),
        })
    };
    assert_eq!(here(None), Ok(Here::Child));
    assert_eq!(here(Some(1)), Ok(Here::Child));
}
