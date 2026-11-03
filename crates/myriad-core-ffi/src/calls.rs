//! The dispatch table: each call is a crate function, its input and output
//! as JSON. A call reads only its input; field names are camelCase.
//!
//! Times are microseconds since the Unix epoch (`i64`). Her zone is an IANA
//! name (`Asia/Shanghai`) or a fixed offset (`+08:00`): a name, because the
//! day's edges move with daylight saving and the same name is what her
//! persona carries to every platform.
//!
//! The calls of one area live in its module below; this file holds the
//! table and what they share.

use std::str::FromStr;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::Failure;

mod affect;
mod archive;
mod chat;
mod chat_app;
mod consistency;
mod filter;
mod grants;
mod group;
mod handoff;
mod link;
mod log;
mod memory;
mod motion;
mod nodes;
mod outfit;
mod own;
mod partner;
mod prompts;
mod reach;
mod reminder;
mod serial;
mod soup;
mod speaking;
mod speech;
mod style;
mod sync;
mod timing;
mod vitals;
mod voice;

type Call = fn(&[u8]) -> Result<Value, Failure>;

/// Sorted by name.
const CALLS: &[(&str, Call)] = &[
    ("affect.apply_appraisal", affect::apply_appraisal),
    ("affect.lite_appraisal", affect::lite_appraisal),
    ("affect.mood_band", affect::mood_band),
    ("affect.mood_cue", affect::mood_cue),
    ("affect.music_limits", affect::music_limits),
    ("affect.music_listening", affect::music_listening),
    ("affect.persona_baseline", affect::persona_baseline),
    ("affect.pull_push", affect::pull_push),
    ("affect.settle", affect::settle),
    ("affect.transition", affect::transition),
    ("affect.user_utterance", affect::user_utterance),
    ("audience.admits", memory::admits),
    ("bits.ask", voice::bits_ask),
    ("channel.as_messages", chat_app::as_messages),
    ("chat.history_lines", chat::history_lines),
    ("chat.history_text", chat::history_text),
    ("chat.pause_text", chat::pause_text),
    ("chat.reply_instruction", prompts::reply_instruction),
    ("chat.safe_content", chat::safe_content),
    ("chat_days.day_system", prompts::chat_days_day_system),
    ("chat_days.pick_system", prompts::chat_days_pick_system),
    (
        "chat_remember.already_system",
        prompts::remember_already_system,
    ),
    (
        "chat_remember.extract_system",
        prompts::remember_extract_system,
    ),
    (
        "chat_remember.settle_system",
        prompts::remember_settle_system,
    ),
    ("consistency.apart", consistency::apart),
    ("consistency.measure", consistency::measure),
    ("consistency.prompt_diff", consistency::prompt_diff),
    ("consistency.prompt_segments", consistency::prompt_segments),
    ("contrast.describe", style::describe),
    ("contrast.overused", style::overused),
    ("contrast.pieces", style::pieces),
    ("curiosity.digest_system", prompts::curiosity_digest_system),
    ("curiosity.wonder_system", prompts::curiosity_wonder_system),
    ("diary.compact_summary", memory::compact_summary),
    ("diary.redact", memory::redact),
    ("diary.should_write_chat", memory::should_write_chat_diary),
    ("doing.choice_system", prompts::doing_choice_system),
    ("doing.digest_system", prompts::doing_digest_system),
    ("draft.ask", voice::draft_ask),
    ("draft.send", voice::draft_send),
    ("explore.compare_system", prompts::explore_compare_system),
    ("explore.step_system", prompts::explore_step_system),
    ("explore.think_system", prompts::explore_think_system),
    ("explore.wonder_system", prompts::explore_wonder_system),
    ("filter.check_relay", filter::check_relay),
    ("filter.decide", filter::decide_call),
    ("filter.lists", filter::lists),
    ("filter.page", filter::page),
    ("filter.scan_identifiers", filter::scan_identifiers),
    ("filter.scan_secrets", filter::scan_secrets),
    ("filter.secret_values", filter::secret_values),
    ("filter.source", filter::source),
    ("filter.text_hits", filter::text_hits),
    ("filter.window", filter::window_title),
    ("grants.autonomy", grants::autonomy),
    ("grants.cap_still_allows", grants::cap_still_allows),
    ("grants.check", grants::check),
    ("grants.claim", grants::claim),
    ("grants.effective", grants::effective),
    ("grants.granted", grants::granted),
    ("grants.may_enter_work", grants::may_enter_work),
    ("grants.prepare", grants::prepare),
    ("grants.revoke", grants::revoke),
    ("group.kept_as_heard", group::kept_as_heard),
    ("group.ledger_who", group::ledger_who),
    ("group.merge_ledger", group::merge_ledger),
    ("group.offer", group::offer),
    ("group.step", group::step),
    ("handoff.act", handoff::act),
    ("handoff.ended_line", handoff::ended_line),
    ("handoff.her_choice", handoff::her_choice),
    ("handoff.labelled", handoff::labelled),
    ("handoff.line", handoff::line),
    ("handoff.next_number", handoff::next_number),
    ("handoff.result_text", handoff::result_text),
    ("handoff.route", handoff::route),
    ("handoff.section", handoff::section),
    ("handoff.shown", handoff::shown),
    ("handoff.split", handoff::split),
    ("handoff.stands", handoff::stands),
    ("handoff.work_input", handoff::work_input),
    ("heard.system", prompts::heard_system),
    ("inner.system", prompts::inner_system),
    ("inner.system_for", prompts::inner_system_for),
    ("inner.system_for_group", prompts::inner_system_for_group),
    ("lease.behind", log::lease_behind),
    ("lease.here", log::lease_here),
    ("lease.move", log::lease_move),
    ("lease.newer", log::lease_newer),
    ("library.csv_records", serial::csv_records),
    ("library.from_aozora", serial::from_aozora),
    ("library.from_gutenberg", serial::from_gutenberg),
    ("life.concepts_system", prompts::concepts_system),
    ("life.own_day_system", prompts::own_day_system),
    ("link.check", link::check_frame),
    ("link.encode", link::encode),
    ("log.admit", log::admit),
    ("log.answer", log::send),
    ("log.canonical", log::canonical),
    ("log.clock_next", log::clock_next),
    ("log.clock_observe", log::clock_observe),
    ("log.clock_offset", log::clock_offset),
    ("log.clock_repair", log::clock_repair),
    ("log.fold", log::fold),
    ("log.holdings", log::holdings),
    ("log.ledger", log::ledger),
    ("log.merge", log::merge),
    ("log.missing", log::missing),
    ("log.reach", log::reach),
    ("making.how_it_went", soup::how_it_went),
    ("making.kept_line", soup::kept_line),
    ("making.make_input", soup::make_input),
    ("making.make_system", soup::make_system),
    ("making.named_in", soup::named_in),
    ("making.offer_own", soup::offer_own),
    ("making.parse_idea", soup::parse_idea),
    ("making.record_line", soup::record_line),
    ("making.untried_at", soup::untried_at),
    ("making.writing_about_own", soup::writing_about_own),
    ("making_sense.told_section", speaking::told_section),
    ("memes.read_kept", chat_app::read_kept),
    ("memes.section", chat_app::meme_section),
    ("memory.always_known", memory::always_known),
    ("memory.base_level", memory::base_level),
    ("memory.gap_in", memory::gap_in),
    ("memory.of_row", memory::of_row),
    ("memory.overlap", memory::overlap),
    ("memory.readiness", memory::readiness),
    ("memory.score_all", memory::score_all),
    ("memory.spread", memory::spread),
    ("meta.version", version),
    ("motion.asked", motion::asked),
    ("motion.chat_prompt", motion::chat_prompt),
    ("motion.direction", motion::direction),
    ("motion.floor", motion::floor),
    ("motion.lay_way", motion::lay_way),
    ("motion.plan", motion::plan),
    ("motion.standing_acting", motion::standing_acting),
    ("motion.standing_doing", motion::standing_doing),
    ("motion.standing_floor", motion::standing_floor),
    ("motion.way", motion::way),
    ("nodes.body", nodes::body),
    ("nodes.fold", nodes::fold),
    ("nodes.home_owed", nodes::home_owed),
    ("nodes.home_send", nodes::home_send),
    ("nodes.home_take", nodes::home_take),
    ("nodes.id", nodes::node_id),
    ("nodes.limits", nodes::limits),
    ("nodes.place_input", nodes::place_input),
    ("nodes.place_read", nodes::place_read),
    ("nodes.queue", nodes::queue),
    ("nodes.repo_key", nodes::repo_key),
    ("nodes.retract", nodes::retract),
    ("nodes.upkeep_input", nodes::upkeep_input),
    ("nodes.upkeep_read", nodes::upkeep_read),
    ("nodes.wire_send", nodes::wire_send),
    ("nodes.wire_take", nodes::wire_take),
    ("onboarding.import_system", voice::onboarding_import),
    ("onboarding.name_system", voice::onboarding_name),
    (
        "onboarding.observe_portrait_system",
        voice::onboarding_observe_portrait,
    ),
    ("onboarding.persona_system", voice::onboarding_persona),
    ("onboarding.tags_system", voice::onboarding_tags),
    (
        "onboarding.visual_design_system",
        voice::onboarding_visual_design,
    ),
    ("others.first_words_view", reach::first_words_view),
    ("outfit.after_reply", outfit::after_reply),
    ("outfit.hold_wear", outfit::hold_wear),
    ("outfit.looks_from_profile", outfit::looks_from_profile),
    ("outfit.resolve", outfit::resolve),
    ("outfit.resolve_directive", outfit::resolve_directive),
    ("outfit.split_wear", outfit::split_wear),
    ("outfit.wardrobe_section", outfit::wardrobe_section),
    ("pace.lazing", prompts::pace_lazing),
    ("pace.section", prompts::pace_section),
    ("partner.file", partner::file),
    ("partner.link", partner::link),
    ("partner.migrate", partner::migrate),
    ("partner.mint", partner::mint),
    ("partner.page", partner::page),
    ("persona.flatten", voice::flatten),
    ("prompt.untrusted_block", speaking::untrusted_block),
    ("reach.as_text", reach::as_text),
    ("reach.judge_system", reach::judge_system),
    ("reach.key", reach::key),
    ("reach.parse_judged", reach::parse_judged),
    ("reach.reason", reach::reason),
    ("reach.route", reach::route),
    ("reach.texts", reach::texts),
    ("reach.writing_first", reach::writing_first),
    ("reach.wrote_before", reach::wrote_before),
    ("remembering.system", prompts::remembering_system),
    ("reminder.due", reminder::due),
    ("self_story.ask", voice::self_story_ask),
    ("self_story.checked", voice::self_story_checked),
    ("sending.goes_out", voice::goes_out),
    ("sending.limit_take", voice::limit_take),
    ("serial.advance", serial::advance),
    ("serial.clean_aozora", serial::clean_aozora),
    ("serial.clean_gutenberg", serial::clean_gutenberg),
    ("serial.looking_back", serial::looking_back),
    ("serial.out", serial::out),
    ("serial.parts", serial::parts),
    ("serial.texts", serial::texts),
    ("serial.view", serial::view),
    (
        "sharing.come_back_reason",
        prompts::sharing_come_back_reason,
    ),
    (
        "sharing.come_back_system",
        prompts::sharing_come_back_system,
    ),
    ("sharing.reason", prompts::sharing_reason),
    ("sharing.system", prompts::sharing_system),
    ("sore.carried_section", own::sore_carried_section),
    ("sore.input", own::sore_input),
    ("sore.mood_weighs", own::sore_mood_weighs),
    ("sore.section", own::sore_section),
    ("soup.apply", soup::apply),
    ("soup.judge_input", soup::judge_input),
    ("soup.judge_schema", soup::judge_schema),
    ("soup.section", soup::section),
    ("soup.split_start", soup::split_start),
    ("soup.start_system", soup::start_system),
    ("soup.texts", soup::texts),
    (
        "speaking.acquaintance_section",
        speaking::acquaintance_section,
    ),
    ("speaking.activity_section", speaking::activity_section),
    ("speaking.addressee_section", speaking::addressee_section),
    ("speaking.already_told", speaking::already_told),
    (
        "speaking.already_told_section",
        speaking::already_told_section,
    ),
    ("speaking.bits_section", own::bits_section),
    (
        "speaking.brought_to_mind_section",
        speaking::brought_to_mind_section,
    ),
    ("speaking.chat_app_section", chat_app::chat_app_section),
    ("speaking.contract", speaking::contract),
    ("speaking.contract_at", voice::contract_at),
    ("speaking.curious_section", speaking::curious_section),
    ("speaking.doing_section", own::doing_section),
    ("speaking.emotion_section", speaking::emotion_section),
    ("speaking.found_out_section", prompts::found_out_section),
    ("speaking.group_days_section", own::group_days_section),
    ("speaking.group_section", speaking::group_section),
    ("speaking.guest_section", speaking::guest_section),
    ("speaking.habits_section", speaking::habits_section),
    ("speaking.home", voice::home),
    ("speaking.inner_moment_section", own::inner_moment_section),
    ("speaking.lands_section", own::lands_section),
    ("speaking.mood_section", speaking::mood_section),
    ("speaking.now_section", speaking::now_section),
    (
        "speaking.on_your_mind_section",
        speaking::on_your_mind_section,
    ),
    ("speaking.openers_section", speaking::openers_section),
    ("speaking.own_days_section", own::own_days_section),
    ("speaking.persona", speaking::persona),
    ("speaking.proactive_system", prompts::proactive_system),
    ("speaking.proactive_user", prompts::proactive_user),
    ("speaking.recent_section", speaking::recent_section),
    ("speaking.remembered_section", speaking::remembered_section),
    ("speaking.said_lately_line", prompts::said_lately_line),
    ("speaking.self_story_section", own::self_story_section),
    ("speaking.share_links_section", prompts::share_links_section),
    ("speaking.share_section", prompts::share_section),
    ("speaking.since_section", speaking::since_section),
    ("speaking.soul", voice::soul),
    ("speaking.taste_section", own::taste_section),
    ("speaking.us_section", own::us_section),
    ("speaking.views_section", own::views_section),
    ("speech.admit", speech::admit),
    ("speech.choice_parse", speech::choice_parse),
    ("speech.choice_prompt", speech::choice_prompt),
    ("speech.markers", speech::markers),
    ("speech.said_from_rows", speech::said_from_rows),
    ("speech.strip_markers", speech::strip_markers),
    ("speech.texts", speech::texts),
    ("speech.told", speech::told),
    ("stickers.section", chat_app::sticker_section),
    ("style.distance", style::distance),
    ("style.noticed", style::noticed),
    ("style.unlike", style::unlike),
    ("sync.affect_fold", sync::affect_fold),
    ("sync.affect_replay", sync::affect_replay),
    ("sync.archive_key", archive::archive_key),
    ("sync.archive_open", archive::archive_open),
    ("sync.archive_part", archive::archive_part),
    ("sync.archive_seal", archive::archive_seal),
    ("sync.canonicalize", sync::canonicalize),
    ("sync.check_rev", sync::check_rev),
    ("sync.compare", sync::compare),
    ("sync.hlc_next", sync::hlc_next),
    ("sync.hlc_observe", sync::hlc_observe),
    ("sync.is_empty", sync::is_empty),
    ("sync.legacy_rev", sync::legacy_rev),
    ("sync.manifest", archive::manifest),
    ("sync.merge_plan", sync::merge_plan),
    ("sync.persona_diff", sync::persona_diff),
    ("sync.rig_package_id", sync::rig_package_id),
    ("talk_shape.describe", chat_app::describe),
    ("talk_shape.goes_out_as", chat_app::goes_out_as),
    (
        "talk_shape.messages_this_turn",
        chat_app::messages_this_turn,
    ),
    ("talk_shape.room_of", chat_app::room_of),
    ("talk_shape.usual", chat_app::usual),
    ("threads.section", own::threads_section),
    ("timing.asleep_at", timing::asleep_at),
    ("timing.asleep_for", timing::asleep_for),
    ("timing.day_at", timing::day_at),
    ("timing.her_date_at", timing::her_date_at),
    ("timing.her_day_at", timing::her_day_at),
    ("timing.past_bedtime", timing::past_bedtime),
    ("timing.past_bedtime_at", timing::past_bedtime_at),
    ("timing.sleep", timing::sleep),
    ("timing.typing", chat_app::typing),
    ("timing.until_read", voice::until_read),
    ("timing.where_she_is", voice::where_she_is),
    ("views.system", prompts::views_system),
    ("vitals.alerts", vitals::alerts),
    ("vitals.ends_asking", vitals::ends_asking),
    ("vitals.leaned_on", vitals::leaned_on),
    ("vitals.quantile", vitals::quantile),
    ("wants.section", prompts::wants_section),
    ("wants.system", prompts::wants_system),
];

pub(crate) fn names() -> Vec<&'static str> {
    CALLS.iter().map(|(name, _)| *name).collect()
}

pub(crate) fn dispatch(name: &str, input: &[u8]) -> Result<Value, Failure> {
    let at = CALLS
        .binary_search_by(|(known, _)| (*known).cmp(name))
        .map_err(|_| Failure::UnknownCall(name.to_string()))?;
    (CALLS[at].1)(input)
}

fn input<T: DeserializeOwned>(raw: &[u8]) -> Result<T, Failure> {
    serde_json::from_slice(raw).map_err(|error| Failure::BadInput(error.to_string()))
}

/// [`input`] for calls whose values name people or permissions: a failure
/// keeps a field name, the client's own word, and never a value.
fn quiet_input<T: DeserializeOwned>(raw: &[u8]) -> Result<T, Failure> {
    serde_json::from_slice(raw).map_err(|error| {
        let message = error.to_string();
        if message.starts_with("unknown field") || message.starts_with("missing field") {
            let named = message.split(", expected").next().unwrap_or("");
            Failure::BadInput(named.split(" at line").next().unwrap_or("").to_string())
        } else {
            Failure::BadInput("input does not fit the call".into())
        }
    })
}

fn text(text: Option<String>) -> Value {
    json!({ "text": text })
}

/// A call that takes nothing still takes an object.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nothing {}

fn at(micros: i64) -> Result<DateTime<Utc>, Failure> {
    DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| Failure::BadInput(format!("time {micros} is out of range")))
}

fn opt_at(micros: Option<i64>) -> Result<Option<DateTime<Utc>>, Failure> {
    micros.map(at).transpose()
}

/// Her zone: an IANA name, or a fixed offset `±HH:MM`.
#[derive(Clone, Copy)]
enum Zone {
    Named(chrono_tz::Tz),
    Fixed(FixedOffset),
}

impl Zone {
    fn parse(raw: &str) -> Result<Self, Failure> {
        if let Ok(zone) = chrono_tz::Tz::from_str(raw) {
            return Ok(Self::Named(zone));
        }
        FixedOffset::from_str(raw)
            .map(Self::Fixed)
            .map_err(|_| Failure::BadInput(format!("unknown zone {raw:?}")))
    }
}

/// `$body` with `$zone` bound to the zone itself, whichever kind it is: the
/// crate's rules are generic over `chrono::TimeZone`.
macro_rules! on_zone {
    ($zone:expr, |$bound:ident| $body:expr) => {
        match $zone {
            $crate::calls::Zone::Named($bound) => $body,
            $crate::calls::Zone::Fixed($bound) => $body,
        }
    };
}
use on_zone;

fn version(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(json!({
        "abi": crate::ABI_VERSION,
        "upstreamCommit": crate::UPSTREAM_COMMIT,
        "calls": names(),
    }))
}
