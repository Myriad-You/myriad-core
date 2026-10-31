//! Her nodes (`myriad_nodes`): a node's id and key, a row's body and its
//! one byte form, the fold, the limits both hosts cut to, the pure half of
//! placing a talk and of her upkeep, and the upkeep queue. The rules are
//! the crate's; these calls only carry them across. Inputs hold her words,
//! so a refusal names a field, never a value.

use myriad_nodes::{
    Body, NodeError, fold as fold_rows, home, limits as node_limits, place, queue as due, upkeep,
    wire,
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
/// another id is refused. `peerAbi` is the other side's ABI (2 when its
/// hello listed `log2`, else 1): rows it cannot read are not sent to it
/// (`tooOld`). `heldBack` lists rows that went as stubs or were left out,
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
