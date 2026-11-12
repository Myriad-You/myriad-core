//! The cards her look at the owner's sessions puts before them (scene
//! `agents`): what she made of a change that needs them (`decide`, `nod`),
//! and where a session came to (`status`). Every end that looks at
//! sessions makes them here, so a card is the same wherever she is.
//!
//! Its facts are the program's, said from what the device told of the
//! session: never the words of a conversation, so a quoted line of the
//! owner's, the session's or another session's is not one of them. What
//! she rests her view on (the material's numbers) is told by the facts
//! that say the same thing.

use myriad_her_link::now::{AskedBy, Change, CheckKind, CheckState, Session, WaitingOn, rules};
use myriad_persona_log::payloads::{CardAction, CardFact, CardVersion, CardView};
use serde::Deserialize;

use crate::NodeError;
use crate::cards::{self, OPEN};
use crate::place::clean;
use crate::session_look::{Judgment, Material, Told, Verdict, one_line};
use crate::sessions::{self, session_ref};

/// The scene of these cards.
pub const SCENE: &str = "agents";

/// What each kind of fact says, in the order a card lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Says {
    Session,
    Given,
    Stands,
    OwnerSaid,
    Files,
    Checks,
    Failing,
    Overwrote,
}

impl Says {
    /// The kind of fact a piece of the material is told by, if any. What
    /// another session did or hit, what she remembers and what she made of
    /// it before is not one of this session's facts: a card lists no
    /// grounds for it.
    fn of_piece(what: &str) -> Option<Self> {
        Some(match what {
            "the session" => Self::Session,
            "who gave it this work" | "what the session that sent it on told it" => Self::Given,
            "what it last said or asked" | "its choices" | "what its choices hold" => Self::Stands,
            "what the owner last told it" | "when the owner last spoke to it themselves" => {
                Self::OwnerSaid
            }
            "files it changed" => Self::Files,
            "its tests and builds" => Self::Checks,
            "a failure it keeps hitting" => Self::Failing,
            "what it did to another session's work" => Self::Overwrote,
            _ => return None,
        })
    }
}

fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn place(session: &Session) -> &str {
    match (&session.remote, &session.folder) {
        (Some(remote), _) => remote.rsplit('/').next().unwrap_or(remote),
        (None, Some(folder)) => folder.rsplit('/').next().unwrap_or(folder),
        (None, None) => "",
    }
}

/// What a card says, in one language. `{…}` are filled in.
struct Words {
    session: &'static str,
    session_in: &'static str,
    given_owner: &'static str,
    given_by: &'static str,
    given_session: &'static str,
    now: &'static str,
    started: &'static str,
    asked: &'static str,
    finished: &'static str,
    failed: &'static str,
    quiet: &'static str,
    waits_owner: &'static str,
    waits_session: &'static str,
    waits_checks: &'static str,
    waits_program: &'static str,
    owner_said: &'static str,
    files: &'static str,
    list: &'static str,
    checks: [&'static str; 3],
    check: &'static str,
    states: [&'static str; 4],
    between: &'static str,
    failing: &'static str,
    undid: &'static str,
    rewrote: &'static str,
    just_now: &'static str,
    minutes: &'static str,
    hours: &'static str,
    days: &'static str,
    missing: &'static str,
    open: &'static str,
    agree: &'static str,
    ok: &'static str,
}

const EN: Words = Words {
    session: "{tool} session",
    session_in: "{tool} session in {repo}",
    given_owner: "You assigned this work yourself",
    given_by: "Sent on by «{by}»; reports back there",
    given_session: "Sent on by another session; reports back there",
    now: "Now: {stands}",
    started: "started working",
    asked: "stopped with a question for you",
    finished: "finished a turn, nothing for you to answer",
    failed: "stuck (kept erring, or its checks failed)",
    quiet: "quiet for long",
    waits_owner: "; waiting on you",
    waits_session: "; waiting on the session that sent it on",
    waits_checks: "; waiting on tests or builds",
    waits_program: "; waiting on another program",
    owner_said: "Your last word to the session: {ago}",
    files: "Changed {n} file(s): {list}",
    list: ", ",
    checks: ["Tests", "Build", "Check"],
    check: "{kind} {state}",
    states: ["running", "passed", "failed", "ended, outcome unknown"],
    between: "; ",
    failing: "The same failure {n} times, latest {ago}",
    undid: "Put back what another session had just written ({n} file(s), {ago})",
    rewrote: "Rewrote what another session had just written ({n} file(s), {ago})",
    just_now: "just now",
    minutes: "{n} min ago",
    hours: "{n} h ago",
    days: "{n} d ago",
    missing: "Missing: {missing}",
    open: "Open session",
    agree: "Go with it",
    ok: "OK",
};

const ZH_CN: Words = Words {
    session: "{tool} 会话",
    session_in: "{repo} 里的 {tool} 会话",
    given_owner: "你亲自派的活",
    given_by: "「{by}」派的活，做完要回那边",
    given_session: "另一个会话派的活，做完要回那边",
    now: "现在：{stands}",
    started: "开始干活",
    asked: "停下来，有问题要问你",
    finished: "做完了这一轮，没有要你回答的",
    failed: "卡住了（一直出错，或检查没过）",
    quiet: "很久没有动静",
    waits_owner: "；在等你",
    waits_session: "；在等派活的会话",
    waits_checks: "；在等测试或构建",
    waits_program: "；在等别的程序",
    owner_said: "你上次亲自发话：{ago}",
    files: "改了 {n} 个文件：{list}",
    list: "、",
    checks: ["测试", "构建", "检查"],
    check: "{kind}{state}",
    states: ["在跑", "通过", "没过", "结束了，结果不知道"],
    between: "；",
    failing: "同一个错误出了 {n} 次，最近一次：{ago}",
    undid: "改回了另一个会话刚写的内容（{n} 个文件，{ago}）",
    rewrote: "改写了另一个会话刚写的内容（{n} 个文件，{ago}）",
    just_now: "刚才",
    minutes: "{n} 分钟前",
    hours: "{n} 小时前",
    days: "{n} 天前",
    missing: "还缺：{missing}",
    open: "去会话里说",
    agree: "按建议办",
    ok: "可以",
};

const ZH_TW: Words = Words {
    session: "{tool} 工作階段",
    session_in: "{repo} 裡的 {tool} 工作階段",
    given_owner: "你親自交辦的工作",
    given_by: "「{by}」交辦的工作，做完要回報那邊",
    given_session: "另一個工作階段交辦的，做完要回報那邊",
    now: "現在：{stands}",
    started: "開始動工",
    asked: "停下來，有問題要問你",
    finished: "做完了這一輪，沒有要你回答的",
    failed: "卡住了（一直出錯，或檢查沒過）",
    quiet: "很久沒有動靜",
    waits_owner: "；在等你",
    waits_session: "；在等交辦的工作階段",
    waits_checks: "；在等測試或建置",
    waits_program: "；在等別的程式",
    owner_said: "你上次親自發話：{ago}",
    files: "改了 {n} 個檔案：{list}",
    list: "、",
    checks: ["測試", "建置", "檢查"],
    check: "{kind}{state}",
    states: ["在跑", "通過", "沒過", "結束了，結果不明"],
    between: "；",
    failing: "同一個錯誤出了 {n} 次，最近一次：{ago}",
    undid: "改回了另一個工作階段剛寫的內容（{n} 個檔案，{ago}）",
    rewrote: "改寫了另一個工作階段剛寫的內容（{n} 個檔案，{ago}）",
    just_now: "剛才",
    minutes: "{n} 分鐘前",
    hours: "{n} 小時前",
    days: "{n} 天前",
    missing: "還缺：{missing}",
    open: "去工作階段裡說",
    agree: "照建議辦",
    ok: "可以",
};

const JA: Words = Words {
    session: "{tool} セッション",
    session_in: "{repo} の {tool} セッション",
    given_owner: "自分で頼んだ作業",
    given_by: "「{by}」から回された作業。終わったらそちらに戻る",
    given_session: "別のセッションから回された作業。終わったらそちらに戻る",
    now: "今：{stands}",
    started: "作業を始めた",
    asked: "止まって、あなたに質問がある",
    finished: "ひと区切りついた。答えることはない",
    failed: "行き詰まっている（エラーが続くか、チェックが通らない）",
    quiet: "長いあいだ動きがない",
    waits_owner: "。あなたを待っている",
    waits_session: "。回してきたセッションを待っている",
    waits_checks: "。テストかビルドを待っている",
    waits_program: "。別のプログラムを待っている",
    owner_said: "最後に自分で話しかけた：{ago}",
    files: "{n} 個のファイルを変更：{list}",
    list: "、",
    checks: ["テスト", "ビルド", "チェック"],
    check: "{kind}：{state}",
    states: ["実行中", "成功", "失敗", "終了（結果不明）"],
    between: "、",
    failing: "同じエラーが {n} 回、最新は{ago}",
    undid: "別のセッションが書いたばかりの内容を元に戻した（{n} ファイル、{ago}）",
    rewrote: "別のセッションが書いたばかりの内容を書き換えた（{n} ファイル、{ago}）",
    just_now: "さっき",
    minutes: "{n} 分前",
    hours: "{n} 時間前",
    days: "{n} 日前",
    missing: "足りないもの：{missing}",
    open: "セッションで話す",
    agree: "提案どおりに",
    ok: "OK",
};

const KO: Words = Words {
    session: "{tool} 세션",
    session_in: "{repo}의 {tool} 세션",
    given_owner: "직접 맡긴 작업",
    given_by: "「{by}」에서 넘긴 작업, 끝나면 그쪽으로 보고",
    given_session: "다른 세션에서 넘긴 작업, 끝나면 그쪽으로 보고",
    now: "지금: {stands}",
    started: "작업 시작",
    asked: "멈추고 당신에게 질문이 있음",
    finished: "한 차례 끝남, 답할 것 없음",
    failed: "막힘 (오류가 계속되거나 검사 실패)",
    quiet: "오랫동안 움직임 없음",
    waits_owner: "; 당신을 기다리는 중",
    waits_session: "; 넘겨준 세션을 기다리는 중",
    waits_checks: "; 테스트나 빌드를 기다리는 중",
    waits_program: "; 다른 프로그램을 기다리는 중",
    owner_said: "마지막으로 직접 말한 때: {ago}",
    files: "파일 {n}개 변경: {list}",
    list: ", ",
    checks: ["테스트", "빌드", "검사"],
    check: "{kind} {state}",
    states: ["실행 중", "통과", "실패", "종료(결과 모름)"],
    between: "; ",
    failing: "같은 오류 {n}회, 마지막은 {ago}",
    undid: "다른 세션이 방금 쓴 내용을 되돌림 ({n}개 파일, {ago})",
    rewrote: "다른 세션이 방금 쓴 내용을 고쳐 씀 ({n}개 파일, {ago})",
    just_now: "방금",
    minutes: "{n}분 전",
    hours: "{n}시간 전",
    days: "{n}일 전",
    missing: "부족한 것: {missing}",
    open: "세션에서 말하기",
    agree: "제안대로",
    ok: "좋아요",
};

const FR: Words = Words {
    session: "session {tool}",
    session_in: "session {tool} dans {repo}",
    given_owner: "Travail que vous avez confié vous-même",
    given_by: "Confié par « {by} » ; rend compte là-bas",
    given_session: "Confié par une autre session ; rend compte là-bas",
    now: "Maintenant : {stands}",
    started: "a commencé",
    asked: "arrêtée, avec une question pour vous",
    finished: "a fini un tour, rien à répondre",
    failed: "bloquée (erreurs répétées ou vérifications en échec)",
    quiet: "silencieuse depuis longtemps",
    waits_owner: " ; vous attend",
    waits_session: " ; attend la session qui l'a confiée",
    waits_checks: " ; attend des tests ou une compilation",
    waits_program: " ; attend un autre programme",
    owner_said: "Votre dernier mot à la session : {ago}",
    files: "{n} fichier(s) modifié(s) : {list}",
    list: ", ",
    checks: ["Tests", "Compilation", "Vérification"],
    check: "{kind} : {state}",
    states: ["en cours", "succès", "échec", "terminé, résultat inconnu"],
    between: " ; ",
    failing: "La même erreur {n} fois, dernière {ago}",
    undid: "A annulé ce qu'une autre session venait d'écrire ({n} fichier(s), {ago})",
    rewrote: "A réécrit ce qu'une autre session venait d'écrire ({n} fichier(s), {ago})",
    just_now: "à l'instant",
    minutes: "il y a {n} min",
    hours: "il y a {n} h",
    days: "il y a {n} j",
    missing: "Manque : {missing}",
    open: "Ouvrir",
    agree: "D'accord",
    ok: "OK",
};

const DE: Words = Words {
    session: "{tool}-Sitzung",
    session_in: "{tool}-Sitzung in {repo}",
    given_owner: "Von dir selbst beauftragt",
    given_by: "Von „{by}“ beauftragt; berichtet dorthin zurück",
    given_session: "Von einer anderen Sitzung beauftragt; berichtet dorthin zurück",
    now: "Jetzt: {stands}",
    started: "hat begonnen",
    asked: "angehalten, mit einer Frage an dich",
    finished: "eine Runde fertig, nichts zu beantworten",
    failed: "steckt fest (wiederholte Fehler oder fehlgeschlagene Prüfungen)",
    quiet: "lange still",
    waits_owner: "; wartet auf dich",
    waits_session: "; wartet auf die beauftragende Sitzung",
    waits_checks: "; wartet auf Tests oder Build",
    waits_program: "; wartet auf ein anderes Programm",
    owner_said: "Dein letztes Wort an die Sitzung: {ago}",
    files: "{n} Datei(en) geändert: {list}",
    list: ", ",
    checks: ["Tests", "Build", "Prüfung"],
    check: "{kind}: {state}",
    states: [
        "läuft",
        "bestanden",
        "fehlgeschlagen",
        "beendet, Ergebnis unbekannt",
    ],
    between: "; ",
    failing: "Derselbe Fehler {n}-mal, zuletzt {ago}",
    undid: "Hat zurückgesetzt, was eine andere Sitzung gerade geschrieben hatte ({n} Datei(en), {ago})",
    rewrote: "Hat überschrieben, was eine andere Sitzung gerade geschrieben hatte ({n} Datei(en), {ago})",
    just_now: "gerade eben",
    minutes: "vor {n} Min.",
    hours: "vor {n} Std.",
    days: "vor {n} Tagen",
    missing: "Es fehlt: {missing}",
    open: "Öffnen",
    agree: "Ja, so",
    ok: "OK",
};

/// The words of `language` (`zh-CN`, `zh-TW`, `ja-JP`, `ko-KR`, `fr-FR`,
/// `de-DE`, `en-US`, or only its first part); English for any other.
fn words(language: &str) -> &'static Words {
    let language = language.to_ascii_lowercase().replace('_', "-");
    match language.as_str() {
        "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => &ZH_TW,
        _ if language.starts_with("zh-hant") => &ZH_TW,
        _ if language == "zh" || language.starts_with("zh-") => &ZH_CN,
        _ if language == "ja" || language.starts_with("ja-") => &JA,
        _ if language == "ko" || language.starts_with("ko-") => &KO,
        _ if language == "fr" || language.starts_with("fr-") => &FR,
        _ if language == "de" || language.starts_with("de-") => &DE,
        _ => &EN,
    }
}

fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut text = template.to_string();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// The card's title: the session's own, or what it is.
fn title(session: &Session, words: &Words) -> String {
    let title = match &session.title {
        Some(title) if !clean(title, true).is_empty() => clean(title, true),
        _ => fill(words.session, &[("tool", &session.tool)]),
    };
    cut(&title, cards::TITLE_CHARS)
}

fn ago(at: i64, then: i64, words: &Words) -> String {
    match at.saturating_sub(then) / 60_000 {
        ..=0 => words.just_now.to_string(),
        minutes @ ..60 => fill(words.minutes, &[("n", &minutes.to_string())]),
        minutes @ ..1440 => fill(words.hours, &[("n", &(minutes / 60).to_string())]),
        minutes => fill(words.days, &[("n", &(minutes / 1440).to_string())]),
    }
}

fn stands(session: &Session, words: &Words) -> String {
    let change = match session.change {
        Change::Started => words.started,
        Change::Asked => words.asked,
        Change::Finished => words.finished,
        Change::Failed => words.failed,
        Change::Quiet => words.quiet,
    };
    let waiting = match session.waiting_on {
        // Asking is waiting on them already.
        Some(WaitingOn::Owner) if session.change == Change::Asked => "",
        Some(WaitingOn::Owner) => words.waits_owner,
        Some(WaitingOn::Session) => words.waits_session,
        Some(WaitingOn::Checks) => words.waits_checks,
        Some(WaitingOn::Program) => words.waits_program,
        Some(WaitingOn::Unknown) | None => "",
    };
    fill(words.now, &[("stands", &format!("{change}{waiting}"))])
}

/// One fact of each kind the session has, as the card lists them.
fn facts(told: &Told, at: i64, words: &Words) -> Vec<(Says, CardFact)> {
    let session = &told.session;
    let reference = session_ref(&told.device, &session.grant, &session.session);
    let mut facts = Vec::new();
    let mut add = |says: Says, text: String| {
        facts.push((
            says,
            CardFact {
                text: cut(&text, cards::FACT_CHARS),
                reference: Some(reference.clone()),
            },
        ));
    };
    let repo = place(session);
    add(
        Says::Session,
        if repo.is_empty() {
            fill(words.session, &[("tool", &session.tool)])
        } else {
            fill(words.session_in, &[("tool", &session.tool), ("repo", repo)])
        },
    );
    match (session.asked_by, &session.asked_by_title) {
        (Some(AskedBy::Owner), _) => add(Says::Given, words.given_owner.to_string()),
        (Some(AskedBy::Session), Some(by)) => add(
            Says::Given,
            fill(words.given_by, &[("by", &cut(&clean(by, true), 40))]),
        ),
        (Some(AskedBy::Session), None) => add(Says::Given, words.given_session.to_string()),
        (Some(AskedBy::None) | None, _) => {}
    }
    add(Says::Stands, stands(session, words));
    if let Some(said_at) = session.owner_said_at {
        add(
            Says::OwnerSaid,
            fill(words.owner_said, &[("ago", &ago(at, said_at, words))]),
        );
    }
    if !session.touched.is_empty() {
        let names: Vec<&str> = session
            .touched
            .iter()
            .map(|path| path.rsplit('/').next().unwrap_or(path))
            .collect();
        add(
            Says::Files,
            fill(
                words.files,
                &[
                    ("n", &names.len().to_string()),
                    ("list", &names.join(words.list)),
                ],
            ),
        );
    }
    let mut seen = Vec::new();
    let mut checks = Vec::new();
    for check in &session.checks {
        if seen.contains(&check.kind) {
            continue;
        }
        seen.push(check.kind);
        let kind = match check.kind {
            CheckKind::Test => words.checks[0],
            CheckKind::Build => words.checks[1],
            CheckKind::Other => words.checks[2],
        };
        let state = match check.state {
            CheckState::Running => words.states[0],
            CheckState::Passed => words.states[1],
            CheckState::Failed => words.states[2],
            CheckState::Ended => words.states[3],
        };
        checks.push(fill(words.check, &[("kind", kind), ("state", state)]));
    }
    if !checks.is_empty() {
        add(Says::Checks, checks.join(words.between));
    }
    if let Some(failure) = session
        .errors
        .iter()
        .filter(|failure| failure.count >= rules().repeated_error_at)
        .max_by_key(|failure| failure.count)
    {
        add(
            Says::Failing,
            fill(
                words.failing,
                &[
                    ("n", &failure.count.to_string()),
                    ("ago", &ago(at, failure.last_at, words)),
                ],
            ),
        );
    }
    if let Some(overwrote) = session
        .overwrote
        .iter()
        .max_by_key(|overwrote| overwrote.at)
    {
        add(
            Says::Overwrote,
            fill(
                if overwrote.undid {
                    words.undid
                } else {
                    words.rewrote
                },
                &[
                    ("n", &overwrote.files.to_string()),
                    ("ago", &ago(at, overwrote.at, words)),
                ],
            ),
        );
    }
    facts.truncate(cards::FACTS);
    facts
}

/// The state word of where the session stands (`node.standing`'s).
fn state(session: &Session) -> Option<String> {
    let (state, _) = sessions::standing(session);
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
}

fn action(id: &str, label: &str) -> CardAction {
    CardAction {
        id: id.to_string(),
        label: label.to_string(),
    }
}

/// The action of a session's choice `number` (from 1).
fn choice_id(number: usize) -> String {
    format!("choice_{number}")
}

/// A session's choice as a card's label: what the scans take out of it
/// marked, in one line, cut to [`cards::LABEL_CHARS`] with a mark where it
/// was cut. None when nothing is left.
fn choice_label(choice: &str) -> Option<String> {
    let scrubbed = myriad_perception_filter::secrets::scan(choice).text;
    let scrubbed = myriad_perception_filter::identifiers::scan(&scrubbed).text;
    let label = cut(&clean(&one_line(&scrubbed), true), cards::LABEL_CHARS);
    (!label.trim().is_empty()).then_some(label)
}

/// What the owner may answer a card of `judgment` with, and the one she
/// leans to. A session that gave choices is answered with them, as it gave
/// them, the one she leans to first; one that gave none, with her
/// suggestion when it is one answer. Going to the session is always last.
fn answers(
    session: &Session,
    judgment: &Judgment,
    words: &Words,
) -> (Vec<CardAction>, Option<String>) {
    let mut actions = Vec::new();
    let mut leans = None;
    let choices: Vec<(usize, String)> = if session.change == Change::Asked {
        session
            .choices
            .iter()
            .enumerate()
            .filter_map(|(at, choice)| Some((at + 1, choice_label(choice)?)))
            .take(cards::ACTIONS - 1)
            .collect()
    } else {
        Vec::new()
    };
    if !choices.is_empty() {
        let leaned = judgment
            .leans
            .filter(|number| choices.iter().any(|(at, _)| at == number));
        if let Some(number) = leaned {
            leans = Some(choice_id(number));
        }
        let (first, rest): (Vec<_>, Vec<_>) = choices
            .into_iter()
            .partition(|(number, _)| Some(*number) == leaned);
        for (number, label) in first.into_iter().chain(rest) {
            actions.push(action(&choice_id(number), &label));
        }
    } else if judgment.answers && !judgment.suggestion.trim().is_empty() {
        let (id, label) = match judgment.verdict {
            Verdict::Nod => ("ok", words.ok),
            _ => ("agree", words.agree),
        };
        actions.push(action(id, label));
        leans = Some(id.to_string());
    }
    actions.push(action(OPEN, words.open));
    (actions, leans)
}

/// `nodes.look_card`: the card of one change of her look.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionInput {
    /// The session's task node.
    pub task: String,
    pub told: Told,
    pub judgment: Judgment,
    /// What she was shown, and which change of it this is (from 1).
    pub material: Material,
    pub change: usize,
    pub at: i64,
    /// The owner's language (`zh-CN`, `en-US`, ...): what the program says
    /// on the card is said in it.
    pub language: String,
}

/// The card her judgment of a change puts before the owner: `decide` or
/// `nod`, on the session's task, with the facts the program has of it, her
/// view resting on those that say what she was shown, and what they may
/// answer. A change that needs nothing of them (`no_need`) has none.
pub fn decision(input: DecisionInput) -> Result<CardVersion, NodeError> {
    let judgment = &input.judgment;
    let kind = match judgment.verdict {
        Verdict::Decide => "decide",
        Verdict::Nod => "nod",
        Verdict::NoNeed => return Err(NodeError::new("a change that needs nothing has no card")),
    };
    if input.change == 0 || input.change > input.material.changes.len() {
        return Err(NodeError::new("change is not one of the material"));
    }
    let words = words(&input.language);
    let facts = facts(&input.told, input.at, words);
    let mut because: Vec<u32> = judgment
        .because
        .iter()
        .filter_map(|number| number.checked_sub(1))
        .filter_map(|at| input.material.pieces.get(at))
        .filter(|piece| piece.change == Some(input.change))
        .filter_map(|piece| Says::of_piece(&piece.what))
        .filter_map(|says| facts.iter().position(|(said, _)| *said == says))
        .map(|at| at as u32 + 1)
        .collect();
    because.sort();
    because.dedup();
    // A judgment with no grounds and nothing to suggest is not hers: the
    // program's own card when no answer of hers came (`card_only`), whose
    // about is the material's heading. Its facts say it; it has no view.
    let hers = !judgment.because.is_empty()
        || !judgment.suggestion.trim().is_empty()
        || judgment.missing.is_some();
    let mut text = if hers {
        judgment.about.trim().to_string()
    } else {
        String::new()
    };
    for more in [
        judgment.suggestion.trim().to_string(),
        judgment
            .missing
            .as_deref()
            .map(str::trim)
            .filter(|missing| !missing.is_empty())
            .map(|missing| fill(words.missing, &[("missing", missing)]))
            .unwrap_or_default(),
    ] {
        if !more.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&more);
        }
    }
    let (actions, leans) = answers(&input.told.session, judgment, words);
    let view = (!text.is_empty()).then(|| CardView {
        text: cut(&text, cards::VIEW_CHARS),
        because,
        leans,
    });
    let card = CardVersion {
        at: input.at,
        id: cards::card_id(SCENE, kind, &format!("node:{}", input.task))?,
        kind: kind.to_string(),
        scene: SCENE.to_string(),
        subject: Some(format!("node:{}", input.task)),
        title: title(&input.told.session, words),
        state: state(&input.told.session),
        facts: facts.into_iter().map(|(_, fact)| fact).collect(),
        view,
        actions,
    };
    cards::check(&card)?;
    Ok(card)
}

/// `nodes.look_status`: where a session came to.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusInput {
    pub task: String,
    pub told: Told,
    pub at: i64,
    pub language: String,
}

/// The card of where a session came to (`status`), when it finished,
/// failed or got stuck: the facts and the state, nothing to answer but
/// going to it. None while it is under way or waiting on someone: that is
/// no news of its own.
pub fn status(input: StatusInput) -> Result<Option<CardVersion>, NodeError> {
    let session = &input.told.session;
    if !matches!(session.change, Change::Finished | Change::Failed) {
        return Ok(None);
    }
    let words = words(&input.language);
    let card = CardVersion {
        at: input.at,
        id: cards::card_id(SCENE, "status", &format!("node:{}", input.task))?,
        kind: "status".to_string(),
        scene: SCENE.to_string(),
        subject: Some(format!("node:{}", input.task)),
        title: title(session, words),
        state: state(session),
        facts: facts(&input.told, input.at, words)
            .into_iter()
            .map(|(_, fact)| fact)
            .collect(),
        view: None,
        actions: vec![action(OPEN, words.open)],
    };
    cards::check(&card)?;
    Ok(Some(card))
}

/// `nodes.look_settled`: a card whose session went on without the owner's
/// answer on it (they answered in the session, it moved on or ended).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettledInput {
    pub card: CardVersion,
    pub told: Told,
    pub at: i64,
    pub language: String,
}

/// The card's next version once its session went on: the same facts as
/// the session now has them, the state it is in, and nothing to answer
/// but going to it, so it no longer waits on the owner.
pub fn settled(input: SettledInput) -> Result<CardVersion, NodeError> {
    let words = words(&input.language);
    let mut card = input.card;
    card.at = input.at;
    card.state = state(&input.told.session);
    card.facts = facts(&input.told, input.at, words)
        .into_iter()
        .map(|(_, fact)| fact)
        .collect();
    // Her view stood on the facts as they were: kept, its grounds and
    // what she leaned to answer gone.
    if let Some(view) = &mut card.view {
        view.because.clear();
        view.leans = None;
    }
    // Nothing to answer but going to it, said in the owner's language now.
    card.actions = vec![action(OPEN, words.open)];
    cards::check(&card)?;
    Ok(card)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::session_look::{Recalled, Shown, material};

    const AT: i64 = 1_760_000_000_000;

    fn told(session: &str, extra: serde_json::Value) -> Told {
        let mut session = json!({
            "grant": "g_0123456789", "session": session, "change": "failed",
            "occurredAt": AT, "tool": "coding-agent", "title": "Split the store",
            "remote": "github.com/demo/sample-repo", "askedBy": "session",
            "askedByTitle": "Plan the release", "fromPeer": "split it",
            "fromOwner": "and commit", "ownerSaidAt": AT - 60_000,
            "fromAgent": "Q: push?", "touched": ["src/a.rs"],
            "checks": [{"kind": "test", "state": "failed", "at": AT}],
            "errors": [{"sig": "e1", "count": 9, "firstAt": AT - 60_000, "lastAt": AT}],
            "overwrote": [{"session": "s-2", "files": 1, "undid": true, "at": AT}],
        });
        for (key, value) in extra.as_object().unwrap() {
            session[key] = value.clone();
        }
        Told {
            device: "dev_mac".to_string(),
            session: serde_json::from_value(session).unwrap(),
        }
    }

    /// Every piece the material shows of a change is told by a fact, or is
    /// one a card never holds as one: a new piece is placed on purpose.
    #[test]
    fn every_piece_of_a_change_is_a_fact_or_never_one() {
        let never = [
            "another session in this repository",
            "a failure another session hits too",
            "what another session did to its work",
            "what you know of the owner (how they decided before)",
            "what you made of this session last time",
        ];
        let one = told("s-1", json!({}));
        let two = told("s-2", json!({ "change": "asked" }));
        let shown = material(
            &[Shown {
                told: one.clone(),
                memories: vec![Recalled {
                    id: "m1".to_string(),
                    text: "pushes need their own word".to_string(),
                }],
                before: Some("要你定：推送".to_string()),
            }],
            std::slice::from_ref(&two),
            &[],
            AT,
        );
        let mut said = Vec::new();
        for piece in shown.pieces.iter().filter(|piece| piece.change.is_some()) {
            match Says::of_piece(&piece.what) {
                Some(says) => said.push(says),
                None => assert!(never.contains(&piece.what.as_str()), "{}", piece.what),
            }
        }
        // And each kind told has its fact on the card.
        let facts = facts(&one, AT, words("zh-CN"));
        for says in said {
            assert!(facts.iter().any(|(fact, _)| *fact == says), "{says:?}");
        }
    }
}
