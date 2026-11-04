//! Her standing acting: what she does with her body while nobody is talking
//! with her, shown on her face instead of written beside it. The director
//! decides it, as it decides all of her acting, from what she is doing on her
//! own; one direction serves everyone watching for as long as she keeps
//! doing that. Keeping that direction and asking again are the platform's.

use super::PerformanceDirective;
use crate::sources::Thing;

/// What she is doing on her own: a thing she does, or lazing of a kind.
#[derive(Debug, Clone, Copy)]
pub enum Doing<'a> {
    Thing(&'a Thing),
    Lazing(&'a str),
}

/// What she is doing as the director hears it, and the `way` it shows
/// (`presence::WAYS`) for when the director cannot be reached. Only what she
/// is doing: why she picked it stays hers, and the minutes in are left out so
/// one direction holds for the whole of it. `together` is someone listening
/// along to the song she is on.
pub fn doing(doing: Doing<'_>, together: bool) -> (&'static str, String) {
    let (way, what) = match doing {
        Doing::Thing(thing) => (
            way_of_thing(thing),
            format!("{} {}", thing.verb(), thing.describe()),
        ),
        Doing::Lazing(kind) => (
            way_of_lazing(kind),
            crate::pace::lazing_line(kind).to_string(),
        ),
    };
    let activity = if together { together_on(&what) } else { what };
    (way, activity)
}

/// What she is doing with someone listening along: the person who has her
/// face open is in it. For an end that has only the words of what she is
/// doing, not the thing itself.
pub fn together_on(what: &str) -> String {
    format!("{what}, and the person who has her face open is listening along with her")
}

/// Whether the director's standing acting is any: a score, or a standing look.
pub fn is_acting(direction: &PerformanceDirective) -> bool {
    !direction.score.is_empty() || direction.plan.baseline.is_some()
}

/// How what she is doing shows on her, by its kind (`presence::WAYS`).
fn way_of_thing(thing: &Thing) -> &'static str {
    match thing {
        Thing::Song { .. } => "listening",
        Thing::Note { .. } | Thing::Chapter { .. } => "reading",
        Thing::Inquiry { .. } => "looking-about",
    }
}

/// How lazing of `kind` shows on her (`presence::WAYS`).
fn way_of_lazing(kind: &str) -> &'static str {
    match kind {
        "wander_the_site" => "looking-about",
        "reread_old_chats" => "reading",
        _ => "daydreaming",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_she_does_shows_by_its_kind_and_a_listener_is_heard_of() {
        let song = Thing::Song {
            id: "1".into(),
            source: "netease".into(),
            name: "夜に駆ける".into(),
            artist: "YOASOBI".into(),
            album: String::new(),
            cover: String::new(),
            duration_ms: 1,
        };
        let (way, alone) = doing(Doing::Thing(&song), false);
        assert_eq!(way, "listening");
        assert!(alone.starts_with(song.verb()) && alone.contains("夜に駆ける"));
        let (_, together) = doing(Doing::Thing(&song), true);
        assert_eq!(
            together,
            format!("{alone}, and the person who has her face open is listening along with her")
        );
        let note = Thing::Note {
            item_id: 3,
            title: "小王子".into(),
        };
        assert_eq!(doing(Doing::Thing(&note), false).0, "reading");
        assert_eq!(
            doing(Doing::Lazing("wander_the_site"), false),
            (
                "looking-about",
                crate::pace::lazing_line("wander_the_site").into()
            )
        );
        assert_eq!(doing(Doing::Lazing("reread_old_chats"), false).0, "reading");
        assert_eq!(doing(Doing::Lazing("nap"), false).0, "daydreaming");
    }
}
