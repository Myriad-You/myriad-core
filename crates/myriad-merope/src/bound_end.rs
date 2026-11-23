//! When what someone asked of her ends (「今晚别找我了」, "leave me alone till
//! next Wednesday"): the model names which stretch they meant
//! ([`BoundEnd`]), and the time it ends is worked out here from when they
//! said it, so no date is the model's to work out and no clock is read here
//! (as [`crate::happened::Range`]).
//!
//! A week starts on Monday. Tonight ends at [`NIGHT_ENDS`] on their clock:
//! her own day turns when she gets up, on her clock, which says nothing of
//! theirs. Their clock is the zone the host keeps for them; a host that keeps
//! none (the site keeps nobody's) goes by hers ([`BoundEnd::ends_at`]). What
//! can be read two ways is the longer: a day or a week of this one gone by is
//! the next to come, and before today, this week or this month is through
//! it. A time of today gone by when they said it, or a stretch that does not
//! hold together, has no end known, as one they gave none.

use chrono::{
    DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
    Utc, Weekday,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::happened::{Unit, span_of, unit_start};

/// The hour a night ends and the next day's morning begins, on their clock.
pub const NIGHT_ENDS: u32 = 5;
/// The most days, weeks, months or hours from now an end may be.
const COUNT_AT_MOST: u32 = 400;

/// What the model is told to answer, beside [`bound_end_schema`].
pub const BOUND_END_RULE: &str = "end says when it ends, as they put it, never worked out by you; every field is given, with what does not apply as \"none\", 0 or \"\". \
how: \"tonight\" (今晚, tonight, 今夜), \"today\", \"tomorrow\", \"week\" or \"month\" (which is this or next), \"weekday\" (a day of the week: weekday, with which next when they say 下周三 / next Wednesday / 来週の水曜, this when they say 这周三 / this Wednesday / 今週の水曜, coming when they only say the day: 周三, Wednesday, 水曜), \"days\" (count days: which is this for 这三天, these three days, 今日から三日, next for 接下来两天, 未来两天, the next two days, これから二日, 明天起两天), \"hours\" (count hours from now), \"date\" (a date they wrote or named by month and day: date as YYYY-MM-DD), or \"unsaid\" (they gave no end, or one that is none of these: 一阵子, 忙完再说). \
edge is \"before\" for 下周三之前, until Wednesday, 水曜まで待って when the day itself is free; \"after\" for 周日之后再说, after Sunday, 日曜が終わったら; \"through\" for that day, week or month itself (这周, through Friday). \
time is the clock time they named on that day (HH:MM), else \"\".";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    Tonight,
    Today,
    Tomorrow,
    Week,
    Month,
    Weekday,
    Days,
    Hours,
    Date,
    Unsaid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Which {
    This,
    Next,
    /// The next one to come, today among them.
    Coming,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// Over once it begins.
    Before,
    /// Over once it is over, as [`Edge::Through`]: the day itself is still
    /// part of it.
    After,
    /// Over once it is over.
    Through,
    /// Not told: the longer of the two.
    None,
}

/// What goes before and after [`BOUND_END_RULE`] where what they said is
/// judged for what it leaves her (`myriad_nodes::intents`): what else of it
/// she is to go by.
const MARKS: [&str; 2] = [
    "known is some of what you know of them.\n\
Also mark what you must do differently because of what they just said, if anything: \
- \"keep\": they ask you to remember something, tell you what to call them (or what they call you) or what never to bring up, or tell you of a big change in their life. \
- \"forget\": they take back something they said, or ask you not to keep it. \
- \"change\": they correct something in known that you have wrong, or tell you it is no longer so. \
- \"bound\": they ask you not to reach out to them for a while; ",
    " On other marks end is null. \
Appointments, reminders and things they ask you to do are not marks (they are handled as you take them); a change of mind within this same talk is not a mark; most of what people say is none of these: marks is empty.\n\
Answer only in JSON.",
];
const ANSWER_ONLY: &str = "Answer only in JSON.";

/// A judgment of what they said (`system`, `schema`, its input) asked for its
/// marks too: what the host asks instead. None when `schema` is no object's
/// or `input` no object.
pub fn with_marks(system: &str, schema: &Value, input: &Value) -> Option<(String, Value, Value)> {
    let system = system.trim_end();
    let system = system.strip_suffix(ANSWER_ONLY).unwrap_or(system);
    let system = format!("{system}{}{BOUND_END_RULE}{}", MARKS[0], MARKS[1]);
    let mut schema = schema.clone();
    schema.get_mut("properties")?.as_object_mut()?.insert(
        "marks".into(),
        json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["keep", "forget", "change", "bound"] },
                    "end": { "anyOf": [bound_end_schema(), { "type": "null" }] }
                },
                "required": ["kind", "end"],
                "additionalProperties": false
            }
        }),
    );
    schema
        .get_mut("required")?
        .as_array_mut()?
        .push("marks".into());
    let mut input = input.clone();
    input.as_object_mut()?.insert("known".into(), json!([]));
    Some((system, schema, input))
}

/// Until when they asked to be left alone in her answer to [`with_marks`],
/// one for each time they did; an end that does not read is one they did
/// not give. Nothing in an answer that does not read.
pub fn read_bounds(answer: &str) -> Vec<BoundEnd> {
    let Some(answer) = crate::answer::parse::<Value>(answer) else {
        return Vec::new();
    };
    answer["marks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|mark| mark["kind"] == "bound")
        .filter_map(|mark| read_bound_end(&mark["end"]))
        .collect()
}

/// The stretch they meant, as the model names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundEnd {
    pub how: How,
    pub which: Which,
    /// `mon` to `sun`.
    pub weekday: String,
    /// 0 when it is no number of days or hours, whatever else it says.
    #[serde(deserialize_with = "count")]
    pub count: u32,
    pub edge: Edge,
    pub date: String,
    pub time: String,
}

fn count<'de, D: Deserializer<'de>>(read: D) -> Result<u32, D::Error> {
    let value = Value::deserialize(read)?;
    Ok(value
        .as_u64()
        .and_then(|count| u32::try_from(count).ok())
        .unwrap_or(0))
}

pub fn bound_end_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "how": { "type": "string", "enum": ["tonight", "today", "tomorrow", "week", "month", "weekday", "days", "hours", "date", "unsaid"] },
            "which": { "type": "string", "enum": ["this", "next", "coming", "none"] },
            "weekday": { "type": "string", "enum": ["mon", "tue", "wed", "thu", "fri", "sat", "sun", "none"] },
            "count": { "type": "integer" },
            "edge": { "type": "string", "enum": ["before", "after", "through", "none"] },
            "date": { "type": "string" },
            "time": { "type": "string" }
        },
        "required": ["how", "which", "weekday", "count", "edge", "date", "time"],
        "additionalProperties": false
    })
}

/// The end in her answer; None for what is not one.
pub fn read_bound_end(value: &Value) -> Option<BoundEnd> {
    serde_json::from_value(value.clone()).ok()
}

impl BoundEnd {
    /// When it ends on their clock, said at `said` there. None when they gave
    /// no end, it was over already, or it does not hold together.
    pub fn ends(&self, said: NaiveDateTime) -> Option<NaiveDateTime> {
        let today = said.date();
        if self.how == How::Hours {
            return self.hours().map(|hours| said + hours);
        }
        if self.how == How::Days {
            return self.days(today);
        }
        let time = self.clock()?;
        if self.how == How::Tonight {
            let mut end = tonight(said, time);
            // Past midnight, an evening's time is that of the day begun.
            if end <= said && said.hour() < NIGHT_ENDS {
                end += Duration::days(1);
            }
            return (end > said).then_some(end);
        }
        if self.how == How::Weekday {
            let day = self.weekday_from(today)?;
            let end = |day| self.over(day, day, time);
            // Gone by: the next to come.
            return end(day)
                .filter(|end| *end > said)
                .or_else(|| end(on(day, 7)?))
                .filter(|end| *end > said);
        }
        // The days it is about, first and last.
        let (first, last) = match self.how {
            How::Today => (today, today),
            How::Tomorrow => {
                let day = on(today, 1)?;
                (day, day)
            }
            How::Week | How::Month => {
                let unit = if self.how == How::Week {
                    Unit::Week
                } else {
                    Unit::Month
                };
                let begins = unit_start(today, unit)?;
                let after = |from: NaiveDate| match unit {
                    Unit::Week => on(from, 7),
                    _ => from.checked_add_months(Months::new(1)),
                };
                let first = match self.which {
                    Which::This => begins,
                    Which::Next => after(begins)?,
                    Which::Coming | Which::None => return None,
                };
                (first, on(after(first)?, -1)?)
            }
            How::Date => span_of(self.date.trim())?,
            How::Tonight | How::Hours | How::Weekday | How::Days | How::Unsaid => return None,
        };
        // What today is of is not over before it began: 今天之内, 这周内.
        let first = if first <= today && today <= last && (time.is_none() || first != last) {
            on(last, 1)?
        } else {
            first
        };
        self.over(first, last, time)
            .filter(|end| *end > said)
            // Today's midnight, named as a time, is the one it ends with.
            .or_else(|| {
                (self.how == How::Today && time == Some(NaiveTime::MIN))
                    .then(|| on(today, 1))
                    .flatten()?
                    .and_hms_opt(0, 0, 0)
            })
    }

    /// When the days from `first` to `last` are over, as edge has it, or at
    /// `time` of a single one.
    fn over(
        &self,
        first: NaiveDate,
        last: NaiveDate,
        time: Option<NaiveTime>,
    ) -> Option<NaiveDateTime> {
        Some(match (time, self.edge) {
            (Some(time), _) if first == last => self.day_of(last)?.and_time(time),
            (_, Edge::Before) => first.and_time(NaiveTime::MIN),
            (_, Edge::After | Edge::Through | Edge::None) => on(last, 1)?.and_time(NaiveTime::MIN),
        })
    }

    /// From tomorrow, however they said it: which of the two they meant is
    /// not told apart reliably, and a day too long is the smaller wrong. Over
    /// with the last of them, whatever edge and time say: 三天内, 三天后 and 三天
    /// end the same day.
    fn days(&self, today: NaiveDate) -> Option<NaiveDateTime> {
        (1..=COUNT_AT_MOST)
            .contains(&self.count)
            .then(|| on(today, i64::from(self.count) + 1))
            .flatten()?
            .and_hms_opt(0, 0, 0)
    }

    fn hours(&self) -> Option<Duration> {
        (1..=COUNT_AT_MOST)
            .contains(&self.count)
            .then(|| Duration::hours(i64::from(self.count)))
    }

    /// [`Self::ends`], as a moment: said at `said`, on the clock of `theirs`,
    /// or of hers when the host keeps no zone of theirs. A local time that
    /// comes twice is the first; one that never comes (the clock jumped over
    /// it), an hour on. Hours are of time gone by, whatever the clock does.
    pub fn ends_at<Z: TimeZone>(
        &self,
        said: DateTime<Utc>,
        theirs: Option<&Z>,
        hers: &Z,
    ) -> Option<DateTime<Utc>> {
        if self.how == How::Hours {
            return self.hours().map(|hours| said + hours);
        }
        let zone = theirs.unwrap_or(hers);
        let ends = self.ends(said.with_timezone(zone).naive_local())?;
        let moment = |at: NaiveDateTime| zone.from_local_datetime(&at);
        let at = match moment(ends) {
            // The first of them, or the one still to come.
            chrono::LocalResult::Ambiguous(first, second) => {
                if first.with_timezone(&Utc) > said {
                    first
                } else {
                    second
                }
            }
            one => one
                .single()
                .or_else(|| moment(ends + Duration::hours(1)).earliest())?,
        };
        Some(at.with_timezone(&Utc)).filter(|at| *at > said)
    }

    /// The time of day they named; `Some(None)` when they named none, None
    /// when what is there is not one. 24:00 is the midnight the day ends with
    /// ([`Self::day_of`]).
    fn clock(&self) -> Option<Option<NaiveTime>> {
        match self.time.trim() {
            "" | "none" => Some(None),
            "24:00" => Some(Some(NaiveTime::MIN)),
            time => NaiveTime::parse_from_str(time, "%H:%M").ok().map(Some),
        }
    }

    /// The day the time they named is on, for `day`.
    fn day_of(&self, day: NaiveDate) -> Option<NaiveDate> {
        on(day, i64::from(self.time.trim() == "24:00"))
    }

    /// The day of the week they meant: 下周三 is next week's, 这周三 this
    /// week's, 周三 the next to come, today when it is one.
    fn weekday_from(&self, today: NaiveDate) -> Option<NaiveDate> {
        let weekday: Weekday = self.weekday.parse().ok()?;
        let monday = unit_start(today, Unit::Week)?;
        let into = i64::from(weekday.num_days_from_monday());
        match self.which {
            Which::This => on(monday, into),
            Which::Next => on(monday, 7 + into),
            Which::Coming => {
                let ahead =
                    (into - i64::from(today.weekday().num_days_from_monday())).rem_euclid(7);
                on(today, ahead)
            }
            Which::None => None,
        }
    }
}

/// The end of the night `said` is in, or goes into: [`NIGHT_ENDS`] after it.
/// Said past midnight, it is the night still going on. With a time they
/// named, the one that night (before noon, in the morning it ends with).
fn tonight(said: NaiveDateTime, time: Option<NaiveTime>) -> NaiveDateTime {
    let evening = if said.hour() < NIGHT_ENDS {
        said.date() - Duration::days(1)
    } else {
        said.date()
    };
    let next = evening + Duration::days(1);
    match time {
        Some(time) if time.hour() >= 12 => evening.and_time(time),
        Some(time) => next.and_time(time),
        None => next.and_time(NaiveTime::from_hms_opt(NIGHT_ENDS, 0, 0).unwrap_or_default()),
    }
}

fn on(day: NaiveDate, days: i64) -> Option<NaiveDate> {
    day.checked_add_signed(Duration::days(days))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end(fields: Value) -> BoundEnd {
        let mut whole = json!({
            "how": "unsaid", "which": "none", "weekday": "none", "count": 0,
            "edge": "through", "date": "", "time": "",
        });
        for (key, value) in fields.as_object().unwrap() {
            whole[key] = value.clone();
        }
        read_bound_end(&whole).unwrap()
    }

    /// Said at `said`, when it ends.
    fn ends(fields: Value, said: &str) -> Option<String> {
        let said = NaiveDateTime::parse_from_str(said, "%Y-%m-%dT%H:%M").unwrap();
        end(fields)
            .ends(said)
            .map(|end| end.format("%Y-%m-%dT%H:%M").to_string())
    }

    fn is(fields: Value, said: &str, expected: &str) {
        assert_eq!(
            ends(fields.clone(), said).as_deref(),
            Some(expected),
            "{fields} at {said}"
        );
    }

    // 2026-11-23 is a Monday.
    const MONDAY: &str = "2026-11-23T21:00";
    const SUNDAY: &str = "2026-11-29T10:00";

    /// 今晚, tonight, 今夜: the night now or coming, till the morning; past
    /// midnight, the one still going on.
    #[test]
    fn tonight_ends_in_the_morning() {
        let tonight = json!({"how": "tonight"});
        is(tonight.clone(), MONDAY, "2026-11-24T05:00");
        is(tonight.clone(), "2026-11-23T09:00", "2026-11-24T05:00");
        is(tonight.clone(), "2026-11-24T01:30", "2026-11-24T05:00");
        is(tonight.clone(), "2026-11-24T05:00", "2026-11-25T05:00");
        is(
            json!({"how": "tonight", "time": "22:30"}),
            MONDAY,
            "2026-11-23T22:30",
        );
        is(
            json!({"how": "tonight", "time": "02:00"}),
            MONDAY,
            "2026-11-24T02:00",
        );
        is(
            json!({"how": "tonight", "time": "24:00"}),
            MONDAY,
            "2026-11-24T00:00",
        );
        // Over already when said: not known. Past midnight, the evening of
        // the day begun.
        assert_eq!(
            ends(json!({"how": "tonight", "time": "20:00"}), MONDAY),
            None
        );
        is(
            json!({"how": "tonight", "time": "22:30"}),
            "2026-11-24T01:30",
            "2026-11-24T22:30",
        );
    }

    /// 今晚别找我，明早八点再说: the morning the night ends in.
    #[test]
    fn a_morning_of_tonight_is_the_next() {
        let eight = json!({"how": "tonight", "time": "08:00"});
        is(eight.clone(), MONDAY, "2026-11-24T08:00");
        is(eight.clone(), "2026-11-23T09:00", "2026-11-24T08:00");
        is(eight.clone(), "2026-11-23T07:30", "2026-11-24T08:00");
        is(eight, "2026-11-24T01:30", "2026-11-24T08:00");
        is(
            json!({"how": "tonight", "time": "11:59"}),
            MONDAY,
            "2026-11-24T11:59",
        );
        assert_eq!(
            ends(json!({"how": "tonight", "time": "12:00"}), MONDAY),
            None
        );
    }

    /// 三天后下午三点: the days hold, whatever the time says.
    #[test]
    fn days_hold_though_the_time_does_not_read() {
        for time in ["三点", "15:00", "25:00"] {
            is(
                json!({"how": "days", "count": 3, "time": time}),
                MONDAY,
                "2026-11-27T00:00",
            );
        }
    }

    /// 今天之内, 这周内, 三天内, 三天后: what edge says never ends them sooner.
    #[test]
    fn before_does_not_end_what_has_begun() {
        let before = |fields: Value| {
            let mut fields = fields;
            fields["edge"] = json!("before");
            fields
        };
        is(before(json!({"how": "today"})), MONDAY, "2026-11-24T00:00");
        is(
            before(json!({"how": "week", "which": "this"})),
            MONDAY,
            "2026-11-30T00:00",
        );
        is(
            before(json!({"how": "month", "which": "this"})),
            MONDAY,
            "2026-12-01T00:00",
        );
        is(
            before(json!({"how": "date", "date": "2026-11-23"})),
            MONDAY,
            "2026-11-24T00:00",
        );
        is(
            before(json!({"how": "days", "count": 3})),
            MONDAY,
            "2026-11-27T00:00",
        );
        for time in ["24:00", "00:00"] {
            is(
                before(json!({"how": "today", "time": time})),
                MONDAY,
                "2026-11-24T00:00",
            );
        }
        // What has not begun ends before it does.
        is(
            before(json!({"how": "month", "which": "next"})),
            MONDAY,
            "2026-12-01T00:00",
        );
        is(
            before(json!({"how": "date", "date": "2028-02-29"})),
            MONDAY,
            "2028-02-29T00:00",
        );
        is(
            json!({"how": "date", "date": "2028-02-29"}),
            MONDAY,
            "2028-03-01T00:00",
        );
    }

    /// A count that is none is that alone.
    #[test]
    fn a_count_that_is_no_count_is_none() {
        for count in [json!(-1), json!("3"), json!(1.5), json!(null)] {
            is(
                json!({"how": "tonight", "count": count}),
                MONDAY,
                "2026-11-24T05:00",
            );
            assert_eq!(
                ends(json!({"how": "days", "count": count}), MONDAY),
                None,
                "{count}"
            );
        }
    }

    /// 下周三, next Wednesday, 来週の水曜: next week's, the day itself free
    /// with 之前.
    #[test]
    fn a_weekday_of_next_week_is_next_weeks() {
        let next =
            |edge: &str| json!({"how": "weekday", "which": "next", "weekday": "wed", "edge": edge});
        is(next("before"), MONDAY, "2026-12-02T00:00");
        is(next("through"), MONDAY, "2026-12-03T00:00");
        // 下周三之后再说: that Wednesday is still part of it; not told, too.
        is(next("after"), MONDAY, "2026-12-03T00:00");
        is(next("none"), MONDAY, "2026-12-03T00:00");
        // On a Sunday next Monday is tomorrow.
        let monday = json!({"how": "weekday", "which": "next", "weekday": "mon", "edge": "before"});
        is(monday.clone(), SUNDAY, "2026-11-30T00:00");
        is(monday, MONDAY, "2026-11-30T00:00");
    }

    /// 周三, Wednesday, 水曜: the next to come, today when it is one.
    #[test]
    fn a_weekday_alone_is_the_next_to_come() {
        let coming = |day: &str, edge: &str| json!({"how": "weekday", "which": "coming", "weekday": day, "edge": edge});
        is(coming("wed", "through"), MONDAY, "2026-11-26T00:00");
        is(
            coming("wed", "through"),
            "2026-11-25T08:00",
            "2026-11-26T00:00",
        );
        is(coming("wed", "before"), MONDAY, "2026-11-25T00:00");
        is(coming("mon", "through"), SUNDAY, "2026-12-01T00:00");
        is(
            coming("sun", "through"),
            "2026-11-28T10:00",
            "2026-11-30T00:00",
        );
        // 周三之前, on that Wednesday, and a time of today gone by: the next.
        is(
            coming("wed", "before"),
            "2026-11-25T08:00",
            "2026-12-02T00:00",
        );
        let three = json!({"how": "weekday", "which": "coming", "weekday": "wed", "time": "15:00"});
        is(three.clone(), "2026-11-25T08:00", "2026-11-25T15:00");
        is(three, "2026-11-25T16:00", "2026-12-02T15:00");
    }

    /// 这周三, this Wednesday, 今週の水曜: this week's; gone already, the next
    /// to come (said on a Sunday, 'this Friday' is).
    #[test]
    fn a_weekday_of_this_week_gone_already_is_the_next() {
        let this = |day: &str| json!({"how": "weekday", "which": "this", "weekday": day, "edge": "through"});
        is(this("wed"), MONDAY, "2026-11-26T00:00");
        is(this("fri"), "2026-11-25T10:00", "2026-11-28T00:00");
        is(this("wed"), "2026-11-26T10:00", "2026-12-03T00:00");
        is(this("fri"), SUNDAY, "2026-12-05T00:00");
        is(this("sun"), SUNDAY, "2026-11-30T00:00");
    }

    #[test]
    fn days_weeks_months_and_hours_count_from_when_it_was_said() {
        is(json!({"how": "today"}), MONDAY, "2026-11-24T00:00");
        is(
            json!({"how": "today", "time": "23:00"}),
            MONDAY,
            "2026-11-23T23:00",
        );
        is(json!({"how": "tomorrow"}), MONDAY, "2026-11-25T00:00");
        is(
            json!({"how": "tomorrow", "time": "15:00"}),
            MONDAY,
            "2026-11-24T15:00",
        );
        is(
            json!({"how": "week", "which": "this"}),
            MONDAY,
            "2026-11-30T00:00",
        );
        is(
            json!({"how": "week", "which": "this"}),
            SUNDAY,
            "2026-11-30T00:00",
        );
        is(
            json!({"how": "week", "which": "next"}),
            SUNDAY,
            "2026-12-07T00:00",
        );
        is(
            json!({"how": "week", "which": "next", "edge": "before"}),
            MONDAY,
            "2026-11-30T00:00",
        );
        is(
            json!({"how": "month", "which": "this"}),
            MONDAY,
            "2026-12-01T00:00",
        );
        is(
            json!({"how": "month", "which": "next"}),
            "2026-12-31T10:00",
            "2027-02-01T00:00",
        );
        is(
            json!({"how": "days", "count": 2, "which": "next"}),
            MONDAY,
            "2026-11-26T00:00",
        );
        is(
            json!({"how": "days", "count": 3, "which": "this"}),
            MONDAY,
            "2026-11-27T00:00",
        );
        is(
            json!({"how": "days", "count": 2}),
            MONDAY,
            "2026-11-26T00:00",
        );
        is(
            json!({"how": "hours", "count": 3}),
            MONDAY,
            "2026-11-24T00:00",
        );
        is(
            json!({"how": "date", "date": "2026-12-05"}),
            MONDAY,
            "2026-12-06T00:00",
        );
        is(
            json!({"how": "date", "date": "2026-12-05", "edge": "before"}),
            MONDAY,
            "2026-12-05T00:00",
        );
    }

    #[test]
    fn what_does_not_hold_together_has_no_end_known() {
        for fields in [
            json!({"how": "unsaid"}),
            json!({"how": "week"}),
            json!({"how": "weekday", "which": "next"}),
            json!({"how": "weekday", "weekday": "wed"}),
            json!({"how": "days", "count": 0, "which": "this"}),
            json!({"how": "hours", "count": 0}),
            json!({"how": "date", "date": "下周"}),
            json!({"how": "date", "date": "2026-11-20"}),
            json!({"how": "tomorrow", "time": "三点"}),
        ] {
            assert_eq!(ends(fields.clone(), MONDAY), None, "{fields}");
        }
        assert_eq!(read_bound_end(&json!({"how": "soon"})), None);
    }

    /// Tonight ends in their own morning; where no zone of theirs is kept,
    /// in hers.
    #[test]
    fn their_clock_is_hers_where_none_of_theirs_is_kept() {
        use chrono_tz::{America::New_York, Asia::Singapore};
        let said = Utc.with_ymd_and_hms(2026, 11, 23, 13, 0, 0).unwrap();
        let tonight = end(json!({"how": "tonight"}));
        let at = |theirs| {
            tonight
                .ends_at(said, theirs, &Singapore)
                .unwrap()
                .format("%Y-%m-%dT%H:%M")
                .to_string()
        };
        // 21:00 in Singapore: five the next morning there.
        assert_eq!(at(None), "2026-11-23T21:00");
        // Eight in the morning in New York: the night after that day.
        assert_eq!(at(Some(&New_York)), "2026-11-24T10:00");
    }

    /// Where the clock jumps: hours are of time gone by, a local time that
    /// comes twice is the first still to come, and one that never comes, an
    /// hour on.
    #[test]
    fn a_clock_that_jumps_ends_nothing_early() {
        use chrono_tz::America::New_York;
        let at = |fields: Value, said: DateTime<Utc>| {
            end(fields)
                .ends_at(said, None, &New_York)
                .unwrap()
                .format("%Y-%m-%dT%H:%M")
                .to_string()
        };
        // 2026-11-01: 01:30 comes twice (05:30Z and 06:30Z).
        let first = Utc.with_ymd_and_hms(2026, 11, 1, 5, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 11, 1, 6, 0, 0).unwrap();
        let half_past = json!({"how": "today", "time": "01:30"});
        assert_eq!(at(half_past.clone(), first), "2026-11-01T05:30");
        // Said at 01:00 for the second time: the first 01:30 is gone.
        assert_eq!(at(half_past, second), "2026-11-01T06:30");
        assert_eq!(
            at(json!({"how": "hours", "count": 3}), first),
            "2026-11-01T08:00"
        );
        // 2026-03-08: 02:30 never comes.
        let spring = Utc.with_ymd_and_hms(2026, 3, 8, 5, 30, 0).unwrap();
        assert_eq!(
            at(json!({"how": "today", "time": "02:30"}), spring),
            "2026-03-08T07:30"
        );
    }

    #[test]
    fn a_judgment_is_asked_for_its_marks_too() {
        let schema = json!({"type": "object", "properties": {"take": {"type": "array"}}, "required": ["take"], "additionalProperties": false});
        let (system, schema, input) = with_marks(
            "You are her. Answer only in JSON.",
            &schema,
            &json!({"said": "今晚别找我了"}),
        )
        .unwrap();
        assert!(system.starts_with("You are her. known is some of"));
        assert!(system.contains(BOUND_END_RULE));
        assert!(!system.contains("will be away"));
        assert_eq!(system.matches(ANSWER_ONLY).count(), 1);
        assert!(system.ends_with(ANSWER_ONLY));
        assert_eq!(schema["required"], json!(["take", "marks"]));
        assert_eq!(input["known"], json!([]));
        assert!(with_marks("", &json!([]), &json!({})).is_none());
    }

    #[test]
    fn only_bounds_with_an_end_that_reads_are_read() {
        let tonight = json!({"how": "tonight", "which": "none", "weekday": "none", "count": 0, "edge": "none", "date": "", "time": ""});
        let answer = json!({"take": [], "drop": [], "marks": [
            {"kind": "keep", "end": null},
            {"kind": "bound", "end": tonight},
            {"kind": "bound", "end": null},
        ]});
        let bounds = read_bounds(&answer.to_string());
        assert_eq!(bounds.len(), 1);
        assert_eq!(bounds[0].how, How::Tonight);
        assert_eq!(read_bounds(&format!("```json\n{answer}\n```")).len(), 1);
        assert!(read_bounds("今晚").is_empty());
        assert!(read_bounds(r#"{"take": []}"#).is_empty());
    }
}
