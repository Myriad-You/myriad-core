//! Synthetic keys and numbers, built at run time from a fixed seed: the same
//! values the native client's tests and on-device probe expand the corpus
//! placeholders into. No key-shaped literal is written in this repository;
//! every prefix is spliced.

#![allow(dead_code)]

const ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

pub fn random(count: usize, alphabet: &str, seed: u64) -> String {
    let chars: Vec<char> = alphabet.chars().collect();
    let step = |state: u64| {
        state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };
    let mut state = step(seed);
    (0..count)
        .map(|_| {
            state = step(state);
            chars[((state >> 33) % chars.len() as u64) as usize]
        })
        .collect()
}

fn r(count: usize, seed: u64) -> String {
    random(count, ALPHABET, seed)
}

pub fn named() -> Vec<(&'static str, String)> {
    let pem = |kind: &str| format!("-----{kind} RSA PRIVATE KEY-----");
    vec![
        ("openai", ["s", "k-", "proj-", &r(40, 1)].concat()),
        (
            "anthropic",
            ["s", "k-", "ant-", "api03-", &r(60, 2)].concat(),
        ),
        ("github", ["gh", "p_", &r(36, 3)].concat()),
        ("github_fine", ["github", "_pat_", &r(60, 4)].concat()),
        (
            "aws",
            [
                "AK",
                "IA",
                &random(16, "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567", 5),
            ]
            .concat(),
        ),
        (
            "slack",
            ["xo", "xb-", &random(12, "0123456789", 6), "-", &r(24, 7)].concat(),
        ),
        ("google", ["AI", "za", &r(35, 8)].concat()),
        ("stripe", ["s", "k_", "live_", &r(24, 9)].concat()),
        (
            "jwt",
            [
                "ey",
                "J",
                &r(20, 10),
                ".ey",
                "J",
                &r(30, 11),
                ".",
                &r(30, 12),
            ]
            .concat(),
        ),
        ("hex", random(48, "0123456789abcdef", 13)),
        ("opaque", r(32, 14)),
        (
            "pem",
            format!(
                "{}\n{}\n{}\n{}",
                pem(&["BEG", "IN"].concat()),
                r(64, 15),
                r(64, 16),
                pem("END")
            ),
        ),
    ]
}

fn digits(count: usize, seed: u64) -> Vec<u32> {
    random(count, "0123456789", seed)
        .chars()
        .map(|c| c.to_digit(10).unwrap())
        .collect()
}

fn groups_of_four(digits: &[u32]) -> String {
    digits
        .chunks(4)
        .map(|group| group.iter().map(u32::to_string).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A card on the 4000 00 test range with its Luhn digit, in groups of four.
pub fn card() -> String {
    let mut all = vec![4, 0, 0, 0, 0, 0];
    all.extend(digits(9, 31));
    let sum: u32 = all
        .iter()
        .rev()
        .enumerate()
        .map(|(offset, &digit)| {
            let doubled = if offset % 2 == 0 { digit * 2 } else { digit };
            if doubled > 9 { doubled - 9 } else { doubled }
        })
        .sum();
    all.push((10 - sum % 10) % 10);
    groups_of_four(&all)
}

/// A Singapore NRIC, S series, with its check letter.
pub fn nric() -> String {
    let body = digits(7, 32);
    let sum: u32 = body
        .iter()
        .zip([2, 7, 6, 5, 4, 3, 2])
        .map(|(d, w)| d * w)
        .sum();
    let letter = b"JZIHGFEDCBA"[(sum % 11) as usize] as char;
    format!(
        "S{}{letter}",
        body.iter().map(u32::to_string).collect::<String>()
    )
}

/// A Japanese individual number with its check digit, in groups of four.
pub fn my_number() -> String {
    let mut body = digits(11, 33);
    let sum: u32 = body
        .iter()
        .rev()
        .enumerate()
        .map(|(index, &digit)| {
            digit
                * if index < 6 {
                    index as u32 + 2
                } else {
                    index as u32 - 4
                }
        })
        .sum();
    body.push(if sum % 11 <= 1 { 0 } else { 11 - sum % 11 });
    groups_of_four(&body)
}

/// A China resident identity number from a seed: a made-up area and
/// sequence, a real birth date and its ISO 7064 check character. Seeds are
/// tried from `seed` on until the check character is `X` when `x` says so,
/// a digit otherwise.
pub fn cn_resident_id(seed: u64, x: bool) -> String {
    for seed in seed.. {
        let mut body: Vec<u32> = "110101".chars().map(|c| c.to_digit(10).unwrap()).collect();
        body.extend("19900215".chars().map(|c| c.to_digit(10).unwrap()));
        body.extend(digits(3, seed));
        let sum: u32 = body
            .iter()
            .zip([7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2])
            .map(|(d, w)| d * w)
            .sum();
        let check = b"10X98765432"[(sum % 11) as usize] as char;
        if (check == 'X') == x {
            return format!(
                "{}{check}",
                body.iter().map(u32::to_string).collect::<String>()
            );
        }
    }
    unreachable!()
}

/// Passport numbers from a seed, one per shape: Japan (two letters, 7
/// digits), Singapore (letter, 7 digits, letter), China (`E` and 8 digits,
/// `E`, a letter and 7 digits, the older `G` and 8 digits). Made up: no check
/// digit to get right.
pub fn passports(seed: u64) -> Vec<(&'static str, String)> {
    let d = |count: usize, offset: u64| random(count, "0123456789", seed + offset);
    let letter = |offset: u64| random(1, "ABCDEFGHJKLMNPRSTUVWXYZ", seed + offset);
    vec![
        ("jp", format!("TZ{}", d(7, 1))),
        ("sg", format!("K{}{}", d(7, 2), letter(3))),
        ("cn", format!("E{}", d(8, 4))),
        ("cn_letter", format!("E{}{}", letter(5), d(7, 6))),
        ("cn_old", format!("G{}", d(8, 7))),
    ]
}

/// The ICAO 9303 check digit, written out apart from the crate's.
pub fn mrz_check(data: &str) -> char {
    let sum: u32 = data
        .chars()
        .zip([7, 3, 1].into_iter().cycle())
        .map(|(c, w)| {
            w * match c {
                '<' => 0,
                c if c.is_ascii_digit() => c.to_digit(10).unwrap(),
                c => c as u32 - 'A' as u32 + 10,
            }
        })
        .sum();
    char::from_digit(sum % 10, 10).unwrap()
}

/// A passport's machine readable zone (TD3), both lines, for a made-up
/// holder: the second line's check digits all right.
pub fn mrz(number: &str, birth: &str, expiry: &str) -> (String, String) {
    let first = format!("{:<<44}", "P<JPNSHIRAKAWA<<AOI");
    let number = format!("{number:<<9}");
    let optional = "<".repeat(14);
    let mut second = format!(
        "{number}{}JPN{birth}{}F{expiry}{}{optional}{}",
        mrz_check(&number),
        mrz_check(birth),
        mrz_check(expiry),
        mrz_check(&optional),
    );
    let whole = format!("{}{}{}", &second[..10], &second[13..20], &second[21..43]);
    second.push(mrz_check(&whole));
    (first, second)
}

pub fn identifiers() -> Vec<(&'static str, String)> {
    vec![
        ("card", card()),
        ("nric", nric()),
        ("mynumber", my_number()),
    ]
}

/// The text with every placeholder expanded, and the keys and numbers put in.
pub struct Expanded {
    pub text: String,
    pub keys: Vec<String>,
    pub ids: Vec<String>,
}

pub fn expand(text: &str) -> Expanded {
    let mut out = text.to_string();
    let (mut keys, mut ids) = (Vec::new(), Vec::new());
    for (name, value) in named() {
        let placeholder = format!("{{{{key:{name}}}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &value);
            keys.push(value);
        }
    }
    for (name, value) in identifiers() {
        let placeholder = format!("{{{{id:{name}}}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &value);
            ids.push(value);
        }
    }
    Expanded {
        text: out,
        keys,
        ids,
    }
}
