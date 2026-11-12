//! When what they told of happened, and where it stands now: the two marks
//! a fact she keeps for good can carry, so that asked "how many last
//! month" or "which are still to pick up", code counts them and she does
//! not have to work dates out of a few dozen lines of text.
//!
//! - **When it happened** ([`Happened`]) is not when they said it (a row's
//!   clock says that): a day, a month or a year, or a stretch from one to
//!   another, and whether it is only rough (`about`).
//! - **Where it stands** is a word or two in the fact's language (`state`:
//!   待取, 已退, 已完成…), with `open` for code to go by: true while
//!   something is still to be done about it or it is still going on.
//!
//! Both are asked in the same call that keeps the fact (going over a
//! stretch of talk, `chat_remember::settle_system_prompt`, and sorting what
//! she knew before, `topics::sort_system_prompt`), by one rule
//! ([`HAPPENED_AND_STATE`]), and read back by [`read_standing`]: what is
//! absent or cannot be read is none, never a refusal.
//!
//! Asked for all of a kind, the recall judgment
//! (`remembering::system`) names the stretch of time the question is
//! about as a [`Range`] ("上个月", "今年以来", "最近三周"); [`Range::dates`]
//! turns it into days from the host's today, so no date is the model's to
//! work out and no clock is read here.

use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A state, at most, in characters: a word or two.
pub const STATE_CHARS: usize = 16;

/// How many units back a range may reach.
pub const RANGE_COUNT: u32 = 120;

/// The first and last day of `when`: a day (`2026-10-03`), a month
/// (`2026-10`) or a year (`2026`), written with every digit (one form for
/// each). None for anything else.
pub fn span_of(when: &str) -> Option<(NaiveDate, NaiveDate)> {
    let parts: Vec<&str> = when.split('-').collect();
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    let year = parts
        .first()
        .filter(|year| digits(year, 4))?
        .parse::<i32>()
        .ok()?;
    match parts.as_slice() {
        [_] => Some((
            NaiveDate::from_ymd_opt(year, 1, 1)?,
            NaiveDate::from_ymd_opt(year, 12, 31)?,
        )),
        [_, month] if digits(month, 2) => {
            let first = NaiveDate::from_ymd_opt(year, month.parse().ok()?, 1)?;
            let last = first.checked_add_months(Months::new(1))? - Duration::days(1);
            Some((first, last))
        }
        [_, month, day] if digits(month, 2) && digits(day, 2) => {
            let day = NaiveDate::from_ymd_opt(year, month.parse().ok()?, day.parse().ok()?)?;
            Some((day, day))
        }
        _ => None,
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// When what a fact tells of happened: `from` a day, a month or a year
/// ([`span_of`]); `to` the last one when it went on past `from`, absent
/// when it did not (never the same as `from`); `about` when the time is
/// only rough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Happened {
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub about: bool,
}

impl Happened {
    /// Its first and last day; None when it does not hold.
    pub fn span(&self) -> Option<(NaiveDate, NaiveDate)> {
        let (first, last) = span_of(&self.from)?;
        let Some(to) = &self.to else {
            return Some((first, last));
        };
        let (to_first, to_last) = span_of(to)?;
        (*to != self.from && to_first >= first && to_last >= last).then_some((first, to_last))
    }

    /// Why it does not hold, naming no value.
    pub fn check(&self) -> Result<(), &'static str> {
        if span_of(&self.from).is_none() {
            return Err("happened from is a day, a month or a year");
        }
        if let Some(to) = &self.to {
            if span_of(to).is_none() {
                return Err("happened to is a day, a month or a year");
            }
            if *to == self.from {
                return Err("happened to is left out when it is from");
            }
            if self.span().is_none() {
                return Err("happened to comes after from");
            }
        }
        Ok(())
    }

    /// As she reads it beside a fact: `2026-10-03`, `2026-10-03 to
    /// 2026-10-20`, `around 2026-10`.
    pub fn shown(&self) -> String {
        let when = match &self.to {
            Some(to) => format!("{} to {to}", self.from),
            None => self.from.clone(),
        };
        if self.about {
            format!("around {when}")
        } else {
            when
        }
    }
}

/// A state as a fact carries it: its words, one space apart, at most
/// [`STATE_CHARS`], no control character. None when it does not hold.
pub fn state_of(text: &str) -> Option<String> {
    let state = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!state.is_empty()
        && state.chars().count() <= STATE_CHARS
        && !state.chars().any(char::is_control))
    .then_some(state)
}

/// When a fact happened and where it stands, each none when not given or
/// not readable; `open` only beside a state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Standing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub happened: Option<Happened>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<bool>,
}

impl Standing {
    pub fn is_empty(&self) -> bool {
        self.happened.is_none() && self.state.is_none() && self.open.is_none()
    }
}

/// What a fact she keeps carries beside it, the same wherever she keeps
/// one: going over a stretch of talk, or sorting what she knew before.
macro_rules! happened_and_state {
    ($words:literal) => {
        concat!("happened is when what the fact tells of happened, which is not when they said it: from is the day (YYYY-MM-DD), or only the month (YYYY-MM) or the year (YYYY) when that is all that is known; to is the last day, month or year when it went on past from, else empty; about is true when the time is only rough (around then, early that month, a few weeks ago; also a month worked out from 'last month', which people say of a few weeks back). Work it out from today and what they said ('last Friday', 'the week before'); a plan has the day it is planned for. from and to are empty when the fact tells of no time of its own (a liking, who someone is, where they live) or none was given. \
state is where it stands now, in a word or two in the language of the fact ", $words, ", for something that is still to be done or can be over (a thing to pick up or return, an order, a booking, a project); else empty. open is true while something is still to be done about it or it is still going on, false once it is over (picked up, returned, finished, called off), and null when state is empty.")
    };
}

pub const HAPPENED_AND_STATE: &str = happened_and_state!("(待取, 已退, 已完成, 还在用, picked up)");

/// When a fact happened, the first half of [`HAPPENED_AND_STATE`], for an
/// end that asks when each fact happened and not where it stands.
pub fn happened_only() -> &'static str {
    HAPPENED_AND_STATE
        .split(" state is where")
        .next()
        .unwrap_or(HAPPENED_AND_STATE)
}

/// [`HAPPENED_AND_STATE`] with the state words in the examples' language.
pub fn happened_and_state(examples: crate::chat_remember::Examples) -> &'static str {
    match examples {
        crate::chat_remember::Examples::Chinese => HAPPENED_AND_STATE,
        crate::chat_remember::Examples::English => {
            happened_and_state!("(to pick up, returned, done, still in use, picked up)")
        }
    }
}

/// The schema of when a fact happened, in her answer.
pub fn happened_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "from": { "type": "string", "maxLength": 10 },
            "to": { "type": "string", "maxLength": 10 },
            "about": { "type": "boolean" }
        },
        "required": ["from", "to", "about"],
        "additionalProperties": false
    })
}

/// The schema of where it stands, in her answer.
pub fn state_schema() -> Value {
    json!({ "type": "string", "maxLength": STATE_CHARS })
}

pub fn open_schema() -> Value {
    json!({ "type": ["boolean", "null"] })
}

/// When it happened, read from her answer: `{from, to?, about?}` or a bare
/// `from`. None when from is empty or none of it holds.
pub fn read_happened(value: Option<&Value>) -> Option<Happened> {
    let text = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let value = value?;
    let (from, to, about) = match value {
        Value::String(_) => (text(Some(value))?, None, false),
        Value::Object(_) => (
            text(value.get("from"))?,
            text(value.get("to")),
            value.get("about").and_then(Value::as_bool).unwrap_or(false),
        ),
        _ => return None,
    };
    let happened = Happened {
        to: to.filter(|to| *to != from),
        from,
        about,
    };
    happened.check().is_ok().then_some(happened)
}

/// When it happened and where it stands, read from one item of her answer
/// (its `happened`, `state` and `open`): each none when absent or not
/// readable.
pub fn read_standing(item: &Value) -> Standing {
    let state = item.get("state").and_then(Value::as_str).and_then(state_of);
    Standing {
        happened: read_happened(item.get("happened")),
        open: state
            .as_ref()
            .and_then(|_| item.get("open"))
            .and_then(Value::as_bool),
        state,
    }
}

/// A unit of time a range counts in. A week starts on Monday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

/// The stretch of time a question is about, as the recall judgment names
/// it; [`Range::dates`] makes days of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case", deny_unknown_fields)]
pub enum Range {
    /// The whole of this day, week, month or year (今年以来, 这个月).
    This { unit: Unit },
    /// The `count` whole units before this one (上个月 is one month, 前两周
    /// two weeks, 去年 one year).
    Previous { unit: Unit, count: u32 },
    /// The last `count` units up to today, today included (最近三周, 这几天).
    Past { unit: Unit, count: u32 },
    /// From one day, month or year to another ([`span_of`]); either may be
    /// left open, not both (十月, 九月以来).
    Dates {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
}

fn unit_start(day: NaiveDate, unit: Unit) -> Option<NaiveDate> {
    match unit {
        Unit::Day => Some(day),
        Unit::Week => day.checked_sub_signed(Duration::days(i64::from(
            day.weekday().num_days_from_monday(),
        ))),
        Unit::Month => day.with_day(1),
        Unit::Year => NaiveDate::from_ymd_opt(day.year(), 1, 1),
    }
}

/// `day` moved `count` units back (or on, with `on`).
fn shift(day: NaiveDate, unit: Unit, count: u32, on: bool) -> Option<NaiveDate> {
    let days = |n: i64| Duration::days(if on { n } else { -n });
    match unit {
        Unit::Day => day.checked_add_signed(days(i64::from(count))),
        Unit::Week => day.checked_add_signed(days(7 * i64::from(count))),
        Unit::Month | Unit::Year => {
            let months = Months::new(if unit == Unit::Year {
                12 * count
            } else {
                count
            });
            if on {
                day.checked_add_months(months)
            } else {
                day.checked_sub_months(months)
            }
        }
    }
}

impl Range {
    /// Why it does not hold, naming no value.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::This { .. } => Ok(()),
            Self::Previous { count, .. } | Self::Past { count, .. } => {
                if (1..=RANGE_COUNT).contains(count) {
                    Ok(())
                } else {
                    Err("a range counts 1 to 120 units")
                }
            }
            Self::Dates { from, to } => {
                let span = |when: &Option<String>| when.as_deref().map(span_of);
                match (span(from), span(to)) {
                    (None, None) => Err("a range of dates has from or to"),
                    (Some(None), _) | (_, Some(None)) => {
                        Err("a range's from and to are a day, a month or a year")
                    }
                    (Some(Some((first, _))), Some(Some((_, last)))) if last < first => {
                        Err("a range's to comes after its from")
                    }
                    _ => Ok(()),
                }
            }
        }
    }

    /// Its first and last day, from `today` (the host's, in her zone);
    /// either is none when the range is open at that end. None when it
    /// does not hold.
    pub fn dates(&self, today: NaiveDate) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
        self.check().ok()?;
        let day = Duration::days(1);
        Some(match self {
            Self::This { unit } => {
                let first = unit_start(today, *unit)?;
                (Some(first), Some(shift(first, *unit, 1, true)? - day))
            }
            Self::Previous { unit, count } => {
                let this = unit_start(today, *unit)?;
                (Some(shift(this, *unit, *count, false)?), Some(this - day))
            }
            Self::Past { unit, count } => {
                (Some(shift(today, *unit, *count, false)? + day), Some(today))
            }
            Self::Dates { from, to } => (
                from.as_deref().and_then(span_of).map(|(first, _)| first),
                to.as_deref().and_then(span_of).map(|(_, last)| last),
            ),
        })
    }
}

/// What the recall judgment is told of the range, after `thorough`.
pub const RANGE_RULE: &str = "range is the stretch of time the question is about, when it names one; code works out the days from today, so do not: this with unit for the whole of this day, week, month or year (今年以来 is this year, 这个月 this month); previous with unit and count for the whole weeks, months or years before this one (上个月 is previous month 1, 前两周 previous week 2, 去年 previous year 1); past with unit and count for the last days, weeks, months or years up to today (最近三周 is past week 3, 这几天 past day 3, 近半年 past month 6); dates with from and to, each YYYY-MM-DD, YYYY-MM or YYYY and either left empty, for days or months named (十月 is from and to that October as YYYY-MM, 九月以来 from that September, 上上周 the first and last day of that week); none when it names no time. Unused parts are day, 0 and empty. \
open: true if they ask only about what is still to be done or still going on (还有哪些没取), false if only about what is over, null otherwise.";

/// The schema of the range and of `open` in the recall judgment.
pub fn range_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "how": { "type": "string", "enum": ["none", "this", "previous", "past", "dates"] },
            "unit": { "type": "string", "enum": ["day", "week", "month", "year"] },
            "count": { "type": "integer", "minimum": 0, "maximum": RANGE_COUNT },
            "from": { "type": "string", "maxLength": 10 },
            "to": { "type": "string", "maxLength": 10 }
        },
        "required": ["how", "unit", "count", "from", "to"],
        "additionalProperties": false
    })
}

/// The range in her answer; None for none, or what does not hold.
pub fn read_range(value: Option<&Value>) -> Option<Range> {
    let value = value?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let unit = || serde_json::from_value::<Unit>(value.get("unit")?.clone()).ok();
    let count = || {
        value
            .get("count")
            .and_then(Value::as_u64)
            .and_then(|count| u32::try_from(count).ok())
    };
    let range = match value.get("how")?.as_str()? {
        "this" => Range::This { unit: unit()? },
        "previous" => Range::Previous {
            unit: unit()?,
            count: count()?,
        },
        "past" => Range::Past {
            unit: unit()?,
            count: count()?,
        },
        "dates" => Range::Dates {
            from: text("from"),
            to: text("to"),
        },
        _ => return None,
    };
    range.check().is_ok().then_some(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_time_is_a_day_a_month_or_a_year_written_whole() {
        assert_eq!(
            span_of("2026-10-03"),
            Some((day("2026-10-03"), day("2026-10-03")))
        );
        assert_eq!(
            span_of("2026-02"),
            Some((day("2026-02-01"), day("2026-02-28")))
        );
        assert_eq!(
            span_of("2024-02"),
            Some((day("2024-02-01"), day("2024-02-29")))
        );
        assert_eq!(
            span_of("2026"),
            Some((day("2026-01-01"), day("2026-12-31")))
        );
        for wrong in [
            "2026-1-3",
            "2026-13",
            "2026-02-30",
            "26-10",
            "",
            "2026-10-03T00",
            "上个月",
        ] {
            assert_eq!(span_of(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn when_it_happened_reads_what_holds_and_drops_the_rest() {
        let read = |value: Value| read_happened(Some(&value));
        assert_eq!(
            read(json!({"from": " 2026-10-03 ", "to": "2026-10-03", "about": false})),
            Some(Happened {
                from: "2026-10-03".into(),
                to: None,
                about: false
            })
        );
        assert_eq!(
            read(json!({"from": "2026-10", "to": "2026-12", "about": true}))
                .unwrap()
                .shown(),
            "around 2026-10 to 2026-12"
        );
        assert_eq!(read(json!("2026-10")).unwrap().from, "2026-10");
        assert_eq!(
            read(json!({"from": "", "to": "2026-10", "about": false})),
            None
        );
        assert_eq!(
            read(json!({"from": "2026-10-20", "to": "2026-10-03"})),
            None
        );
        assert_eq!(read(json!({"from": "上周五"})), None);
        assert_eq!(read(json!(3)), None);
        assert_eq!(read_happened(None), None);
        // A day inside a month reaching past it is a stretch; one ending
        // before the month does is not.
        assert!(read(json!({"from": "2026-10", "to": "2026-11-02"})).is_some());
        assert!(read(json!({"from": "2026-10", "to": "2026-10-15"})).is_none());
    }

    #[test]
    fn where_it_stands_is_a_short_word_and_open_only_beside_it() {
        let standing = read_standing(&json!({
            "happened": {"from": "2026-10-03", "to": "", "about": false},
            "state": " 待取 ",
            "open": true
        }));
        assert_eq!(standing.state.as_deref(), Some("待取"));
        assert_eq!(standing.open, Some(true));
        assert!(standing.happened.is_some());
        let none = read_standing(&json!({"state": "", "open": false}));
        assert_eq!(none, Standing::default());
        assert!(none.is_empty());
        let long = read_standing(
            &json!({"state": "这一件衣服现在还放在干洗店里等着去取回来", "open": true}),
        );
        assert_eq!(long, Standing::default(), "too long is garbled, not cut");
        assert_eq!(
            read_standing(&json!({"state": "已退", "open": "yes"})).open,
            None
        );
        assert!(HAPPENED_AND_STATE.contains("which is not when they said it"));
    }

    #[test]
    fn a_range_becomes_days_from_today() {
        // A Thursday.
        let today = day("2026-11-05");
        let dates = |range: Range| {
            let (from, to) = range.dates(today).unwrap();
            (from.map(|d| d.to_string()), to.map(|d| d.to_string()))
        };
        let both = |from: &str, to: &str| (Some(from.to_string()), Some(to.to_string()));
        assert_eq!(
            dates(Range::This { unit: Unit::Year }),
            both("2026-01-01", "2026-12-31")
        );
        assert_eq!(
            dates(Range::This { unit: Unit::Week }),
            both("2026-11-02", "2026-11-08")
        );
        assert_eq!(
            dates(Range::Previous {
                unit: Unit::Month,
                count: 1
            }),
            both("2026-10-01", "2026-10-31")
        );
        assert_eq!(
            dates(Range::Previous {
                unit: Unit::Week,
                count: 2
            }),
            both("2026-10-19", "2026-11-01")
        );
        assert_eq!(
            dates(Range::Previous {
                unit: Unit::Year,
                count: 1
            }),
            both("2025-01-01", "2025-12-31")
        );
        assert_eq!(
            dates(Range::Past {
                unit: Unit::Week,
                count: 3
            }),
            both("2026-10-16", "2026-11-05")
        );
        assert_eq!(
            dates(Range::Past {
                unit: Unit::Month,
                count: 6
            }),
            both("2026-05-06", "2026-11-05")
        );
        assert_eq!(
            dates(Range::Dates {
                from: Some("2026-09".into()),
                to: None
            }),
            (Some("2026-09-01".to_string()), None)
        );
        assert_eq!(
            dates(Range::Dates {
                from: Some("2026-10".into()),
                to: Some("2026-10".into())
            }),
            both("2026-10-01", "2026-10-31")
        );
        assert_eq!(
            Range::Past {
                unit: Unit::Day,
                count: 0
            }
            .dates(today),
            None
        );
        assert_eq!(
            Range::Dates {
                from: None,
                to: None
            }
            .dates(today),
            None
        );
        // Month ends: a month back from the 31st is the last day there is.
        assert_eq!(
            Range::Past {
                unit: Unit::Month,
                count: 1
            }
            .dates(day("2026-03-31")),
            Some((Some(day("2026-03-01")), Some(day("2026-03-31"))))
        );
    }

    #[test]
    fn the_range_she_names_is_read_or_is_none() {
        let read = |value: Value| read_range(Some(&value));
        assert_eq!(
            read(json!({"how": "previous", "unit": "month", "count": 1, "from": "", "to": ""})),
            Some(Range::Previous {
                unit: Unit::Month,
                count: 1
            })
        );
        assert_eq!(
            read(json!({"how": "this", "unit": "year", "count": 0, "from": "", "to": ""})),
            Some(Range::This { unit: Unit::Year })
        );
        assert_eq!(
            read(json!({"how": "dates", "unit": "day", "count": 0, "from": "2026-09", "to": " "})),
            Some(Range::Dates {
                from: Some("2026-09".into()),
                to: None
            })
        );
        assert_eq!(
            read(json!({"how": "none", "unit": "day", "count": 0, "from": "", "to": ""})),
            None
        );
        assert_eq!(
            read(json!({"how": "past", "unit": "week", "count": 0})),
            None
        );
        assert_eq!(
            read(json!({"how": "past", "unit": "fortnight", "count": 2})),
            None
        );
        assert_eq!(
            read(json!({"how": "dates", "from": "2026-10", "to": "2026-09"})),
            None
        );
        assert_eq!(read(json!("last month")), None);
        assert_eq!(read_range(None), None);
        let range = Range::Past {
            unit: Unit::Week,
            count: 3,
        };
        let value = serde_json::to_value(&range).unwrap();
        assert_eq!(value, json!({"how": "past", "unit": "week", "count": 3}));
        assert_eq!(serde_json::from_value::<Range>(value).unwrap(), range);
    }
}
