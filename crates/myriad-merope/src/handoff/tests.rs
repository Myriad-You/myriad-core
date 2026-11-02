use super::*;

fn new(text: &str) -> HandOff {
    HandOff::New { text: text.into() }
}

fn piece(number: u32, standing: Standing) -> Piece {
    Piece {
        number,
        asked: format!("第{number}件"),
        started_at: Some(0),
        ended_at: None,
        standing,
    }
}

fn waiting(number: u32) -> Piece {
    piece(
        number,
        Standing::Waiting {
            question: "要几点？".into(),
        },
    )
}

fn doing(number: u32) -> Piece {
    piece(
        number,
        Standing::Doing {
            progress: None,
            steps: None,
        },
    )
}

const RESTART: Standing = Standing::CutOff {
    by: CutOffBy::Restart,
};

#[test]
fn her_hand_off_line_is_taken_out_of_what_she_says() {
    assert_eq!(
        split("好，我让助手去查。\n[[work: 查一下明天东京的天气]]"),
        (
            "好，我让助手去查。".into(),
            vec![new("查一下明天东京的天气")]
        )
    );
    assert_eq!(split("今天好累"), ("今天好累".into(), vec![]));
    assert_eq!(split("嗯[[work:   ]]"), ("嗯".into(), vec![]));
    // An unfinished line is left for the stream to hold back.
    assert_eq!(split("嗯[[work: 查"), ("嗯[[work: 查".into(), vec![]));
}

#[test]
fn a_line_for_one_piece_goes_to_that_piece() {
    assert_eq!(
        split("好，改成周六。\n[[work #2: 改成周六，原话：还是周六吧]]\n[[stop #1]]"),
        (
            "好，改成周六。".into(),
            vec![
                HandOff::To {
                    work: 2,
                    text: "改成周六，原话：还是周六吧".into()
                },
                HandOff::Stop { work: 1 },
            ]
        )
    );
    // Written loosely, it is still hers.
    assert_eq!(
        split("[[work 3：是]][[stop＃4]]").1,
        vec![
            HandOff::To {
                work: 3,
                text: "是".into()
            },
            HandOff::Stop { work: 4 }
        ]
    );
    // Hers written wrong goes unsaid, whatever the case.
    assert_eq!(
        split("好[[work 订餐厅]][[Work: 查]]"),
        ("好".into(), vec![new("查")])
    );
    // One on a line of its own goes with that line.
    assert_eq!(
        split("好的\n[[work: 查天气]]\n明天告诉你"),
        ("好的\n明天告诉你".into(), vec![new("查天气")])
    );
    // With spaces around it, or CRLF, the line still goes.
    assert_eq!(
        split("好的\n  [[work: 查天气]]  \n明天告诉你").0,
        "好的\n明天告诉你"
    );
    assert_eq!(
        split("好的\r\n[[work: 查天气]]\r\n明天告诉你").0,
        "好的\r\n明天告诉你"
    );
    assert_eq!(split("好的\r\n[[work: 查天气]] ").0, "好的");
    assert_eq!(split(" [[work: 查天气]] ").0, "");
    // A line with words, or another kind of line, on it stays.
    assert_eq!(
        split("好的\n先这样 [[work: 查天气]]\n明天").0,
        "好的\n先这样 \n明天"
    );
    // The same new piece twice in one reply is one.
    assert_eq!(split("[[work: 查]][[work: 查]]").1, vec![new("查")]);
    // What she tells a piece goes each time: the last word is the last.
    let to = |text: &str| HandOff::To {
        work: 1,
        text: text.into(),
    };
    assert_eq!(
        split("[[work #1: 按价格]][[work #1: 按距离]][[work #1: 按价格]]").1,
        vec![to("按价格"), to("按距离"), to("按价格")]
    );
    // A piece without a number is not one; it says nothing.
    assert_eq!(split("嗯[[stop]][[work #x: 查]]"), ("嗯".into(), vec![]));
    // A word that only begins like hers is not hers.
    assert_eq!(
        split("[[workers]]\n[[stopwatch]] [[work 买菜]]"),
        ("[[workers]]\n[[stopwatch]]".into(), vec![])
    );
    // Another kind of line is left where it is.
    assert_eq!(
        split("好呀[[sticker: 开心]]\n[[work: 查]]"),
        ("好呀[[sticker: 开心]]".into(), vec![new("查")])
    );
}

#[test]
fn what_goes_to_an_end_says_which_piece() {
    assert_eq!(
        serde_json::to_value(split("[[work #2: 是]][[stop #3]][[work: 查]]").1).unwrap(),
        serde_json::json!([
            {"kind": "to", "work": 2, "text": "是"},
            {"kind": "stop", "work": 3},
            {"kind": "new", "text": "查"},
        ])
    );
}

#[test]
fn she_hands_off_only_what_they_asked_for() {
    let quiet = section(Host::Site, &[], 0, None);
    assert!(quiet.contains("If it is your own idea, ask first"));
    assert!(quiet.contains("with their permissions"));
    assert!(quiet.contains("[[stop #N]]"));
    assert!(quiet.contains("never for a new one"));
    assert!(quiet.contains("working with their data on this site"));
    assert!(quiet.contains("cut off by a restart"));
    assert!(!quiet.contains("handed_off"));
    assert!(!quiet.contains("waiting for their answer"));
    let busy = section(
        Host::Mac,
        &[
            "#1「查天气」在办，已办 2 分钟（40%）".into(),
            "#2「订餐厅」在等对方回答：要几点？".into(),
            "#3「订票」在等对方回答：哪天？".into(),
        ],
        2,
        Some(2),
    );
    assert!(busy.contains("working with files and apps on this Mac"));
    assert!(busy.contains("cut off when the app quit"));
    assert!(busy.contains("Numbers are given by the app"));
    assert!(busy.contains("handed_off"));
    assert!(busy.contains("#2「订餐厅」"));
    assert!(busy.contains("2 pieces are waiting for their answer"));
    assert!(busy.ends_with("Their words just now went to #2 as its answer."));
}

#[test]
fn a_piece_stands_as_what_is_known_of_it() {
    let facts = |edit: fn(&mut Facts)| {
        let mut facts = Facts::default();
        edit(&mut facts);
        stands(&facts)
    };
    assert_eq!(facts(|_| {}), Standing::Done { result: None });
    assert_eq!(
        facts(|f| {
            f.under_way = true;
            f.progress = Some(40);
        }),
        Standing::Doing {
            progress: Some(40),
            steps: None
        }
    );
    // A restart left its run waiting on a question nobody saw.
    assert_eq!(
        facts(|f| {
            f.under_way = true;
            f.unheard_question = true;
        }),
        RESTART
    );
    assert_eq!(
        facts(|f| {
            f.unheard_question = true;
            f.question = Some("要几点？".into());
        }),
        Standing::Waiting {
            question: "要几点？".into()
        }
    );
    assert_eq!(
        facts(|f| {
            f.stopped = true;
            f.under_way = true;
        }),
        Standing::Stopped
    );
    // Its run never began: not done, whatever was said in its session.
    assert_eq!(facts(|f| f.begun = Begun::NotYet), Standing::Starting);
    assert_eq!(
        facts(|f| f.begun = Begun::Never),
        Standing::Failed {
            why: Some(NEVER_BEGAN.into())
        }
    );
    assert_eq!(
        facts(|f| {
            f.begun = Begun::Never;
            f.under_way = true;
        }),
        Standing::Doing {
            progress: None,
            steps: None
        }
    );
}

#[test]
fn the_next_number_is_one_past_the_highest() {
    assert_eq!(next_number([]), Some(1));
    assert_eq!(next_number([1, 3, 2]), Some(4));
    assert_eq!(next_number([u32::MAX]), None);
}

#[test]
fn the_one_waiting_takes_the_answer_and_more_are_hers_to_place() {
    assert_eq!(route(&[]), Route::Her);
    assert_eq!(route(&[doing(1), doing(2)]), Route::Her);
    assert_eq!(route(&[doing(1), waiting(2)]), Route::Answer { work: 2 });
    assert_eq!(route(&[waiting(1), waiting(2)]), Route::Her);
    // A card waiting to be confirmed is not answered in chat.
    assert_eq!(
        route(&[piece(1, Standing::Confirming), waiting(2)]),
        Route::Answer { work: 2 }
    );
}

#[test]
fn her_line_goes_to_the_piece_as_it_stands() {
    let pieces = [
        doing(1),
        waiting(2),
        piece(3, Standing::Done { result: None }),
        piece(4, Standing::Stopped),
        piece(5, RESTART),
        piece(6, Standing::Failed { why: None }),
        piece(7, Standing::Starting),
        piece(8, Standing::Confirming),
    ];
    let to = |work, text: &str| HandOff::To {
        work,
        text: text.into(),
    };
    let text = |text: &str| text.to_string();
    assert_eq!(
        act(&to(1, "改成周六"), &pieces),
        Act::Steer {
            work: 1,
            text: text("改成周六")
        }
    );
    assert_eq!(
        act(&to(2, "是"), &pieces),
        Act::Answer {
            work: 2,
            text: text("是")
        }
    );
    assert_eq!(
        act(&to(3, "再查一次"), &pieces),
        Act::Continue {
            work: 3,
            text: text("再查一次")
        }
    );
    assert_eq!(act(&to(4, "接着办"), &pieces), Act::Nothing);
    // A restart cut it off: it goes on from where it was saved.
    assert_eq!(
        act(&to(5, "接着办"), &pieces),
        Act::PickUp {
            work: 5,
            text: text("接着办")
        }
    );
    assert_eq!(
        act(&to(6, "再试一次"), &pieces),
        Act::Continue {
            work: 6,
            text: text("再试一次")
        }
    );
    assert_eq!(
        act(&to(7, "顺便"), &pieces),
        Act::Steer {
            work: 7,
            text: text("顺便")
        }
    );
    assert_eq!(
        act(&to(8, "顺便"), &pieces),
        Act::Steer {
            work: 8,
            text: text("顺便")
        }
    );
    assert_eq!(act(&to(9, "嗯"), &pieces), Act::Nothing);
    for work in [1, 2, 5, 7, 8] {
        assert_eq!(act(&HandOff::Stop { work }, &pieces), Act::Stop { work });
    }
    for work in [3, 4, 6, 9] {
        assert_eq!(act(&HandOff::Stop { work }, &pieces), Act::Nothing);
    }
    assert_eq!(act(&new("查"), &pieces), Act::Start { text: text("查") });
}

#[test]
fn she_sees_each_piece_on_a_line() {
    let now = 3 * 60 * 60;
    let mut running = doing(2);
    running.asked = "查明天东京天气".into();
    running.started_at = Some(now - 200);
    running.standing = Standing::Doing {
        progress: Some(60),
        steps: None,
    };
    assert_eq!(
        line(&running, now),
        "#2「查明天东京天气」在办，已办 3 分钟（60%）"
    );
    running.standing = Standing::Doing {
        progress: None,
        steps: Some(4),
    };
    assert_eq!(
        line(&running, now),
        "#2「查明天东京天气」在办，已办 3 分钟，第 4 步"
    );
    let mut asking = waiting(3);
    asking.asked = "订周五的餐厅".into();
    asking.standing = Standing::Waiting {
        question: "要几点？\n\n1. 18:00\n2. 19:00".into(),
    };
    assert_eq!(
        line(&asking, now),
        "#3「订周五的餐厅」在等对方回答：要几点？ 1. 18:00 2. 19:00"
    );
    let long = Standing::Waiting {
        question: "问".repeat(RESULT_CHARS + 5),
    };
    let asked = line(&piece(3, long), now);
    assert!(asked.ends_with('…'));
    let back = piece(
        1,
        Standing::Done {
            result: Some("周报整理好了".into()),
        },
    );
    assert_eq!(line(&back, now), "#1「第1件」办完了：周报整理好了");
    assert_eq!(
        line(&piece(4, Standing::Stopped), now),
        "#4「第4件」被叫停了"
    );
    assert_eq!(
        line(&piece(5, RESTART), now),
        "#5「第5件」被打断了（重启），还没办完"
    );
    let quit = Standing::CutOff {
        by: CutOffBy::AppQuit,
    };
    assert_eq!(
        line(&piece(6, quit), now),
        "#6「第6件」被打断了（应用退出），还没办完"
    );
    assert_eq!(
        line(&piece(7, Standing::Starting), now),
        "#7「第7件」刚交出去，还没开始"
    );
    assert_eq!(
        line(&piece(8, Standing::Confirming), now),
        "#8「第8件」在等对方在卡片上确认"
    );
    let failed = Standing::Failed {
        why: Some("没有权限".into()),
    };
    assert_eq!(line(&piece(9, failed), now), "#9「第9件」没办成：没有权限");
    running.started_at = Some(now - 2 * 60 * 60 - 5 * 60);
    assert!(line(&running, now).contains("已办 2 小时 5 分钟"));
}

#[test]
fn she_sees_every_unfinished_piece_and_the_last_few_over() {
    let now = 2 * DONE_FOR_SECS;
    let done = |number, at| Piece {
        started_at: Some(at),
        ..piece(number, Standing::Done { result: None })
    };
    let pieces = vec![
        done(1, 0),
        done(2, now - 10),
        done(3, now - 9),
        done(4, now - 8),
        Piece {
            started_at: Some(now - 7),
            ..piece(5, Standing::Failed { why: None })
        },
        Piece {
            started_at: Some(0),
            ..doing(6)
        },
        waiting(7),
        piece(8, RESTART),
    ];
    let shown: Vec<u32> = shown(pieces, now)
        .iter()
        .map(|piece| piece.number)
        .collect();
    assert_eq!(shown, [3, 4, 5, 6, 7, 8]);
}

#[test]
fn one_that_just_came_back_is_seen_however_long_ago_it_began() {
    let now = 10 * DONE_FOR_SECS;
    let over = |number, started: i64, ended: Option<i64>| Piece {
        started_at: Some(started),
        ended_at: ended,
        ..piece(number, Standing::Done { result: None })
    };
    let pieces = vec![
        // Begun two days ago, back a minute ago.
        over(1, now - 2 * DONE_FOR_SECS, Some(now - 60)),
        over(2, now - 500, Some(now - 400)),
        over(3, now - 300, Some(now - 200)),
        over(4, now - 100, Some(now - 90)),
        // Back more than a day ago.
        over(5, now - 3 * DONE_FOR_SECS, Some(now - 2 * DONE_FOR_SECS)),
    ];
    let shown: Vec<u32> = shown(pieces, now)
        .iter()
        .map(|piece| piece.number)
        .collect();
    assert_eq!(shown, [1, 3, 4]);
}

#[test]
fn what_a_piece_sends_says_which_when_another_is_not_done() {
    assert_eq!(
        label(2, "查一下明天东京的天气怎么样"),
        "【#2 查一下明天东京的天气怎么…】"
    );
    assert_eq!(label(1, "查天气"), "【#1 查天气】");
    assert_eq!(labelled(1, "查天气", false, "晴"), "晴");
    assert_eq!(labelled(1, "查天气", true, "晴"), "【#1 查天气】\n晴");
}

#[test]
fn her_answer_to_a_choice_is_the_choice_she_means() {
    let options = ["18:00", "19:00"]
        .map(|time| PendingOption {
            value: time.into(),
            label: time.into(),
        })
        .to_vec();
    assert_eq!(her_choice(&options, "2（19:00）"), "2");
    assert_eq!(her_choice(&options, "订 19:00 的"), "2");
    assert_eq!(her_choice(&options, "19:00，选项2"), "2");
    assert_eq!(her_choice(&options, "1"), "1");
    // A time is not a choice number; two choices named is not one.
    assert_eq!(her_choice(&options, "19点"), "19点");
    assert_eq!(her_choice(&options, "18:00 或 19:00"), "18:00 或 19:00");
    assert_eq!(her_choice(&[], " 随便 "), "随便");
    assert_eq!(her_choice(&options, "1 点"), "1 点");
    assert_eq!(her_choice(&options, "1时"), "1时");
    assert_eq!(her_choice(&options, "2号"), "2");
    // Only plain digits are a number.
    assert_eq!(her_choice(&options, "01"), "01");
    assert_eq!(her_choice(&options, "+1"), "+1");
    // When the choices are days, `2号` names one, or none.
    let days = |names: [&str; 2]| {
        names
            .map(|name| PendingOption {
                value: name.into(),
                label: name.into(),
            })
            .to_vec()
    };
    assert_eq!(her_choice(&days(["2号", "3号"]), "2号"), "1");
    assert_eq!(her_choice(&days(["11月2号", "11月5号"]), "2号"), "2号");
    assert_eq!(her_choice(&days(["11月2号", "11月5号"]), "2"), "2");
    let book = [("yes", "订"), ("no", "不要")]
        .map(|(value, label)| PendingOption {
            value: value.into(),
            label: label.into(),
        })
        .to_vec();
    // Saying no to one is not choosing it.
    assert_eq!(her_choice(&book, "不订了"), "不订了");
    assert_eq!(her_choice(&book, "那就订吧"), "1");
    assert_eq!(her_choice(&book, "不要"), "2");
    // An empty value names nothing.
    let blank = [("", "18:00"), ("", "19:00")]
        .map(|(value, label)| PendingOption {
            value: value.into(),
            label: label.into(),
        })
        .to_vec();
    assert_eq!(her_choice(&blank, "19:00 吧"), "2");
    assert_eq!(
        her_choice(
            &[PendingOption {
                value: "".into(),
                label: "".into()
            }],
            "还没决定"
        ),
        "还没决定"
    );
    // `9:00` inside `19:00` is the longer one said; both said is not one.
    let clock = ["9:00", "19:00"]
        .map(|time| PendingOption {
            value: time.into(),
            label: time.into(),
        })
        .to_vec();
    assert_eq!(her_choice(&clock, "订 19:00 的"), "2");
    assert_eq!(her_choice(&clock, "9:00 吧"), "1");
    assert_eq!(her_choice(&clock, "9:00 还是 19:00"), "9:00 还是 19:00");
}

#[test]
fn the_helper_hears_their_own_words_too() {
    assert_eq!(
        work_input("查一下明天东京的天气", "明天东京会下雨吗？帮我看看"),
        "查一下明天东京的天气\n\n（对方原话，只作参考，这一件只办上面这一条：明天东京会下雨吗？帮我看看）"
    );
    assert_eq!(work_input("帮我看看", "帮我看看"), "帮我看看");
}

#[test]
fn how_it_ended_is_one_line() {
    assert_eq!(
        ended_line("\n## 明天晴\n\n最高 21 度", false),
        "明天晴 最高 21 度"
    );
    // What it leads into comes along.
    assert_eq!(
        ended_line("助手列的是：\n\n1. 整理桌面\n2. 散步", false),
        "助手列的是： 1. 整理桌面 2. 散步"
    );
    // A minus sign is not a bullet; a rule is nothing.
    assert_eq!(ended_line("- -5°C\n---\n-12.5%", false), "-5°C -12.5%");
    assert_eq!(ended_line("#3 号桌\n> * 已订", false), "#3 号桌 已订");
    assert_eq!(ended_line("", false), "办完了。");
    assert_eq!(ended_line("没有权限", true), "没办成：没有权限");
    assert_eq!(ended_line("  ", true), "没办成。");
    let long = "很".repeat(ENDED_LINE_CHARS + 5);
    let line = ended_line(&long, false);
    assert_eq!(line.chars().count(), ENDED_LINE_CHARS + 1);
    assert!(line.ends_with('…'));
}

#[test]
fn a_helpers_result_keeps_all_but_her_lines_that_say_something() {
    let result =
        "[[Work log]]\n```\na\n\n\nb\n```\n[[stop #2]]\n[[work 买菜]]\n结果 [[work: 再查]]\n";
    assert_eq!(
        result_text(result),
        "[[Work log]]\n```\na\n\n\nb\n```\n[[work 买菜]]\n结果 \n"
    );
    assert_eq!(result_text("[[workers]]"), "[[workers]]");
}

#[test]
fn a_line_inside_another_does_not_leave_one_behind() {
    let nested = "[[[[stop #1]]work: 再订一张]]";
    assert!(!result_text(nested).contains("[[work"));
    let (spoken, handed) = split(nested);
    assert!(!spoken.contains("[[work"), "{spoken}");
    assert_eq!(handed, vec![HandOff::Stop { work: 1 }, new("再订一张")]);
}
