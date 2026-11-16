//! Whole numbers as JSON carries them between ends: `3` and `3.0` are the same count. An
//! encoder that keeps every number as a double writes the second; a state or an input that
//! went through one reads back the same. A number with a fraction, a negative one where a
//! count is due, or one past what the type holds is not taken.

use serde::{Deserialize, Deserializer, de::Error};

#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

impl Number {
    /// The whole number it is, when it is one a double holds exactly or an integer as written.
    fn whole(self) -> Option<i128> {
        match self {
            Number::Unsigned(n) => Some(i128::from(n)),
            Number::Signed(n) => Some(i128::from(n)),
            Number::Float(n) => {
                (n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0)
                    .then_some(n as i128)
            }
        }
    }
}

fn read<'de, D: Deserializer<'de>, T: TryFrom<i128>>(deserializer: D) -> Result<T, D::Error> {
    Number::deserialize(deserializer)
        .ok()
        .and_then(Number::whole)
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| D::Error::custom("not a whole number of this range"))
}

pub fn u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    read(deserializer)
}

pub fn i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    read(deserializer)
}

pub fn u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    read(deserializer)
}

pub fn u16<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    read(deserializer)
}

/// Every whole number in `value` that is written as a double (`3.0`), as the whole number
/// it is (`3`), all the way down. For what an end whose JSON library keeps every number as
/// a double hands in: a frame's body goes out as it was given, and her host reads a count
/// only from a whole number. A number with a fraction, or one a double does not hold
/// exactly, stays as it is.
pub fn settle(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Number(number) if number.is_f64() => {
            let n = number.as_f64().unwrap_or(f64::NAN);
            if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
                *value = Value::from(n as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(settle),
        Value::Object(fields) => fields.values_mut().for_each(settle),
        _ => {}
    }
}

/// A count or null. A field that may also be left out says `#[serde(default)]` beside it:
/// `deserialize_with` alone makes it required.
pub fn maybe_u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    struct Whole(#[serde(deserialize_with = "u64")] u64);
    Ok(Option::<Whole>::deserialize(deserializer)?.map(|whole| whole.0))
}

pub fn maybe_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    struct Whole(#[serde(deserialize_with = "i64")] i64);
    Ok(Option::<Whole>::deserialize(deserializer)?.map(|whole| whole.0))
}

pub fn maybe_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    struct Whole(#[serde(deserialize_with = "u32")] u32);
    Ok(Option::<Whole>::deserialize(deserializer)?.map(|whole| whole.0))
}

pub fn maybe_u16<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u16>, D::Error> {
    #[derive(Deserialize)]
    struct Whole(#[serde(deserialize_with = "u16")] u16);
    Ok(Option::<Whole>::deserialize(deserializer)?.map(|whole| whole.0))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Counts {
        #[serde(deserialize_with = "super::u64")]
        count: u64,
        #[serde(default, deserialize_with = "super::maybe_u16")]
        code: Option<u16>,
        #[serde(deserialize_with = "super::i64")]
        time: i64,
    }

    fn read(count: serde_json::Value) -> Option<u64> {
        serde_json::from_value::<Counts>(json!({ "count": count, "time": 0 }))
            .ok()
            .map(|counts| counts.count)
    }

    #[test]
    fn a_whole_number_is_read_however_it_is_written() {
        assert_eq!(read(json!(3)), Some(3));
        assert_eq!(read(json!(3.0)), Some(3));
        assert_eq!(read(json!(-0.0)), Some(0));
        assert_eq!(read(json!(u64::MAX)), Some(u64::MAX));
        assert_eq!(read(json!(9_007_199_254_740_992.0)), Some(1 << 53));
        for not in [
            json!(2.5),
            json!(-1),
            json!(-1.0),
            json!(1e300),
            json!("3"),
            json!(null),
            json!([3]),
        ] {
            assert_eq!(read(not.clone()), None, "{not}");
        }
        let all: Counts =
            serde_json::from_value(json!({ "count": 1.0, "code": 4001.0, "time": -5.0 })).unwrap();
        assert_eq!(
            all,
            Counts {
                count: 1,
                code: Some(4001),
                time: -5
            }
        );
        // Null and left out are both none; out of the type's range is not taken.
        let none: Counts =
            serde_json::from_value(json!({ "count": 1, "code": null, "time": 0 })).unwrap();
        assert_eq!(none.code, None);
        let far = json!({ "count": 1, "code": 70000, "time": 0 });
        assert!(serde_json::from_value::<Counts>(far).is_err());
    }

    #[test]
    fn whole_doubles_settle_to_whole_numbers_and_nothing_else_moves() {
        let mut value =
            json!({ "a": 4.0, "b": [1.5, -2.0, 1e300], "c": { "d": 7, "e": "4.0", "f": null } });
        super::settle(&mut value);
        assert_eq!(
            value.to_string(),
            r#"{"a":4,"b":[1.5,-2,1e+300],"c":{"d":7,"e":"4.0","f":null}}"#
        );
    }
}
