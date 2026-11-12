//! Her nodes (`myriad_nodes`): a node's id and key, a row's body and its
//! one byte form, the fold, the limits both hosts cut to, the pure half of
//! placing a talk and of her upkeep, the upkeep queue, where a fact she
//! keeps goes and the rows that put it there, picking nodes for recall,
//! the three trust tiers, and her intents (the gate, when they are due,
//! the recheck before one runs, the judgment's request and reading). The
//! rules are the crate's; these calls only carry them across. Inputs hold
//! her words, so a refusal names a field, never a value.

use myriad_nodes::{
    Body, NodeError, entries, fold as fold_rows, home, intents, limits as node_limits, listing,
    place, queue as due, recall, trust, upkeep, wire,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::quiet_input;
use crate::Failure;

fn refused(error: NodeError) -> Failure {
    Failure::BadInput(error.to_string())
}

fn answer(value: &impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|_| Failure::BadInput("answer does not serialize".into()))
}

/// `{kind, body}` → `{body, canonical, digest}`: one row's body, checked.
pub(super) fn body(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        kind: String,
        body: Value,
    }
    let In { kind, body } = quiet_input(raw)?;
    let body = Body::read(&kind, &body).map_err(refused)?;
    let canonical = String::from_utf8(body.canonical_bytes())
        .map_err(|_| Failure::BadInput("canonical bytes are UTF-8".into()))?;
    Ok(json!({ "body": body.to_value(), "canonical": canonical, "digest": body.digest() }))
}

/// `{audience, key}` → `{id}`.
pub(super) fn node_id(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        audience: String,
        key: String,
    }
    let In { audience, key } = quiet_input(raw)?;
    Ok(json!({ "id": myriad_nodes::node_id(&audience, &key).map_err(refused)? }))
}

/// `{remote}` → `{key}`.
pub(super) fn repo_key(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        remote: String,
    }
    let In { remote } = quiet_input(raw)?;
    Ok(json!({ "key": myriad_nodes::repo_key(&remote).map_err(refused)? }))
}

/// `{}` → every limit.
pub(super) fn limits(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {}
    let In {} = quiet_input(raw)?;
    answer(&node_limits::limits())
}

/// `{rows, bodies?, ledger}` → the nodes folded; the ledger is required.
/// `reached` is what this side's forgets reach through merges and splits,
/// for local cleanup only: a host writes only the forget the partner made,
/// never a row for these.
pub(super) fn fold(raw: &[u8]) -> Result<Value, Failure> {
    answer(&fold_rows(quiet_input(raw)?).map_err(refused)?)
}

/// `{audience, node, rows, bodies?, ledger, forget?}` → `{takes}`: the
/// rows of other nodes this side's fold places in the node the partner
/// takes back from sync, one sorted list per `retract` row to write (at
/// least one). A retract reaches its node's own rows and what it lists,
/// never through a merge or a split. With `forget`, the lists for a
/// `forget` row a home writes: only the rows it keeps that its fold shows
/// in the node, which a forget takes only when it lists them.
pub(super) fn retract(raw: &[u8]) -> Result<Value, Failure> {
    answer(&fold_rows::retract_lists(quiet_input(raw)?).map_err(refused)?)
}

/// `{audience, rows, bodies?, ledger, holder, elsewhere?}` → a home page
/// (`{audience, rows, payloads, withheld?}`): every row of a node this side
/// keeps, whole, for the export file's home part. A body goes as it would
/// on the wire, but one kept on its device goes too; one that may not go
/// goes as its row alone (`withheld`), and is stored with its body missing
/// where the page is taken. Only a computer of the partner's.
pub(super) fn home_send(raw: &[u8]) -> Result<Value, Failure> {
    answer(&home::send(quiet_input(raw)?).map_err(refused)?)
}

/// `{ledger, origin}` → `{retract}`: the nodes this side is a home of and
/// has written no retract of; it writes one for each, with no list.
pub(super) fn home_owed(raw: &[u8]) -> Result<Value, Failure> {
    answer(&home::home_owed(quiet_input(raw)?).map_err(refused)?)
}

/// `{page, holder, peer, ledger, held?, origin?}` → `{rows, bodies?,
/// filled?, withheld?, ledger, retract?}`: a home page taken onto this
/// computer (`peer` the file header's id, `local-owner` without one),
/// which becomes the home (`retract`: [`home::owed`]);
/// the rows are stored as its own, before the file's log pages, and each
/// stub of `held` the page holds whole is `filled`. Each body is checked as
/// one from the wire; one that fails is `withheld`. Only a computer of the
/// partner's.
pub(super) fn home_take(raw: &[u8]) -> Result<Value, Failure> {
    answer(&home::take(quiet_input(raw)?).map_err(refused)?)
}

/// `{audience, rows, bodies?, theirs?, ask?, holder, here, ledger, peerAbi,
/// elsewhere?}` → `{batches, notHeld?, tooOld?, heldBack?}`: what to send
/// another host core of one audience; a forgotten or retracted node goes as
/// stubs only. `here` is the id this side goes by: a host core under
/// another id is refused. `peerAbi` is the other side's ABI (3 when its
/// hello listed `log3`, 2 when `log2`, else 1): rows it cannot read are not
/// sent to it (`tooOld`). `heldBack` lists rows that went as stubs or were left out,
/// a host's fault.
pub(super) fn wire_send(raw: &[u8]) -> Result<Value, Failure> {
    answer(&wire::send(quiet_input(raw)?).map_err(refused)?)
}

/// `{batch, holder, peer, now, known?, members?, ledger?, epochs?, held?,
/// elsewhere?, people?}` (`peer` the id the sender goes by: one under
/// another id is refused) → what `log.admit` answers, with `bodies?`,
/// `withheld?`, `ask?` and `elsewhere?`. The ledger is required when the
/// batch holds a retract; names come from the rows' envelopes.
pub(super) fn wire_take(raw: &[u8]) -> Result<Value, Failure> {
    answer(&wire::take(quiet_input(raw)?).map_err(refused)?)
}

pub(super) fn place_input(raw: &[u8]) -> Result<Value, Failure> {
    answer(&place::prepare(quiet_input(raw)?).map_err(refused)?)
}

pub(super) fn place_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&place::read(quiet_input(raw)?).map_err(refused)?)
}

/// `{rows, bodies?, max}` → `{due, more?}`: which nodes her upkeep takes
/// next, and why.
pub(super) fn queue(raw: &[u8]) -> Result<Value, Failure> {
    answer(&due::queue(quiet_input(raw)?).map_err(refused)?)
}

pub(super) fn upkeep_input(raw: &[u8]) -> Result<Value, Failure> {
    answer(&upkeep::prepare(quiet_input(raw)?).map_err(refused)?)
}

pub(super) fn upkeep_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&upkeep::read(quiet_input(raw)?).map_err(refused)?)
}

/// `{topics?: [{id, name, aliases?, holds?}], facts: [{id, text}]}` →
/// `{system, schema, input, offered, taken}`: sorting facts under topics.
pub(super) fn sort_input(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::sort_prepare(quiet_input(raw)?).map_err(refused)?)
}

/// `{output, offered?, facts, known?}` → `{facts: [{fact, to, …}]}`.
pub(super) fn sort_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::sort_read(quiet_input(raw)?).map_err(refused)?)
}

/// `{value?, offered?, known?}` → `{to, …}`: where one fact of the
/// settling answer goes.
pub(super) fn under_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::under_read(quiet_input(raw)?).map_err(refused)?)
}

/// `{audience, sync?, start?|key?, facts?}` → `{node, key, rows: [{id,
/// kind, body}]}`.
pub(super) fn fact_rows(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::fact_rows(quiet_input(raw)?).map_err(refused)?)
}

/// `{audience, device, session, repoKnown?, repoSync?, task?, sync?}` →
/// `{repo, repoKey, task, taskKey, rows: [{node, id, kind, body}]}`: what
/// one change of a coding-agent session (`now.session`) puts on her nodes,
/// by rules. Each row's envelope replicate follows its own body's sync.
pub(super) fn session_rows(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::sessions::session_rows(quiet_input(raw)?).map_err(refused)?)
}

/// `{frame, linkedAt, at}` → `{route: look|kept, late, waitsOnOwner}`: what
/// becomes of one `now.session` as it came (`sessions.look`).
pub(super) fn look_route(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::session_look::route_call(quiet_input(raw)?).map_err(refused)?)
}

/// `{firstAt, lastAt}` → `{at}`: when what came together is looked at.
pub(super) fn look_due(raw: &[u8]) -> Result<Value, Failure> {
    Ok(myriad_nodes::session_look::due_call(quiet_input(raw)?))
}

/// `{changes: [{told: {device, session}, memories?, before?}], others?,
/// said?, at, soul?}` → `{material, waits, system, prompt, again, schema}`:
/// her one look at what came together, to ask of the model.
pub(super) fn look_input(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::session_look::look_input(quiet_input(raw)?).map_err(refused)?)
}

/// `{material, waits, raw}` → `{status: good|bad_grounds|unreadable,
/// judgments: [{verdict, about, suggestion, because, missing, ownerMustSay,
/// hasCard, card, evidence} | null], line: {change, text, because} | null}`:
/// her answer read and held to itself.
pub(super) fn look_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::session_look::look_read(quiet_input(raw)?).map_err(refused)?)
}

/// `{kind, node, source, revision?}` → `{id}`.
pub(super) fn row_id(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::row_id(quiet_input(raw)?).map_err(refused)?)
}

/// `{ranked, nodes, aboutThem?, her?, thorough?, budget?}` → `{picked?,
/// aboutThem?, loose?, together?}`.
pub(super) fn pick(raw: &[u8]) -> Result<Value, Failure> {
    answer(&recall::pick(quiet_input(raw)?).map_err(refused)?)
}

/// `{item}` → `{happened?, state?, open?}`: when one fact of the settling
/// answer happened and where it stands; what does not read is left out.
pub(super) fn standing_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&entries::standing_read(quiet_input(raw)?))
}

/// `{range, now, zone}` → `{today, from?, to?}`: the days of the stretch
/// of time the recall judgment named.
pub(super) fn range_dates(raw: &[u8]) -> Result<Value, Failure> {
    answer(&listing::range_dates(quiet_input(raw)?).map_err(refused)?)
}

/// `{nodes, zone, from?, to?, open?, limit?}` → `{items, looked, of,
/// outside, otherState, section?}`: all of a kind, listed once each.
pub(super) fn list(raw: &[u8]) -> Result<Value, Failure> {
    answer(&listing::list(quiet_input(raw)?).map_err(refused)?)
}

/// `{items: [{saidBy, provenance?, aboutPartner?}]}` → `{tiers}`: each
/// piece's tier (`seen`, `heard`, `counts`; `null`: void, the person
/// denied it), from who said it and where it came from, never from its
/// words. `partner` is the one person of the private audience.
pub(super) fn trust_tier(raw: &[u8]) -> Result<Value, Failure> {
    answer(&trust::tiers(quiet_input(raw)?))
}

/// `{private, items: [{saidBy, audience, provenance?}]}` → `{usable}`:
/// whether each piece (said in `audience`) may be used in the private talk
/// of `private` (`personal:<one>`) at all.
pub(super) fn usable_in_private(raw: &[u8]) -> Result<Value, Failure> {
    answer(&trust::usable(quiet_input(raw)?).map_err(refused)?)
}

/// `{because, known?, heard?}` → `{mayChange, verdict}`: whether a view
/// resting on `because` may change (everything it rests on counts), and
/// what it may be written as (`view`, `heard_only`, `refused`).
pub(super) fn view_may_change(raw: &[u8]) -> Result<Value, Failure> {
    answer(&trust::view_answer(quiet_input(raw)?))
}

/// `{origin, action?, because, message?, trigger?, known?}` →
/// `{admitted, action?, why?}`: the one gate an intent passes.
pub(super) fn intent_admit(raw: &[u8]) -> Result<Value, Failure> {
    answer(&intents::admit(quiet_input(raw)?).map_err(refused)?)
}

/// `{origin?, action?, instruction?, now}` → `{verdict, why?}`: whether a
/// due intent runs now or her hand goes up instead.
pub(super) fn intent_check(raw: &[u8]) -> Result<Value, Failure> {
    answer(&intents::check(quiet_input(raw)?))
}

/// `{rows, bodies?, ledger, now}` → `{due, next?}`: the waiting intents due
/// at `now`, and when the next time trigger comes.
pub(super) fn intents_due(raw: &[u8]) -> Result<Value, Failure> {
    answer(&intents::due(quiet_input(raw)?).map_err(refused)?)
}

/// `{soul, said, before?, notes?, waiting?, now, zone}` → `{operation,
/// system, schema, input, notes, waiting}`: the judgment's request.
pub(super) fn intent_judge_input(raw: &[u8]) -> Result<Value, Failure> {
    answer(&intents::prepare(quiet_input(raw)?).map_err(refused)?)
}

/// `{output, notes?, waiting?, now, zone}` → `{takes, drops, unreadable?}`.
pub(super) fn intent_judge_read(raw: &[u8]) -> Result<Value, Failure> {
    answer(&intents::read(quiet_input(raw)?).map_err(refused)?)
}
