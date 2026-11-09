//! Each rule's own vectors: the ones published with the design, and the
//! native client's tests for the Swift rules these supersede.

mod synthetic;

use myriad_perception_filter::{
    Ask, By, Classify, Decision, Disposition, IdentifierKind, Leaves, PageKind, Policy, Private,
    PrivateKind, Reason, Relay, RelayCheck, SecretKind, Sources, Window, Withheld, check_relay,
    decide, decide_with, identifiers, others, page, secrets, text, text_hits, window,
};

fn seen(title: &str) -> Window {
    window(Some("com.example.editor"), title, Sources::built_in())
}

#[test]
fn a_key_file_window_is_not_read_at_all() {
    for title in [
        ".env — MyriadNative",
        "vim .env.local",
        "prod.env — repo",
        "server.pem - Preview",
        "credentials — ~/.aws",
        "id_ed25519 (~/.ssh) — Visual Studio Code",
        "deploy/secrets.yaml",
        "~/.netrc",
        "app_credentials.json — Xcode",
    ] {
        assert_eq!(seen(title), Window::Withheld(Withheld::KeyFile), "{title}");
    }
    for title in [
        "README.md — MyriadNative",
        "environment setup notes",
        "Inbox (3) — Mail",
        "PLACE.md — docs",
        "Meeting notes — Notes",
        "keynote talk draft",
        "foo.environment notes",
        "keyboard-shortcuts.md",
    ] {
        assert!(matches!(seen(title), Window::Seen(_)), "{title}");
    }
}

#[test]
fn password_manager_windows_are_withheld_whatever_the_title() {
    for bundle in ["com.apple.keychainaccess", "com.apple.Passwords"] {
        assert_eq!(
            window(Some(bundle), "login", Sources::built_in()),
            Window::Withheld(Withheld::PasswordApp)
        );
    }
}

#[test]
fn titles_are_scrubbed_flattened_and_capped() {
    let (_, key) = &synthetic::named()[0];
    let Window::Seen(title) = window(
        Some("com.apple.Terminal"),
        &format!("zsh — export OPENAI={key}\n第二行"),
        Sources::built_in(),
    ) else {
        panic!("withheld");
    };
    assert!(!title.contains(key.as_str()) && !title.contains('\n'));
    assert_eq!(
        title,
        format!("zsh — export OPENAI={} 第二行", secrets::MARK)
    );
    let Window::Seen(long) = window(None, &"长".repeat(400), Sources::built_in()) else {
        panic!("withheld");
    };
    assert_eq!(long.chars().count(), window::TITLE_LIMIT + 1);
    assert!(long.ends_with('…'));
}

#[test]
fn every_known_prefix_and_high_entropy_string_is_wiped() {
    for (name, key) in synthetic::named() {
        if name == "pem" {
            continue;
        }
        let text = format!("cat .env 输出了 {key} 然后继续");
        let scan = secrets::scan(&text);
        assert!(!scan.text.contains(key.as_str()), "{name}");
        assert!(
            scan.text.contains(secrets::MARK) && !scan.wiped.is_empty(),
            "{name}"
        );
        assert!(
            scan.text.starts_with("cat .env 输出了 ") && scan.text.ends_with(" 然后继续"),
            "{name}"
        );
        let kind = if matches!(name, "hex" | "opaque") {
            SecretKind::HighEntropy
        } else {
            SecretKind::VendorPrefix
        };
        assert_eq!(scan.wiped, [kind], "{name}");
    }
    let pem = synthetic::named().pop().unwrap().1;
    let scan = secrets::scan(&format!("before\n{pem}\nafter"));
    assert!(!scan.text.contains("PRIVATE KEY"));
    assert!(scan.text.starts_with("before\n") && scan.text.ends_with("\nafter"));
    assert_eq!(scan.wiped, [SecretKind::PrivateKey]);
}

#[test]
fn the_label_stays_and_only_the_value_goes() {
    let value = synthetic::random(18, "abcdefghijklmnop", 21);
    let mark = secrets::MARK;
    for (raw, expected, kind) in [
        (
            format!("OPENAI_API_KEY={value}"),
            format!("OPENAI_API_KEY={mark}"),
            SecretKind::Assignment,
        ),
        (
            format!("\"password\": \"{value}\""),
            format!("\"password\": \"{mark}\""),
            SecretKind::Assignment,
        ),
        (
            format!("Authorization: Bearer {value}"),
            format!("Authorization: Bearer {mark}"),
            SecretKind::Bearer,
        ),
        (
            format!("export DB_SECRET='{value}'"),
            format!("export DB_SECRET='{mark}'"),
            SecretKind::Assignment,
        ),
    ] {
        let scan = secrets::scan(&raw);
        assert_eq!(scan.text, expected);
        assert_eq!(scan.wiped, [kind], "{raw}");
    }
}

/// `myriad-error`'s redaction blanks these names after `=`; the assignment
/// rule already takes each of them.
#[test]
fn the_backend_redaction_key_names_are_assignments_too() {
    let value = synthetic::random(18, "abcdefghijklmnop", 22);
    for name in [
        "password",
        "token",
        "access_token",
        "refresh_token",
        "client_secret",
        "api_key",
        "apikey",
        "setup_secret",
        "notion_token",
        "openai_api_key",
        "anthropic_api_key",
        "gemini_api_key",
        "app_secret",
        "qq_bot_app_secret",
        "telegram_bot_token",
        "discord_bot_token",
        "feishu_bot_app_secret",
        "clientSecret",
    ] {
        let scan = secrets::scan(&format!("{name}={value}"));
        assert_eq!(scan.text, format!("{name}={}", secrets::MARK), "{name}");
    }
    let scan = secrets::scan(&format!("authorization={value}"));
    assert_eq!(scan.text, format!("authorization={}", secrets::MARK));
}

#[test]
fn a_short_value_after_a_label_in_words_goes_and_the_label_stays() {
    let mark = secrets::MARK;
    let mixed = synthetic::random(6, "abcdefghijkmnpqrstuvwxyz", 51)
        + &synthetic::random(2, "23456789", 55);
    let letters = synthetic::random(8, "abcdefghijkmnpqrstuvwxyz", 52);
    let key = synthetic::random(30, "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz", 53);
    for (label, value) in [
        ("WiFi 密码 ", &letters),
        ("WiFi 密码：", &mixed),
        ("Wi-Fi パスワード ", &letters),
        ("パスワードは ", &mixed),
        ("WiFi password 是 ", &letters),
        ("wifi password ", &mixed),
        ("Passcode：", &letters),
        ("密钥：", &key),
        ("シークレット：", &key),
        ("secret key: ", &key),
        ("API 密钥：", &key),
        ("令牌 = ", &mixed),
    ] {
        let scan = secrets::scan(&format!("{label}{value}，记得改"));
        assert_eq!(scan.text, format!("{label}{mark}，记得改"), "{label}");
        assert_eq!(scan.wiped, [SecretKind::Labelled], "{label}");
    }
}

#[test]
fn a_label_in_prose_or_before_a_variable_takes_nothing() {
    let letters = synthetic::random(8, "abcdefghijkmnpqrstuvwxyz", 52);
    for text in [
        "Reset your password below",
        "The password is incorrect, try again",
        "password manager and password reset links",
        "密码是正确的，请登录",
        "パスワードを忘れた方はこちら",
        "the secret ingredient is love",
        "token 数量上限 4096",
        "--token $GITHUB_TOKEN",
        "Edit the token budget in settings",
    ] {
        let scan = secrets::scan(text);
        assert_eq!(scan.text, text);
        assert!(scan.wiped.is_empty(), "误抹：{text}");
    }
    // A password of letters alone after an English label and only a space
    // reads as a word: the model's to hold, not this scan's.
    let text = format!("wifi password {letters}");
    assert!(secrets::scan(&text).wiped.is_empty());
}

#[test]
fn a_long_random_run_with_few_digits_is_wiped_and_camel_case_is_not() {
    let random = synthetic::random(30, "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz", 54);
    for text in [
        random.clone(),
        format!("lmp_{random}"),
        format!("rvr-key-{random}"),
    ] {
        let scan = secrets::scan(&format!("用 {text} 登录"));
        assert_eq!(scan.wiped, [SecretKind::HighEntropy], "{}", text.len());
        assert!(!scan.text.contains(&random));
    }
    for text in [
        "consciousnessProductionAttentionSuspendsReadyLine",
        "XMLHttpRequestEventTargetPrototype",
        "AGENT_WATCH_DEFAULT_TIMEOUT_SECONDS",
    ] {
        assert!(secrets::scan(text).wiped.is_empty(), "误抹：{text}");
    }
}

#[test]
fn ordinary_code_text_is_left_alone() {
    for text in [
        "Edit Sources/MyriadCore/AgentWatch/SecretScrub.swift at line 42",
        "session 6F9619FF-8B86-D011-B42D-00C04FC964FF finished at 2026-10-19T02:09:16.694Z",
        "commit e473772 on branch claude/agent-watch-603d78",
        "consciousnessProductionAttentionSuspendsReadyLineAndResumesWithoutSecondModel",
        "max_output_tokens=4096 and max_tokens: 8192",
        "test_case_2026_10_19_agent_watch passes",
        "她在看你开着的几个 agent，没有截屏。",
        "",
    ] {
        let scan = secrets::scan(text);
        assert_eq!(scan.text, text);
        assert!(scan.wiped.is_empty(), "误抹：{text}");
    }
}

#[test]
fn scanning_twice_wipes_nothing_more() {
    let all: Vec<String> = synthetic::named().into_iter().map(|(_, key)| key).collect();
    let once = secrets::scan(&all.join(" ")).text;
    let twice = secrets::scan(&once);
    assert_eq!(twice.text, once);
    assert!(twice.wiped.is_empty());
}

fn wiped_kinds(text: &str) -> Vec<IdentifierKind> {
    identifiers::scan(text).wiped
}

#[test]
fn identifiers_with_a_right_check_digit_are_wiped() {
    use IdentifierKind::*;
    assert_eq!(wiped_kinds("卡号 4111 1111 1111 1111 有效期 12/28"), [Card]);
    assert_eq!(wiped_kinds("card 4111-1111-1111-1111."), [Card]);
    assert_eq!(wiped_kinds("4111111111111111"), [Card]);
    for nric in ["S1234567D", "T1234567J", "F1234567N", "G1234567X"] {
        assert_eq!(wiped_kinds(&format!("NRIC: {nric}")), [SgNric], "{nric}");
    }
    assert_eq!(wiped_kinds("マイナンバー 1234 5678 9018"), [JpMyNumber]);
    assert_eq!(wiped_kinds("1234-5678-9018"), [JpMyNumber]);
    assert_eq!(wiped_kinds("個人番号：123456789018"), [JpMyNumber]);
    let scan = identifiers::scan("卡 4111 1111 1111 1111，NRIC S1234567D。");
    assert_eq!(
        scan.text,
        format!("卡 {m}，NRIC {m}。", m = identifiers::MARK)
    );
    for (_, value) in synthetic::identifiers() {
        assert_eq!(identifiers::scan(&value).wiped.len(), 1, "{value}");
    }
}

#[test]
fn a_china_resident_id_with_its_date_and_check_character_is_wiped() {
    use IdentifierKind::*;
    for x in [false, true] {
        let id = synthetic::cn_resident_id(40, x);
        for text in [
            format!("身份证号 {id}"),
            format!("身份证号：{id}。"),
            format!("Resident ID {id}"),
            id.clone(),
        ] {
            let scan = identifiers::scan(&text);
            assert_eq!(scan.wiped, [CnResidentId], "x {x}");
            assert!(!scan.text.contains(&id) && scan.text.contains(identifiers::MARK));
        }
        // Lower-case `x` is the same character.
        assert_eq!(wiped_kinds(&id.to_lowercase()), [CnResidentId]);
        // A wrong check character, a date that does not exist, or glued to more.
        let wrong = format!("{}{}", &id[..17], if x { '0' } else { 'X' });
        let no_date = format!("{}0230{}", &id[..10], &id[14..]);
        for text in [wrong, no_date, format!("A{id}"), format!("{id}7")] {
            assert!(!wiped_kinds(&text).contains(&CnResidentId), "x {x}");
        }
    }
}

#[test]
fn a_plain_my_number_under_its_chinese_label_is_wiped() {
    let plain: String = synthetic::my_number()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    for label in ["个人编号 ", "个人番号：", "マイナンバー "] {
        assert_eq!(
            wiped_kinds(&format!("{label}{plain}")),
            [IdentifierKind::JpMyNumber],
            "{label}"
        );
    }
    // No label, no grouping: an order number with a right check digit stays.
    assert!(wiped_kinds(&format!("订单号 {plain}")).is_empty());
}

#[test]
fn a_passport_number_under_its_label_is_wiped() {
    let mark = identifiers::MARK;
    for (shape, number) in synthetic::passports(60) {
        for label in [
            "护照号码：",
            "护照号 ",
            "旅券番号　",
            "パスポート番号：",
            "Passport No. ",
            "passport no: ",
            "PASSPORT NUMBER ",
            "Passport #",
            "Passport 号码：",
            "护照号码（本人）：",
        ] {
            let text = format!("姓名 白川 葵，{label}{number}，有效期至 2031 年");
            let scan = identifiers::scan(&text);
            assert_eq!(scan.wiped, [IdentifierKind::Passport], "{shape} {label}");
            assert_eq!(
                scan.text,
                format!("姓名 白川 葵，{label}{mark}，有效期至 2031 年"),
                "{shape} {label}"
            );
        }
    }
    // Two on one line, each under its own label.
    let all = synthetic::passports(61);
    let text = format!("旧护照号码 {}，新护照号码 {}", all[4].1, all[2].1);
    assert_eq!(
        wiped_kinds(&text),
        [IdentifierKind::Passport, IdentifierKind::Passport]
    );
}

#[test]
fn a_passport_shape_without_its_label_stays() {
    for (shape, number) in synthetic::passports(62) {
        for text in [
            // The same shapes as order, booking and model numbers.
            format!("订单号 {number}"),
            format!("预订编号：{number}（航班 NH 0858）"),
            format!("型号 {number}，保修三年"),
            format!("Order #{number} has shipped"),
            format!("Flight booking ref {number}"),
            // A label on the line above: a heading, an article, a form laid
            // out in two lines.
            format!("护照办理指南\n办理时带好护照号码和照片。\n订单号 {number}"),
            format!("Passport No.\n{number}"),
            format!("旅券番号\r\n{number}"),
            // The label too far before it on the line.
            format!("护照号码请在下方填写，本页样例的订单号是 {number}"),
            // Glued to a letter or a digit: part of a longer code.
            format!("护照号码 {number}7"),
            format!("Passport No. {number}X"),
            format!("护照号码 A{number}"),
        ] {
            assert!(wiped_kinds(&text).is_empty(), "{shape}: {text}");
        }
    }
    // An article about applying for a passport, no number in it.
    for text in [
        "护照号码在资料页的右上角，办理签证时要填写。",
        "パスポート番号は旅券の顔写真ページに記載されています。",
        "How to find your passport number before you book.",
    ] {
        assert!(wiped_kinds(text).is_empty(), "{text}");
    }
    // Lower case is not how a passport number is printed.
    assert!(wiped_kinds("Passport No. tz1234567").is_empty());
}

#[test]
fn a_machine_readable_zone_line_whose_check_digits_are_right_is_wiped() {
    let mark = identifiers::MARK;
    // ICAO 9303's own specimen: the crate's check digits agree with it.
    let check = |data: &str| identifiers::mrz_check(&data.chars().collect::<Vec<_>>());
    assert_eq!(check("L898902C3"), Some(6));
    assert_eq!(check("740812"), Some(2));
    assert_eq!(check("120415"), Some(9));
    assert!(identifiers::mrz_line(
        "L898902C36UTO7408122F1204159ZE184226B<<<<<10"
    ));
    for (shape, number) in synthetic::passports(63) {
        let (first, second) = synthetic::mrz(&number, "900215", "310214");
        // No label needed; the first line (the name) is not a number.
        for text in [
            format!("{first}\n{second}"),
            format!("{first} {second}"),
            second.clone(),
        ] {
            let scan = identifiers::scan(&text);
            assert_eq!(scan.wiped, [IdentifierKind::Passport], "{shape}");
            assert!(
                !scan.text.contains(&number) && scan.text.ends_with(mark),
                "{shape}"
            );
        }
        // One character changed anywhere a check digit covers.
        for at in [0, 3, 8, 9, 13, 16, 19, 22, 25, 27, 30, 43] {
            let mut chars: Vec<char> = second.chars().collect();
            // Steps that move the value by one, never by a multiple of ten
            // (`<` to `A`, or `K` to `<`, would leave every check digit right).
            chars[at] = match chars[at] {
                '9' => '8',
                'Z' => 'Y',
                '<' => 'B',
                c => (c as u8 + 1) as char,
            };
            let changed: String = chars.into_iter().collect();
            assert!(
                wiped_kinds(&format!("{first}\n{changed}")).is_empty(),
                "{shape} at {at}"
            );
        }
        // A 45th character makes it something else.
        assert!(wiped_kinds(&format!("{second}<")).is_empty(), "{shape}");
    }
}

#[test]
fn a_well_formed_number_with_a_wrong_check_digit_stays() {
    for text in [
        "S1234567A",
        "4000 0012 3456 7890",
        "1234 5678 9012",
        // Twelve digits with the right check digit but no label and no grouping.
        "订单号 123456789018",
        "电话 138 0013 8000",
        "2026-10-19 12:00",
    ] {
        assert!(wiped_kinds(text).is_empty(), "{text}");
    }
}

#[test]
fn text_rules_hit_money_and_health_and_name_the_rule() {
    let rules = |text: &str| {
        text_hits(text)
            .iter()
            .map(|hit| hit.rule)
            .collect::<Vec<_>>()
    };
    assert_eq!(rules("可用余额：¥12,345.67"), [text::FINANCE_AMOUNT]);
    assert_eq!(rules("残高 1,234,567円"), [text::FINANCE_AMOUNT]);
    assert_eq!(
        rules("Statement balance S$1,204.50 due 3 Nov"),
        [text::FINANCE_AMOUNT]
    );
    assert_eq!(rules("HbA1c 6.4%（参考 4.6–6.2）"), [text::HEALTH_LAB]);
    assert_eq!(rules("LDL コレステロール 162 mg/dL"), [text::HEALTH_LAB]);
    assert_eq!(
        rules("処方：ロキソプロフェン 60mg 1日3回"),
        [text::HEALTH_PRESCRIPTION]
    );
    assert_eq!(rules("Diagnosis: mild asthma"), [text::HEALTH_FORM]);
    // v2: a holdings table with no currency, and an offer.
    assert_eq!(
        rules("份额 1,203.55 净值 1.2345 收益率 +3.21%"),
        [text::FINANCE_HOLDINGS]
    );
    assert_eq!(
        rules("Your base pay will be $8,500 per month"),
        [text::FINANCE_PAY]
    );
    assert_eq!(rules("内定のご連絡：年収 650万円"), [text::FINANCE_PAY]);
    // Deliberately narrow.
    for text in [
        "assert_eq!(account.balance(), 100);",
        "两万块的余额",
        "残高は30万くらい",
        "收益率 3.2%",
        "年収 650万円の求人",
    ] {
        assert!(text_hits(text).is_empty(), "{text}");
    }
}

#[test]
fn a_relay_that_carries_numbers_copies_wording_or_says_nothing_is_dropped() {
    let message = "下周三上午 10:30 去仁和医院复查，挂号单号 20261019 我已经发你了";
    assert_eq!(check_relay(message, "妈妈", "复查安排"), RelayCheck::Ok);
    assert_eq!(
        check_relay(message, "妈妈", "单号20261019"),
        RelayCheck::Digits
    );
    assert_eq!(
        check_relay(message, "妈妈", "去仁和医院复查"),
        RelayCheck::Copied
    );
    assert_eq!(check_relay(message, " ", "复查安排"), RelayCheck::Empty);
    assert_eq!(check_relay(message, "妈妈", ""), RelayCheck::Empty);
    // Full-width digits are digits.
    assert_eq!(
        check_relay("暗号は１２３４です", "Aki", "暗号1234"),
        RelayCheck::Digits
    );
    assert_eq!(
        check_relay(
            "Meet me at the old place on Friday night",
            "Sam",
            "meet at the old place"
        ),
        RelayCheck::Ok
    );
    assert_eq!(
        check_relay(
            "Meet me at the old place on Friday night",
            "Sam",
            "the old place on friday"
        ),
        RelayCheck::Copied
    );
}

fn open() -> Decision {
    Decision {
        capture: Some("seen".into()),
        window: Some("seen".into()),
        password_field: None,
        identifiers: Some(vec![]),
        source: Some(None),
        page: None,
        private: Some(Private::Known(false)),
        text_hits: Some(vec![]),
        model: Some("available".into()),
        language: Some("supported".into()),
        relay: Some(None),
        classify: Some(Some(Classify {
            general: Some("not_sensitive".into()),
            tagging: Some("not_sensitive".into()),
            kind: None,
        })),
        others_text: None,
    }
}

fn disposition(decision: &Decision) -> (Disposition, Reason) {
    let decided = decide(decision);
    (decided.disposition, decided.reason)
}

#[test]
fn the_published_boundaries() {
    use Disposition::*;
    assert_eq!(disposition(&open()), (Carry, Reason::Classify));
    // Nothing hit, model unavailable.
    let unavailable = Decision {
        model: Some("unavailable".into()),
        ..open()
    };
    assert_eq!(disposition(&unavailable), (Blind, Reason::ModelUnavailable));
    // A finance source needs no model.
    let finance = Decision {
        source: Some(Some("finance".into())),
        ..unavailable.clone()
    };
    assert_eq!(disposition(&finance), (PresentOnly, Reason::Source));
    // Messaging with no relay.
    let messaging = Decision {
        source: Some(Some("messaging".into())),
        ..open()
    };
    assert_eq!(disposition(&messaging), (Blind, Reason::Relay));
    // One clear answer and one error.
    let mut half = open();
    half.classify = Some(Some(Classify {
        general: Some("not_sensitive".into()),
        tagging: Some("error".into()),
        kind: None,
    }));
    assert_eq!(disposition(&half), (PresentOnly, Reason::Classify));
    // Not a web page, and nothing says whether it is private: the rules after it decide.
    let unknown = Decision {
        private: Some(Private::Other("unknown".into())),
        ..open()
    };
    assert_eq!(disposition(&unknown), (Carry, Reason::Classify));
    // A web page whose browser could not say: held as private, before the model.
    for page in ["ordinary", "unknown", "messaging"] {
        let browser = Decision {
            page: Some(page.into()),
            relay: Some(None),
            classify: Some(None),
            ..unknown.clone()
        };
        let decided = decide(&browser);
        assert_eq!(
            (decided.disposition, decided.reason, decided.kind),
            (
                PresentOnly,
                Reason::Private,
                Some(PrivateKind::PrivateBrowsing)
            ),
            "{page}"
        );
        assert_eq!(decided.missing, None, "{page}");
    }
    // A page the browser said is not private goes on as before.
    let said = Decision {
        page: Some("ordinary".into()),
        ..open()
    };
    assert_eq!(disposition(&said), (Carry, Reason::Classify));
}

#[test]
fn earlier_steps_win() {
    use Disposition::*;
    let all = Decision {
        capture: Some("excluded".into()),
        window: Some("withheld".into()),
        identifiers: Some(vec!["card".into()]),
        source: Some(Some("health".into())),
        private: Some(Private::Known(true)),
        text_hits: Some(vec!["finance".into()]),
        ..open()
    };
    assert_eq!(disposition(&all), (Exclude, Reason::Capture));
    let all = Decision {
        capture: Some("seen".into()),
        ..all
    };
    assert_eq!(disposition(&all), (Exclude, Reason::Window));
    let all = Decision {
        window: Some("seen".into()),
        ..all
    };
    let decided = decide(&all);
    assert_eq!(
        (decided.disposition, decided.kind),
        (PresentOnly, Some(PrivateKind::Identity))
    );
    let all = Decision {
        identifiers: Some(vec![]),
        ..all
    };
    assert_eq!(decide(&all).kind, Some(PrivateKind::Health));
    let all = Decision {
        source: Some(None),
        ..all
    };
    assert_eq!(decide(&all).kind, Some(PrivateKind::PrivateBrowsing));
    let all = Decision {
        private: Some(Private::Known(false)),
        ..all
    };
    assert_eq!(decide(&all).kind, Some(PrivateKind::Finance));
    assert_eq!(decide(&all).reason, Reason::Text);
    // A host rule is never overruled by the model, even a clear one.
    assert_eq!(decide(&all).disposition, PresentOnly);
}

#[test]
fn other_peoples_words_leave_only_as_who_and_what() {
    let relayed = |result: &str, category: &str| {
        decide(&Decision {
            source: Some(Some("messaging".into())),
            relay: Some(Some(Relay {
                result: result.into(),
                category: Some(category.into()),
            })),
            classify: Some(None),
            ..open()
        })
    };
    let ok = relayed("ok", "plans");
    assert_eq!(
        (ok.disposition, ok.leaves),
        (Disposition::WhoWhat, Leaves::WhoWhat)
    );
    assert_eq!(ok.leaves_kind.as_deref(), Some("plans"));
    assert_eq!(
        relayed("ok", "astrology").leaves_kind.as_deref(),
        Some("other")
    );
    assert_eq!(relayed("rejected", "plans").disposition, Disposition::Blind);
    assert_eq!(relayed("error", "plans").disposition, Disposition::Blind);
    assert_eq!(relayed("ok", "health").kind, Some(PrivateKind::Health));
    assert_eq!(relayed("ok", "money").kind, Some(PrivateKind::Finance));
    assert_eq!(relayed("ok", "money").leaves, Leaves::PrivateMarker);
}

fn messaging(category: &str, text: Option<&str>) -> Decision {
    Decision {
        source: Some(Some("messaging".into())),
        relay: Some(Some(Relay {
            result: "ok".into(),
            category: Some(category.into()),
        })),
        classify: Some(None),
        others_text: text.map(str::to_string),
        ..open()
    }
}

const LEAVE: Policy = Policy {
    others_words_leave: true,
    ..Policy::CURRENT
};

#[test]
fn others_words_stay_who_and_what_unless_the_policy_says() {
    const { assert!(!Policy::CURRENT.others_words_leave) };
    let text = "阿杰: 验证码 482913，明天 3 点见";
    let decided = decide(&messaging("plans", Some(text)));
    assert_eq!(
        (decided.disposition, decided.leaves, decided.others_text),
        (Disposition::WhoWhat, Leaves::WhoWhat, None)
    );
    // The text given or not, the default answer is the same.
    assert_eq!(
        decide(&messaging("plans", None)),
        decide(&messaging("plans", Some(text)))
    );
}

#[test]
fn with_the_policy_others_words_leave_scanned_masked_and_theirs() {
    let key = ["gh", "p_", "Zq8RkT3vNw7YbXc2Lm5Pd9Hs4Jf6Ga1Ke0Ut"].concat();
    let text = format!("阿杰: 验证码 482913，明天 3 点见\n小周: token {key} 电话 +65 9123 4567");
    let decided = decide_with(&messaging("plans", Some(&text)), &LEAVE);
    assert_eq!(decided.disposition, Disposition::OthersWords);
    assert_eq!(decided.leaves, Leaves::OthersText);
    assert_eq!(decided.reason, Reason::Relay);
    assert_eq!(decided.leaves_kind.as_deref(), Some("plans"));
    let words = decided.others_text.expect("others' words");
    assert_eq!(words.by, By::Others);
    assert_eq!(
        words.text,
        format!(
            "阿杰: 验证码 {m}，明天 3 点见\n小周: token {s} 电话 +{m}",
            m = others::MARK,
            s = secrets::MARK
        )
    );
    assert!(!words.text.contains(&key));
    let out = serde_json::to_value(decide_with(&messaging("plans", Some(&text)), &LEAVE)).unwrap();
    assert_eq!(out["othersText"]["by"], "others");
    assert_eq!(out["leaves"], "others_text");
    assert_eq!(out["disposition"], "others_words");
}

#[test]
fn with_the_policy_every_earlier_rule_still_holds() {
    let text = Some("明天下午见");
    // Health and money from the relay stay on this device.
    let health = decide_with(&messaging("health", text), &LEAVE);
    assert_eq!(
        (
            health.disposition,
            health.kind,
            health.leaves,
            health.others_text
        ),
        (
            Disposition::PresentOnly,
            Some(PrivateKind::Health),
            Leaves::PrivateMarker,
            None
        )
    );
    assert_eq!(
        decide_with(&messaging("money", text), &LEAVE).kind,
        Some(PrivateKind::Finance)
    );
    // A relay that failed: nothing.
    for result in ["error", "rejected"] {
        let mut d = messaging("plans", text);
        d.relay = Some(Some(Relay {
            result: result.into(),
            category: Some("plans".into()),
        }));
        let decided = decide_with(&d, &LEAVE);
        assert_eq!(
            (decided.disposition, decided.reason, decided.leaves),
            (Disposition::Blind, Reason::Relay, Leaves::Nothing)
        );
    }
    // The client's identifier scan and text rules come first.
    let d = Decision {
        identifiers: Some(vec!["card".into()]),
        ..messaging("plans", text)
    };
    assert_eq!(decide_with(&d, &LEAVE).reason, Reason::Identifier);
    let d = Decision {
        text_hits: Some(vec!["health".into()]),
        ..messaging("plans", text)
    };
    assert_eq!(decide_with(&d, &LEAVE).reason, Reason::Text);
    // What the client missed, the core's own scans catch in the words.
    let card = decide_with(
        &messaging("plans", Some("卡号 4111 1111 1111 1111 你先垫一下")),
        &LEAVE,
    );
    assert_eq!(
        (card.disposition, card.kind, card.others_text),
        (Disposition::PresentOnly, Some(PrivateKind::Identity), None)
    );
    let pay = decide_with(&messaging("plans", Some("这个月工资到手 8,500 元")), &LEAVE);
    assert_eq!(
        (pay.kind, pay.reason),
        (Some(PrivateKind::Finance), Reason::Text)
    );
    // A sensitive page leaves nothing at all.
    let d = Decision {
        page: Some("sensitive".into()),
        ..messaging("plans", text)
    };
    assert_eq!(decide_with(&d, &LEAVE).leaves, Leaves::Nothing);
    // Turned on but no words given: closed.
    let none = decide_with(&messaging("plans", None), &LEAVE);
    assert_eq!(
        (none.disposition, none.missing),
        (Disposition::Blind, Some("othersText"))
    );
}

/// No group or direct-message distinction exists: the same words leave the
/// same way from a group window and a one-to-one window.
#[test]
fn group_and_direct_windows_are_one_kind() {
    let group = "小周: 周五老地方\n阿杰: 订好了，包间 302\n小林: 我晚点到";
    let direct = "订好了，包间 302";
    for text in [group, direct] {
        let decided = decide_with(&messaging("plans", Some(text)), &LEAVE);
        assert_eq!(decided.disposition, Disposition::OthersWords, "{text}");
        assert!(decided.others_text.unwrap().text.contains(others::MARK));
    }
}

#[test]
fn digit_runs_are_masked_as_the_relay_counts_them() {
    for (text, masked) in [
        ("12 点", "12 点"),
        ("123", "[数字已抹去]"),
        ("1 2 3 走", "[数字已抹去] 走"),
        ("2026-11-03 见", "[数字已抹去] 见"),
        ("¥3,280.00。", "¥[数字已抹去]。"),
        ("房间１２０８号", "房间[数字已抹去]号"),
        ("第 1 和 2 条", "第 1 和 2 条"),
        ("v1.2", "v1.2"),
        ("code 7731.", "code [数字已抹去]."),
        ("三五七", "三五七"),
    ] {
        assert_eq!(others::mask_digits(text), masked, "{text}");
    }
}

#[test]
fn closed_by_default() {
    use Disposition::*;
    // Every input the steps need, taken away one at a time.
    for strip in 0..10 {
        let mut d = open();
        let field = match strip {
            0 => {
                d.capture = None;
                "capture"
            }
            1 => {
                d.window = None;
                "window"
            }
            2 => {
                d.identifiers = None;
                "identifiers"
            }
            3 => {
                d.source = None;
                "source"
            }
            4 => {
                d.private = None;
                "private"
            }
            5 => {
                d.text_hits = None;
                "textHits"
            }
            6 => {
                d.model = None;
                "model"
            }
            7 => {
                d.language = None;
                "language"
            }
            8 => {
                d.classify = None;
                "classify"
            }
            _ => {
                d.classify = Some(None);
                "classify"
            }
        };
        let decided = decide(&d);
        assert_eq!(
            (decided.disposition, decided.missing),
            (Blind, Some(field)),
            "{field}"
        );
        assert_eq!(decided.leaves, Leaves::Nothing);
    }
    // Values this core does not know read as the closed one.
    assert_eq!(
        disposition(&Decision {
            capture: Some("partial".into()),
            ..open()
        })
        .0,
        Exclude
    );
    assert_eq!(
        disposition(&Decision {
            window: Some("blurred".into()),
            ..open()
        })
        .0,
        Exclude
    );
    assert_eq!(
        disposition(&Decision {
            source: Some(Some("gambling".into())),
            ..open()
        })
        .0,
        PresentOnly
    );
    assert_eq!(
        disposition(&Decision {
            private: Some(Private::Other("maybe".into())),
            ..open()
        })
        .0,
        PresentOnly
    );
    assert_eq!(
        disposition(&Decision {
            text_hits: Some(vec!["romance".into()]),
            ..open()
        })
        .0,
        PresentOnly
    );
    assert_eq!(
        disposition(&Decision {
            model: Some("downloading".into()),
            ..open()
        })
        .0,
        Blind
    );
    assert_eq!(
        disposition(&Decision {
            language: Some("und".into()),
            ..open()
        })
        .0,
        Blind
    );
    let mut unsure = open();
    unsure.classify = Some(Some(Classify {
        general: Some("probably_fine".into()),
        tagging: Some("not_sensitive".into()),
        kind: None,
    }));
    assert_eq!(disposition(&unsure).0, PresentOnly);
    assert_eq!(decide(&unsure).kind, Some(PrivateKind::Unknown));
}

/// The two open owner questions: today the more closed answer of each, and
/// either other answer is one constant.
#[test]
fn the_open_policies() {
    let mut once = open();
    once.classify = Some(Some(Classify {
        general: Some("not_sensitive".into()),
        tagging: None,
        kind: None,
    }));
    assert_eq!(decide(&once).disposition, Disposition::PresentOnly);
    let ask_once = Policy {
        classify_asks: &[Ask::General],
        ..Policy::CURRENT
    };
    assert_eq!(
        decide_with(&once, &ask_once).disposition,
        Disposition::Carry
    );

    // A health text rule: a sensitive source would leave nothing whatever the policy.
    let health = Decision {
        text_hits: Some(vec!["health".into()]),
        ..open()
    };
    assert_eq!(decide(&health).leaves_kind, None);
    let with_kind = Policy {
        present_only_leaves_kind: true,
        ..Policy::CURRENT
    };
    assert_eq!(
        decide_with(&health, &with_kind).leaves_kind.as_deref(),
        Some("health")
    );
}

#[test]
fn a_sensitive_source_or_page_leaves_nothing_whatever_the_policy() {
    let with_kind = Policy {
        present_only_leaves_kind: true,
        ..Policy::CURRENT
    };
    for decision in [
        Decision {
            source: Some(Some("health".into())),
            ..open()
        },
        Decision {
            source: Some(Some("adult".into())),
            ..open()
        },
        Decision {
            source: Some(Some("gambling".into())),
            ..open()
        },
        Decision {
            page: Some("account".into()),
            ..open()
        },
        Decision {
            page: Some("sensitive".into()),
            ..open()
        },
        // A value this core does not know reads as unknown.
        Decision {
            page: Some("forum".into()),
            ..open()
        },
        // Even when an identifier is what put it there.
        Decision {
            identifiers: Some(vec!["card".into()]),
            page: Some("sensitive".into()),
            ..open()
        },
    ] {
        for policy in [Policy::CURRENT, with_kind] {
            let decided = decide_with(&decision, &policy);
            assert_eq!(
                decided.disposition,
                Disposition::PresentOnly,
                "{decision:?}"
            );
            assert_eq!(
                (decided.leaves, decided.leaves_kind),
                (Leaves::Nothing, None),
                "{decision:?}"
            );
        }
    }
    // An ordinary page goes on to the model; a private window still leaves the marker.
    assert_eq!(
        disposition(&Decision {
            page: Some("ordinary".into()),
            ..open()
        })
        .0,
        Disposition::Carry
    );
    // A page no rule places is not held for that alone (a block list): it goes on like an ordinary
    // one, and what its text holds is still held by the rules after the page step.
    assert_eq!(
        disposition(&Decision {
            page: Some("unknown".into()),
            ..open()
        })
        .0,
        Disposition::Carry
    );
    let card = decide(&Decision {
        identifiers: Some(vec!["card".into()]),
        page: Some("unknown".into()),
        ..open()
    });
    assert_eq!(card.disposition, Disposition::PresentOnly);
    assert_eq!(
        card.leaves,
        Leaves::PrivateMarker,
        "only a sensitive page says nothing at all"
    );
    let private = decide(&Decision {
        private: Some(Private::Known(true)),
        ..open()
    });
    assert_eq!(private.leaves, Leaves::PrivateMarker);
}

#[test]
fn a_showing_password_field_is_not_read() {
    let field = decide(&Decision {
        password_field: Some(true),
        ..open()
    });
    assert_eq!(
        (field.disposition, field.reason),
        (Disposition::Exclude, Reason::PasswordField)
    );
    assert_eq!(field.leaves, Leaves::Nothing);
    let sign_in = decide(&Decision {
        page: Some("sign_in".into()),
        ..open()
    });
    assert_eq!(sign_in.disposition, Disposition::Exclude);
    // Not given, or false: not a reason by itself.
    assert_eq!(
        disposition(&Decision {
            password_field: Some(false),
            ..open()
        })
        .0,
        Disposition::Carry
    );
    // Excluded before anything else is needed.
    let bare = Decision {
        password_field: Some(true),
        capture: Some("seen".into()),
        window: Some("seen".into()),
        ..Decision::default()
    };
    assert_eq!(decide(&bare).disposition, Disposition::Exclude);
}

#[test]
fn the_built_in_table_reads_everyday_sites_and_holds_the_rest() {
    let at = |host: &str, path: Option<&str>| {
        let placed = page(host, path, None, false, None, Sources::built_in());
        (placed.kind, placed.read())
    };
    for host in [
        "www.youtube.com",
        "www.bilibili.com",
        "www.nicovideo.jp",
        "www.bbc.com",
        "www.nhk.or.jp",
        "www.straitstimes.com",
        "en.wikipedia.org",
        "ja.wikipedia.org",
        "developer.mozilla.org",
        "docs.rs",
        "github.com",
        "stackoverflow.com",
        "www.amazon.co.jp",
        "item.taobao.com",
        "shopee.sg",
        "store.steampowered.com",
        "tenki.jp",
        "docs.swift.org",
    ] {
        assert_eq!(at(host, Some("/")), (PageKind::Ordinary, true), "{host}");
    }
    for (host, path, kind) in [
        ("github.com", "/settings/security", PageKind::Account),
        (
            "www.amazon.co.jp",
            "/gp/your-account/order-history",
            PageKind::Account,
        ),
        ("www.youtube.com", "/account", PageKind::Account),
        ("accounts.google.com", "/", PageKind::Sensitive),
        ("www.dbs.com.sg", "/", PageKind::Sensitive),
        ("www.singpass.gov.sg", "/", PageKind::Sensitive),
        ("myna.go.jp", "/", PageKind::Sensitive),
        // Not in the table, but the host says what it is for: held.
        ("www.examplebank.bank", "/", PageKind::Sensitive),
        ("online-banking.example.com", "/", PageKind::Sensitive),
        ("my.city-hospital.example", "/", PageKind::Sensitive),
        ("tax.example.gov", "/", PageKind::Sensitive),
        ("www.example.go.jp", "/", PageKind::Sensitive),
        ("dating.example.net", "/", PageKind::Sensitive),
        // The stem inside a longer name counts too.
        ("www.mybank.example", "/", PageKind::Sensitive),
        ("www.bankofamerica.example", "/", PageKind::Sensitive),
        ("www.healthline.example", "/", PageKind::Sensitive),
        ("secure.someinsurer.example", "/", PageKind::Sensitive),
        ("www.webmedical.example", "/", PageKind::Sensitive),
        // An unknown site's account page is still an account page.
        ("some-blog.example", "/settings", PageKind::Account),
    ] {
        assert_eq!(at(host, Some(path)), (kind, false), "{host}{path}");
    }
    // A site no rule places, and nothing in its name says it is sensitive: read (a block list).
    for host in [
        "some-blog.example",
        "news.example.co.jp",
        "matcha-recipes.example",
        "lawn-care.example",
        // Short words only count as a whole label, not inside other words.
        "syntax-highlight.example",
        "flawless.example",
        "alphabet.example",
    ] {
        assert_eq!(at(host, Some("/")), (PageKind::Unknown, true), "{host}");
    }
    assert_eq!(at("mail.google.com", None), (PageKind::Messaging, true));
}

#[test]
fn account_paths_need_an_account_word() {
    for path in [
        "/settings",
        "/user/settings/security",
        "/my-account",
        "/account/billing",
        "/checkout?step=2",
        "/sign-in",
        "/two-factor",
        "/mypage/",
        "/api-keys",
    ] {
        assert!(myriad_perception_filter::page::account_path(path), "{path}");
    }
    for path in [
        "/",
        "/docs/security",
        "/wiki/Privacy",
        "/profile/someone",
        "/watch?v=settings",
        "/blog/2026/10/accounting-basics",
    ] {
        assert!(
            !myriad_perception_filter::page::account_path(path),
            "{path}"
        );
    }
}
