//! The backend's grant.rs and dispatch.rs cases, with its integer user
//! (`SYSTEM_USER_ID` is 0, a person is above 0) as the actor.

use super::*;

const HEARTBEAT: i32 = 0;

fn actor(id: i32) -> Actor<i32> {
    Actor {
        id,
        personal: id > 0,
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn grant(id: i32, allowed: &[&str], revoked: bool) -> AutonomyGrant<i32> {
    AutonomyGrant {
        actor: id,
        allowed: names(allowed),
        revoked,
    }
}

fn allows(list: &[&str]) -> Verdict {
    Verdict::AllowPersonalWork {
        granted: names(list),
    }
}

#[test]
fn heartbeat_is_never_a_personal_grant() {
    let heartbeat = grant(HEARTBEAT, &["calendar:read"], false);
    let current = names(&["calendar:read"]);
    assert_eq!(
        evaluate(&actor(HEARTBEAT), Some(&heartbeat), &current),
        Verdict::RequireUserReview
    );
    assert_eq!(
        evaluate(&actor(7), Some(&heartbeat), &current),
        Verdict::RequireUserReview
    );
}

#[test]
fn missing_or_revoked_grant_cannot_skip_user_review() {
    let current = names(&["calendar:read"]);
    assert_eq!(
        evaluate(&actor(7), None, &current),
        Verdict::RequireUserReview
    );
    let revoked = grant(7, &["calendar:read"], true);
    assert_eq!(
        evaluate(&actor(7), Some(&revoked), &current),
        Verdict::RequireUserReview
    );
}

#[test]
fn a_grant_is_refiltered_by_what_is_granted_now() {
    let stored = grant(7, &["calendar:read", "mcp:execute", "mail:send"], false);
    assert_eq!(
        evaluate(
            &actor(7),
            Some(&stored),
            &names(&["calendar:read", "mail:send"])
        ),
        allows(&["calendar:read", "mail:send"])
    );
    assert_eq!(
        evaluate(&actor(7), Some(&stored), &[]),
        Verdict::RequireUserReview
    );
}

#[test]
fn review_after_revoke_or_drop_cannot_enter_work() {
    let current = names(&["calendar:read"]);
    let revoked = grant(7, &["calendar:read"], true);
    let live = grant(7, &["calendar:read", "mcp:execute"], false);
    assert!(!may_enter_work(
        &actor(7),
        Some(&revoked),
        &current,
        Source::Autonomy
    ));
    assert!(!may_enter_work(
        &actor(7),
        Some(&live),
        &[],
        Source::Autonomy
    ));
    assert!(!may_enter_work(
        &actor(HEARTBEAT),
        Some(&grant(HEARTBEAT, &["calendar:read"], false)),
        &current,
        Source::User
    ));
    assert!(may_enter_work(&actor(7), None, &current, Source::User));
    assert!(may_enter_work(
        &actor(7),
        Some(&live),
        &current,
        Source::Autonomy
    ));
    assert!(may_enter_work(
        &actor(7),
        Some(&revoked),
        &current,
        Source::User
    ));
}

#[test]
fn an_empty_request_keeps_candidates_and_drops_admin_only() {
    let candidates = names(&["http:fetch", "scheduler:write", "calendar:read"]);
    let admin_current = names(&[
        "system:admin",
        "phantasi:admin",
        "http:fetch",
        "scheduler:write",
    ]);
    let prepared = prepare(&actor(7), &[], &admin_current, &candidates).unwrap();
    assert_eq!(prepared.allowed, names(&["http:fetch", "scheduler:write"]));

    let explicit = prepare(
        &actor(7),
        &names(&["system:admin"]),
        &admin_current,
        &candidates,
    )
    .unwrap();
    assert_eq!(explicit.allowed, names(&["system:admin"]));

    let regular = names(&["http:fetch", "scheduler:write"]);
    let from_empty = prepare(&actor(7), &[], &regular, &candidates).unwrap();
    let from_explicit = prepare(&actor(7), &regular, &regular, &candidates).unwrap();
    assert_eq!(from_empty.allowed, from_explicit.allowed);
    assert_eq!(from_empty.allowed, regular);
}

#[test]
fn grant_revoke_and_drop_go_through_the_work_entry_check() {
    let current = names(&["calendar:read", "mail:send"]);
    let live = prepare(&actor(7), &names(&["calendar:read"]), &current, &[]).unwrap();
    assert!(!live.revoked);
    assert_eq!(live.allowed, names(&["calendar:read"]));
    assert!(may_enter_work(
        &actor(7),
        Some(&live),
        &current,
        Source::Autonomy
    ));

    let revoked = revoke(&live, true).unwrap();
    assert!(revoked.revoked);
    assert_eq!(revoked.allowed, live.allowed);
    assert!(!may_enter_work(
        &actor(7),
        Some(&revoked),
        &current,
        Source::Autonomy
    ));
    assert!(!may_enter_work(
        &actor(7),
        Some(&live),
        &[],
        Source::Autonomy
    ));

    assert_eq!(
        prepare(&actor(HEARTBEAT), &names(&["calendar:read"]), &current, &[]),
        Err(WriteRefusal::HeartbeatIdentity)
    );
    assert_eq!(
        revoke(&grant(HEARTBEAT, &["calendar:read"], false), false),
        Err(WriteRefusal::HeartbeatIdentity)
    );
    assert_eq!(
        prepare(&actor(7), &names(&["mcp:execute"]), &current, &[]),
        Err(WriteRefusal::EmptyAfterFilter)
    );
}

#[test]
fn effective_is_granted_within_the_cap() {
    let current = names(&["calendar:read", "mail:send"]);
    let cap = names(&["calendar:read"]);
    assert_eq!(effective(&current, Some(&cap)), names(&["calendar:read"]));
    assert_eq!(effective(&current, None), current);
    assert_eq!(effective(&current, Some(&[])), Vec::<String>::new());
}

#[test]
fn cap_still_allows_fails_after_revoke_or_drop() {
    let live = grant(7, &["calendar:read"], false);
    let current = names(&["calendar:read", "mail:send"]);
    let cap = names(&["calendar:read"]);
    assert!(cap_still_allows(
        &actor(7),
        Some(&live),
        &current,
        Some(&cap)
    ));
    assert!(cap_still_allows(&actor(7), Some(&live), &current, None));
    let revoked = grant(7, &["calendar:read"], true);
    assert!(!cap_still_allows(
        &actor(7),
        Some(&revoked),
        &current,
        Some(&cap)
    ));
    assert!(!cap_still_allows(&actor(7), Some(&live), &[], Some(&cap)));
    assert!(!cap_still_allows(
        &actor(7),
        Some(&live),
        &names(&["mail:send"]),
        Some(&cap)
    ));
}

#[test]
fn execute_time_revoke_or_drop_fails_closed() {
    let live = grant(7, &["calendar:read"], false);
    let cap = names(&["calendar:read"]);
    let current = names(&["calendar:read", "mail:send"]);
    let read = names(&["calendar:read"]);
    let send = names(&["mail:send"]);
    assert_eq!(
        check(&actor(7), Some(&live), &current, Some(&cap), &read),
        Check::Allowed
    );
    let revoked = grant(7, &["calendar:read"], true);
    assert_eq!(
        check(&actor(7), Some(&revoked), &current, Some(&cap), &read),
        Check::AutonomyWithdrawn
    );
    assert_eq!(
        check(&actor(7), Some(&live), &send, Some(&cap), &read),
        Check::AutonomyWithdrawn
    );
    assert_eq!(
        check(&actor(7), Some(&live), &current, Some(&cap), &send),
        Check::Missing {
            missing: send.clone()
        }
    );
    assert_eq!(
        check(&actor(7), None, &current, None, &send),
        Check::Allowed
    );
    assert_eq!(
        check(&actor(7), None, &read, None, &send),
        Check::Missing {
            missing: send.clone()
        }
    );
    // Nothing required: only the autonomy question is left.
    assert_eq!(
        check(&actor(7), Some(&live), &current, Some(&cap), &[]),
        Check::Allowed
    );
}

#[test]
fn only_autonomy_accepted_rows_with_a_live_grant_are_claimed() {
    let live = grant(7, &["calendar:read", "mail:send"], false);
    let current = names(&["calendar:read"]);
    assert_eq!(
        claim(&actor(7), Source::Autonomy, Some(&live), &current),
        Some(names(&["calendar:read"]))
    );
    assert_eq!(claim(&actor(7), Source::User, Some(&live), &current), None);
    assert_eq!(claim(&actor(7), Source::Autonomy, None, &current), None);
    assert_eq!(claim(&actor(7), Source::Autonomy, Some(&live), &[]), None);
    assert_eq!(
        claim(
            &actor(HEARTBEAT),
            Source::Autonomy,
            Some(&grant(HEARTBEAT, &["calendar:read"], false)),
            &current
        ),
        None
    );
}

#[test]
fn granted_is_where_the_three_layers_meet() {
    let layers = Layers {
        declared: names(&["a", "b", "c", "d"]),
        approved: names(&["c", "a", "b"]),
        runtime_allowed: names(&["b", "c", "x"]),
    };
    assert_eq!(granted(&layers), names(&["c", "b"]));
    let nothing_declared = Layers {
        declared: vec![],
        approved: vec![],
        runtime_allowed: names(&["a"]),
    };
    assert_eq!(granted(&nothing_declared), Vec::<String>::new());
    let not_approved = Layers {
        declared: names(&["a"]),
        approved: vec![],
        runtime_allowed: names(&["a"]),
    };
    assert_eq!(granted(&not_approved), Vec::<String>::new());
    let undeclared = Layers {
        declared: names(&["a"]),
        approved: names(&["a", "b"]),
        runtime_allowed: names(&["a", "b"]),
    };
    assert_eq!(
        granted(&undeclared),
        names(&["a"]),
        "the undeclared is dropped"
    );
    let none_declared = Layers {
        declared: vec![],
        approved: names(&["a", "b"]),
        runtime_allowed: names(&["a", "b"]),
    };
    assert_eq!(granted(&none_declared), Vec::<String>::new());
}
