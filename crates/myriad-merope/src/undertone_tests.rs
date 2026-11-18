use super::*;
use crate::affect::{self, AffectBaseline, DEFAULT_AROUSAL, DEFAULT_MOOD};
use crate::explore::{Compared, Explored};
use crate::serial::Guessed;

fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 1).unwrap() + chrono::Duration::days(n)
}

fn guessed(held: Held, remembered: bool) -> Kept {
    Kept {
        guessed: Some(Guessed {
            said: "the dog is the key".into(),
            held,
            happened: String::new(),
            remembered,
        }),
        ..Kept::default()
    }
}

fn looked_up(answered: &str, surprise: &str, already_known: bool) -> Kept {
    Kept {
        explored: Some(Explored {
            thought: String::new(),
            sure: "fairly".into(),
            sources: vec!["https://example.org".into()],
            compared: Some(Compared {
                surprise: surprise.into(),
                new: String::new(),
                already_known,
                answered: answered.into(),
            }),
        }),
        ..Kept::default()
    }
}

#[test]
fn only_what_landed_hard_is_a_gap() {
    let plain = Kept::default();
    assert_eq!(gap(Some(Reaction::Moved), &plain), Some(MOVED));
    assert_eq!(gap(Some(Reaction::Liked), &plain), None);
    assert_eq!(gap(Some(Reaction::Fine), &plain), None);
    assert_eq!(gap(Some(Reaction::NotForMe), &plain), None);
    assert_eq!(gap(None, &plain), None);
    assert_eq!(gap(Some(Reaction::Fine), &guessed(Held::Yes, false)), Some(GUESS));
    assert_eq!(gap(Some(Reaction::Fine), &guessed(Held::No, false)), Some(-GUESS));
    assert_eq!(gap(None, &guessed(Held::Partly, false)), None);
    assert_eq!(gap(None, &guessed(Held::NotYet, false)), None);
    // What she remembered of a book she knew was no guess.
    assert_eq!(gap(None, &guessed(Held::No, true)), None);
    assert_eq!(gap(None, &looked_up("yes", "some", false)), Some(FOUND_OUT));
    assert_eq!(gap(None, &looked_up("partly", "none", false)), Some(FOUND_OUT));
    assert_eq!(gap(None, &looked_up("yes", "much", false)), Some(-THOUGHT_WRONG));
    assert_eq!(gap(None, &looked_up("yes", "much", true)), Some(FOUND_OUT));
    assert_eq!(gap(None, &looked_up("no", "none", false)), None);
    // Both in one: moved by a part whose guess failed.
    assert_eq!(
        gap(Some(Reaction::Moved), &guessed(Held::No, false)),
        Some(MOVED - GUESS)
    );
    assert_eq!(want_ended(true), CAME_TRUE);
    assert_eq!(want_ended(false), -LET_GO);
}

#[test]
fn a_day_not_lived_is_not_folded_in() {
    assert_eq!(Day::default().activation(), None);
    assert_eq!(Undertone::after(None, day(0), &Day::default()), None);
    let lived = Day {
        things: 4,
        gaps: 1,
        lift: MOVED,
        spent_minutes: 90.0,
        lazed_minutes: 30.0,
    };
    let first = Undertone::after(None, day(0), &lived).unwrap();
    assert_eq!(first.days, 1);
    assert_eq!(Undertone::after(Some(&first), day(1), &Day::default()), Some(first));
    // The same night again, or an older day, changes nothing.
    assert_eq!(Undertone::after(Some(&first), day(0), &lived), Some(first));
    assert!(first.holds());
}

#[test]
fn a_day_is_a_share_not_a_count() {
    let busy = Day {
        things: 10,
        gaps: 0,
        lift: 0.0,
        spent_minutes: 240.0,
        lazed_minutes: 0.0,
    };
    assert_eq!(busy.activation(), Some(0.5));
    let stirred = Day {
        things: 2,
        gaps: 2,
        lift: 2.0 * MOVED,
        spent_minutes: 0.0,
        lazed_minutes: 60.0,
    };
    assert_eq!(stirred.activation(), Some(0.5));
    assert_eq!(stirred.tone(), 1.0);
    let lazy = Day {
        lazed_minutes: 300.0,
        ..Day::default()
    };
    assert_eq!(lazy.activation(), Some(0.0));
    assert_eq!(lazy.tone(), 0.0);
    let odd = Day {
        things: 1,
        gaps: 1,
        lift: f64::NAN,
        spent_minutes: f64::INFINITY,
        lazed_minutes: -5.0,
    };
    assert!(odd.activation().is_some_and(f64::is_finite));
    assert_eq!(odd.tone(), 0.0);
}

/// A day of hers, deterministic: what she spent, lazed, did, and what landed.
fn lived(spent: f64, lazed: f64, things: u32, ups: u32, downs: u32) -> Day {
    Day {
        things,
        gaps: ups + downs,
        lift: f64::from(ups) * MOVED - f64::from(downs) * GUESS,
        spent_minutes: spent,
        lazed_minutes: lazed,
    }
}

fn live(days: &[Day], from: Option<Undertone>, start: i64) -> Vec<Undertone> {
    let mut kept = from;
    let mut out = Vec::new();
    for (n, today) in days.iter().enumerate() {
        kept = Undertone::after(kept.as_ref(), day(start + n as i64), today);
        out.extend(kept);
    }
    out
}

/// Her persona's words weigh less the more days she lives, never cut off.
#[test]
fn her_persona_is_only_where_she_starts() {
    let quiet = affect::QUIET_AROUSAL;
    // A lively life, day after day.
    let days = vec![lived(240.0, 20.0, 12, 6, 0); 60];
    let run = live(&days, None, 0);
    let first = run[0].arousal(quiet);
    let fortnight = run[13].arousal(quiet);
    let two_months = run[59].arousal(quiet);
    assert!(first > quiet && first < fortnight, "{first} {fortnight}");
    assert!(fortnight < two_months, "{fortnight} {two_months}");
    // Not all the way: the persona still weighs, only less.
    assert!(two_months < LIVELY, "{two_months}");
    assert!(two_months > (quiet + LIVELY) / 2.0, "{two_months}");
}

/// Scene 1: a month of only her own days, no one coming. Even a month
/// where almost nothing lands and she lazes more and more stays in bounds,
/// and comes back once her days do.
#[test]
fn a_month_alone_does_not_sink_her() {
    let persona = AffectBaseline::default();
    // An ordinary month: most things only fine, now and then one lands.
    let ordinary: Vec<Day> = (0..30)
        .map(|n| lived(200.0, 60.0, 12, (n % 3 == 0) as u32, (n % 5 == 0) as u32))
        .collect();
    let run = live(&ordinary, None, 0);
    let arousals: Vec<f64> = run.iter().map(|u| u.arousal(persona.arousal)).collect();
    let moods: Vec<f64> = run.iter().map(|u| u.would_move_mood(persona.mood)).collect();
    let last_week = &arousals[23..];
    let spread = last_week.iter().cloned().fold(f64::MIN, f64::max)
        - last_week.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread < 3.0, "settles: {arousals:?}");
    assert!(moods.iter().all(|mood| (mood - DEFAULT_MOOD).abs() <= MOOD_BOUND));

    // The worst month: she does less each day (a low spell narrows her) and
    // nothing lands, until she only lazes.
    let sinking: Vec<Day> = (0..30)
        .map(|n| {
            let spent = (200.0 - 10.0 * n as f64).max(0.0);
            lived(spent, 260.0 - spent, 12u32.saturating_sub(n as u32 / 3), 0, (n < 10) as u32)
        })
        .collect();
    let run = live(&sinking, Some(run[29]), 30);
    let floor = QUIET - BOUND;
    for undertone in &run {
        let arousal = undertone.arousal(persona.arousal);
        assert!(arousal >= floor - 1e-9, "{arousal} below {floor}");
        let mood = undertone.would_move_mood(persona.mood);
        assert!(mood >= DEFAULT_MOOD - MOOD_BOUND - 1e-9, "{mood}");
    }
    let low = run.last().unwrap().arousal(persona.arousal);
    // Then a few ordinary days bring her up again: it is off her own long
    // run, so a turn shows at once.
    let back = live(&ordinary[..5], run.last().copied(), 60);
    let after = back.last().unwrap().arousal(persona.arousal);
    assert!(after > low + 2.0, "{low} → {after}");
}

/// Scene 2: a good week and a bad week of her own, then the same person
/// comes. Her undertone (what she carries to anyone) differs a little and
/// within bounds; how she settles toward that person does not take in
/// anything about them from it, and her valence is not moved at all.
#[test]
fn a_good_week_and_a_bad_week_then_the_same_person() {
    let persona = AffectBaseline::default();
    let ordinary = vec![lived(200.0, 60.0, 12, 1, 0); 21];
    let base = live(&ordinary, None, 0);
    let good = live(&vec![lived(260.0, 10.0, 14, 5, 0); 7], base.last().copied(), 21);
    let bad = live(&vec![lived(80.0, 200.0, 6, 0, 3); 7], base.last().copied(), 21);
    let toward = |undertone: &Undertone| affect::baseline(persona, Some(undertone), 0.0);
    let after_good = toward(good.last().unwrap());
    let after_bad = toward(bad.last().unwrap());
    assert_eq!(after_good.mood, DEFAULT_MOOD);
    assert_eq!(after_bad.mood, DEFAULT_MOOD);
    assert!(after_good.arousal > after_bad.arousal);
    assert!(after_good.arousal - after_bad.arousal <= 2.0 * BOUND + 1e-9);
    // Where she would settle with them, were valence opened, leans the
    // same way and no further than the bound.
    let good_mood = good.last().unwrap().would_move_mood(persona.mood);
    let bad_mood = bad.last().unwrap().would_move_mood(persona.mood);
    assert!(good_mood > bad_mood && good_mood - bad_mood <= 2.0 * MOOD_BOUND + 1e-9);
}

/// Scene 3: weeks of things going well, then someone jabs at her. The jab
/// is theirs: it lowers where she settles toward them (it stings), and her
/// undertone, made of her own days only, cannot take it in.
#[test]
fn smooth_weeks_then_a_jab() {
    let persona = AffectBaseline::default();
    let weeks = live(&vec![lived(240.0, 30.0, 12, 3, 0); 28], None, 0);
    let undertone = weeks.last().unwrap();
    let calm = affect::baseline(persona, Some(undertone), 0.0);
    let stung = affect::baseline(persona, Some(undertone), 8.0);
    assert_eq!(stung.arousal, calm.arousal);
    assert_eq!(stung.mood, calm.mood - 8.0);
    // Nothing of the jab is a day of hers: the next night folds her own day.
    let next = Undertone::after(Some(undertone), day(28), &lived(240.0, 30.0, 12, 3, 0));
    assert_eq!(next.unwrap().days, undertone.days + 1);
}

#[test]
fn without_an_undertone_the_baseline_is_her_persona() {
    let persona = affect::AffectBaseline {
        mood: DEFAULT_MOOD,
        arousal: affect::QUIET_AROUSAL,
    };
    assert_eq!(affect::baseline(persona, None, 0.0), persona);
    assert_eq!(affect::baseline(persona, None, 12.0).mood, DEFAULT_MOOD - 12.0);
    assert_eq!(affect::baseline(persona, None, f64::NAN), persona);
    // One no side could have kept is not read.
    let broken = Undertone {
        day: day(0),
        days: 0,
        recent: Lean {
            activation: 2.0,
            tone: 0.0,
        },
        long: Lean {
            activation: 0.5,
            tone: 0.0,
        },
    };
    assert_eq!(affect::baseline(persona, Some(&broken), 0.0), persona);
    assert_eq!(DEFAULT_AROUSAL, 48.0);
}

/// The record is numbers and a date: nothing anyone said or is called can
/// be in it, and nothing else is taken.
#[test]
fn the_record_is_numbers_and_a_date() {
    let kept = Undertone::after(None, day(0), &lived(100.0, 20.0, 3, 1, 0)).unwrap();
    let json = serde_json::to_value(kept).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "day": "2026-11-01",
            "days": 1,
            "recent": { "activation": kept.recent.activation, "tone": kept.recent.tone },
            "long": { "activation": kept.long.activation, "tone": kept.long.tone },
        })
    );
    let mut with_words = json.clone();
    with_words["note"] = "小林 said …".into();
    assert!(serde_json::from_value::<Undertone>(with_words).is_err());
}
