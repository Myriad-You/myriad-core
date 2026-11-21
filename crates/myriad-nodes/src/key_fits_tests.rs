//! Which keys a node may have in which audience: no group's outside its
//! own audience, no person's in hers.

use super::*;

#[test]
fn a_person_lives_anywhere_but_her_own() {
    assert!(Key::parse("person:7").unwrap().fits("personal:7"));
    // Someone the partner talks about is a node of their talk.
    assert!(Key::parse("person:9").unwrap().fits("personal:7"));
    assert!(
        Key::parse("person:9")
            .unwrap()
            .fits("group:telegram:-100123")
    );
    assert!(!Key::parse("person:7").unwrap().fits("own:"));
}

#[test]
fn a_group_lives_only_in_its_own_audience() {
    let group = Key::parse("group:telegram:-100123").unwrap();
    assert!(group.fits("group:telegram:-100123"));
    assert!(!group.fits("group:telegram:-100999"));
    assert!(!group.fits("personal:7"));
    assert!(!group.fits("own:"));
    assert!(node_id("personal:7", "group:telegram:-100123").is_err());
}

#[test]
fn her_and_things_live_anywhere() {
    for audience in ["personal:7", "own:", "group:telegram:-100123"] {
        for key in [
            "her",
            "repo:github.com/me/repo",
            "minted:t0123456789abcdef01234567",
        ] {
            assert!(Key::parse(key).unwrap().fits(audience), "{key} {audience}");
        }
    }
}
