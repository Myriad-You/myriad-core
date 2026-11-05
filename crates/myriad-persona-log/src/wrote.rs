//! When the partner last wrote to her, by her log: the newest line of
//! theirs (a `segment.line`, the owner's row) in their own private
//! audience, written on any side. Her speaking rules take the later of
//! this and the side's own last input (`myriad_merope::speech`'s
//! `theyWroteAt`), so a reply the partner sent on another device counts
//! as theirs here too.
//!
//! A forgotten line does not count: not one stubbed, nor one the ledger
//! given takes away.

use crate::{Author, Row, forget, row::Audience};

/// The clock of the newest line the partner wrote to her alone among
/// `rows`, in milliseconds; none when there is none.
pub fn partner_wrote(rows: &[Row], ledger: &[forget::Entry]) -> Option<u64> {
    let view = forget::View::new(ledger);
    rows.iter()
        .filter(|row| row.kind == "segment.line" && row.author == Author::Owner)
        .filter(|row| {
            matches!(Audience::parse(&row.audience), Ok(Audience::Personal(members)) if !members.contains(','))
        })
        .filter(|row| !row.forgotten && !view.absorbs(row, forget::Side::Here))
        .map(|row| &row.hlc)
        .max()
        .map(|hlc| hlc.wall_ms)
}
