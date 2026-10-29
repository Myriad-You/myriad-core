//! Every length and count a node rule checks or cuts to, in one place, so
//! the Swift and the Rust host never disagree on where a text ends.
//!
//! Lengths are in Unicode scalar values (Rust `char`s, Swift
//! `unicodeScalars`), not bytes and not grapheme clusters. These are first
//! values; they change here, for every host at once.

use serde::Serialize;

/// A node's name, and an object's name or alias.
pub const NAME_CHARS: usize = 80;
/// One sentence of a summary.
pub const SENTENCE_CHARS: usize = 300;
/// Sentences in a summary.
pub const SUMMARY_SENTENCES: usize = 12;
/// A span, an entry, a view, an open item or an intent.
pub const TEXT_CHARS: usize = 1000;
/// Her reason for a kind change or a relation.
pub const REASON_CHARS: usize = 300;
/// How far someone has got with an object ("第 12 集").
pub const PROGRESS_CHARS: usize = 200;
/// Evidence ids on one thing (`because`).
pub const BECAUSE_IDS: usize = 32;
/// References to originals on one span or entry.
pub const REFS: usize = 64;
/// Versions a revision may name as its base.
pub const BASES: usize = 16;
/// Aliases of one object.
pub const ALIASES: usize = 16;
/// People in who-knows.
pub const WHO_KNOWS: usize = 64;
/// An item id she mints (a view, an open item, an intent), in bytes.
pub const ITEM_ID_BYTES: usize = 64;
/// A minted key's token, in bytes.
pub const MINTED_BYTES: usize = 64;
/// Older versions a folded item keeps, newest kept.
pub const HISTORY: usize = 20;

/// Items of a talk placed in one call.
pub const PLACE_ITEMS: usize = 40;
/// One item's text as the model sees it.
pub const PLACE_ITEM_CHARS: usize = 600;
/// All items' text together, as the model sees it.
pub const PLACE_TOTAL_CHARS: usize = 12_000;
/// Nodes offered to place on.
pub const PLACE_CANDIDATES: usize = 12;
/// A node's name as the model sees it.
pub const PLACE_NAME_CHARS: usize = 40;
/// A node's summary as the model sees it.
pub const PLACE_SUMMARY_CHARS: usize = 160;
/// The gist she gives one span.
pub const PLACE_GIST_CHARS: usize = 120;
/// The model's answer, in bytes; a longer one is not read.
pub const PLACE_OUTPUT_BYTES: usize = 16_384;

/// Devices one upkeep row names in `seen`.
pub const SEEN_DEVICES: usize = 64;
/// Spans of the node she is keeping, newest kept, as the model sees them.
pub const UPKEEP_SPANS: usize = 24;
/// One span's (or inbox item's) text as the model sees it.
pub const UPKEEP_SPAN_CHARS: usize = 400;
/// All spans' text together, as the model sees it.
pub const UPKEEP_TOTAL_CHARS: usize = 12_000;
/// Open items, intents and views of the node, each, as the model sees them.
pub const UPKEEP_HEAD_ITEMS: usize = 12;
/// Nodes one relation away, as the model sees them.
pub const UPKEEP_NEIGHBOURS: usize = 16;
/// Inbox items offered to file, oldest first.
pub const UPKEEP_INBOX: usize = 20;
/// All inbox items' text together, as the model sees it.
pub const UPKEEP_INBOX_TOTAL_CHARS: usize = 6_000;
/// Actions in one answer; an answer with more is not read.
pub const UPKEEP_ACTIONS: usize = 24;
/// The model's upkeep answer, in bytes; a longer one is not read.
pub const UPKEEP_OUTPUT_BYTES: usize = 32_768;
/// Nodes one queue call may hand out.
pub const QUEUE_MAX: usize = 200;

/// The marker a cut text ends with.
pub const ELLIPSIS: char = '…';

/// What `nodes.limits` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub name_chars: usize,
    pub sentence_chars: usize,
    pub summary_sentences: usize,
    pub text_chars: usize,
    pub reason_chars: usize,
    pub progress_chars: usize,
    pub because_ids: usize,
    pub refs: usize,
    pub bases: usize,
    pub aliases: usize,
    pub who_knows: usize,
    pub item_id_bytes: usize,
    pub minted_bytes: usize,
    pub history: usize,
    pub place_items: usize,
    pub place_item_chars: usize,
    pub place_total_chars: usize,
    pub place_candidates: usize,
    pub place_name_chars: usize,
    pub place_summary_chars: usize,
    pub place_gist_chars: usize,
    pub place_output_bytes: usize,
    pub seen_devices: usize,
    pub upkeep_spans: usize,
    pub upkeep_span_chars: usize,
    pub upkeep_total_chars: usize,
    pub upkeep_head_items: usize,
    pub upkeep_neighbours: usize,
    pub upkeep_inbox: usize,
    pub upkeep_inbox_total_chars: usize,
    pub upkeep_actions: usize,
    pub upkeep_output_bytes: usize,
    pub queue_max: usize,
}

pub fn limits() -> Limits {
    Limits {
        name_chars: NAME_CHARS,
        sentence_chars: SENTENCE_CHARS,
        summary_sentences: SUMMARY_SENTENCES,
        text_chars: TEXT_CHARS,
        reason_chars: REASON_CHARS,
        progress_chars: PROGRESS_CHARS,
        because_ids: BECAUSE_IDS,
        refs: REFS,
        bases: BASES,
        aliases: ALIASES,
        who_knows: WHO_KNOWS,
        item_id_bytes: ITEM_ID_BYTES,
        minted_bytes: MINTED_BYTES,
        history: HISTORY,
        place_items: PLACE_ITEMS,
        place_item_chars: PLACE_ITEM_CHARS,
        place_total_chars: PLACE_TOTAL_CHARS,
        place_candidates: PLACE_CANDIDATES,
        place_name_chars: PLACE_NAME_CHARS,
        place_summary_chars: PLACE_SUMMARY_CHARS,
        place_gist_chars: PLACE_GIST_CHARS,
        place_output_bytes: PLACE_OUTPUT_BYTES,
        seen_devices: SEEN_DEVICES,
        upkeep_spans: UPKEEP_SPANS,
        upkeep_span_chars: UPKEEP_SPAN_CHARS,
        upkeep_total_chars: UPKEEP_TOTAL_CHARS,
        upkeep_head_items: UPKEEP_HEAD_ITEMS,
        upkeep_neighbours: UPKEEP_NEIGHBOURS,
        upkeep_inbox: UPKEEP_INBOX,
        upkeep_inbox_total_chars: UPKEEP_INBOX_TOTAL_CHARS,
        upkeep_actions: UPKEEP_ACTIONS,
        upkeep_output_bytes: UPKEEP_OUTPUT_BYTES,
        queue_max: QUEUE_MAX,
    }
}

/// `text` cut to `max` chars: the first `max - 1` and [`ELLIPSIS`] when it
/// is longer.
pub fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push(ELLIPSIS);
    out
}
