//! Which concept a person just brought up is one she knows only a little
//! about: a real gap worth one question; what she always has in mind about
//! someone; and where her mind wanders over what she knows of them.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, FixedOffset};

use super::association;
use super::lexical::{self, Document};
use super::rank::{self, Row};
use crate::Concept;

/// A concept she knows this few things about is one she knows only a little
/// about: curiosity peaks between knowing nothing and knowing plenty.
pub const THINLY_KNOWN: usize = 2;

/// How many memories each concept (by lowercased name) appears in.
pub fn concept_counts<'a>(
    concepts: impl IntoIterator<Item = &'a [Concept]>,
) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for concept in concepts.into_iter().flatten() {
        *counts.entry(concept.name.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

/// The concept of a memory she knows least about, if she knows only a little,
/// with how many memories she has of it.
pub fn thinly_known(
    concepts: &[Concept],
    counts: &HashMap<String, usize>,
) -> Option<(String, usize)> {
    concepts
        .iter()
        .filter_map(|concept| {
            let known = counts.get(&concept.name.to_lowercase()).copied()?;
            (known <= THINLY_KNOWN).then_some((known, concept.name.clone()))
        })
        .min_by_key(|(known, _)| *known)
        .map(|(known, name)| (name, known))
}

/// Among `memories` (one person's, as recall sees them), the concept `query`
/// names that she knows only a little about, with how many memories she has
/// of it: of the memories `query` truly names (see `lexical`), best match
/// first, the first with such a concept.
pub fn gap_in(memories: &[Document], query: &str) -> Option<(String, usize)> {
    let scores = lexical::score_all(query, memories);
    let counts = concept_counts(memories.iter().map(|memory| memory.concepts));
    let mut named: Vec<(f64, usize)> = scores
        .iter()
        .enumerate()
        .filter(|(_, score)| score.strong)
        .map(|(index, score)| (score.value, index))
        .collect();
    named.sort_by(|left, right| right.0.total_cmp(&left.0));
    named
        .into_iter()
        .find_map(|(_, index)| thinly_known(memories[index].concepts, &counts))
}

/// A concept she has at least this many memories about is a large part of
/// their life as she knows it: more than she knows only a little about.
pub const CENTRAL: usize = THINLY_KNOWN + 1;
/// How many such concepts are always in mind with them.
pub const ALWAYS_KNOWN: usize = 3;

/// What she always has in mind about someone, whatever they are saying now,
/// as indices into `rows`: for each of the few things she knows most about
/// (their cat, their work), the memory of it that comes to mind most
/// readily. What they said lately and what is recalled only when named are
/// not among them. Most known first; ties go to the readier memory, then
/// the newer (`rows` come newest first).
pub fn always_known(rows: &[Row], now: DateTime<FixedOffset>) -> Vec<usize> {
    let lasting: Vec<usize> = (0..rows.len())
        .filter(|index| {
            let source = rows[*index].source;
            source != rank::SHORT_NOTE_SOURCE && !rank::RECALLED_WHEN_NAMED.contains(&source)
        })
        .collect();
    let counts = concept_counts(lasting.iter().map(|index| rows[*index].concepts));
    let readiness: Vec<f64> = rows.iter().map(|row| rank::readiness(row, now)).collect();
    // Each central concept, with how many memories are about it and the
    // readiest of them.
    let mut central: Vec<(usize, usize)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for index in &lasting {
        for concept in rows[*index].concepts {
            let key = concept.name.to_lowercase();
            let known = counts.get(&key).copied().unwrap_or(0);
            if known < CENTRAL || !seen.insert(key.clone()) {
                continue;
            }
            let best = lasting
                .iter()
                .copied()
                .filter(|other| {
                    rows[*other]
                        .concepts
                        .iter()
                        .any(|concept| concept.name.to_lowercase() == key)
                })
                .fold(None, |best: Option<usize>, other| match best {
                    Some(best) if readiness[best] >= readiness[other] => Some(best),
                    _ => Some(other),
                });
            if let Some(best) = best {
                central.push((known, best));
            }
        }
    }
    central.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(readiness[right.1].total_cmp(&readiness[left.1]))
            .then(left.1.cmp(&right.1))
    });
    let mut chosen = Vec::new();
    for (_, index) in central {
        if !chosen.contains(&index) {
            chosen.push(index);
        }
        if chosen.len() == ALWAYS_KNOWN {
            break;
        }
    }
    chosen
}

/// How far one bout of mind-wandering drifts.
pub const WANDER_STEPS: usize = 3;
/// A memory she thought of is not landed on again for this long: the same
/// thought does not keep coming back.
pub const NOT_AGAIN_SECS: i64 = 3 * 24 * 3600;

/// One memory her mind may wander over.
pub struct Musing<'a> {
    pub content: &'a str,
    pub concepts: &'a [Concept],
    /// When it was learned.
    pub at: DateTime<FixedOffset>,
    /// What is still on her mind of it (priming), `0` when nothing.
    pub primed: f64,
    /// When she last thought of it while wandering, if she remembers.
    pub thought_at: Option<DateTime<FixedOffset>>,
}

/// Where a bout of mind-wandering settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wandered {
    /// Into the rows given.
    pub index: usize,
    /// Something in it she knows only a little about, and how many memories
    /// she has of it, if anything.
    pub gap: Option<(String, usize)>,
}

/// Let the mind wander over one person's memories (`rows`, newest first, the
/// ones the present audience may hear): the same words count once; start
/// from what is most on the mind (else somewhere, leaning recent), drift up
/// to [`WANDER_STEPS`] along association (see [`association::wander`]) and
/// settle on something not thought of in the last [`NOT_AGAIN_SECS`].
/// `roll` yields uniform numbers in `0..1`: one for the start when nothing
/// is on the mind, then one per step.
pub fn wander(
    rows: &[Musing],
    now: DateTime<FixedOffset>,
    roll: &mut impl FnMut() -> f64,
) -> Option<Wandered> {
    let mut seen = HashSet::new();
    let kept: Vec<usize> = (0..rows.len())
        .filter(|index| {
            let content = rank::normalize_content(rows[*index].content);
            !content.is_empty() && seen.insert(content)
        })
        .collect();
    if kept.len() < 2 {
        return None;
    }
    let nodes: Vec<association::Node> = kept
        .iter()
        .map(|index| association::Node {
            concepts: rows[*index].concepts,
            at: rows[*index].at,
        })
        .collect();
    // The last of equals, as `Iterator::max_by` keeps it.
    let primed = kept
        .iter()
        .enumerate()
        .map(|(position, index)| (rows[*index].primed, position))
        .filter(|(activation, _)| *activation > 0.0)
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, position)| position);
    // Rows are newest first; squaring leans a random start toward recent.
    let start = primed.unwrap_or_else(|| {
        let at = roll().clamp(0.0, 1.0);
        ((at * at) * kept.len() as f64) as usize % kept.len()
    });
    let avoid: Vec<usize> = kept
        .iter()
        .enumerate()
        .filter(|(_, index)| {
            rows[**index]
                .thought_at
                .is_some_and(|at| (now - at).num_seconds() < NOT_AGAIN_SECS)
        })
        .map(|(position, _)| position)
        .collect();
    let landed = association::wander(&nodes, start, WANDER_STEPS, &avoid, roll)?;
    let counts = concept_counts(nodes.iter().map(|node| node.concepts));
    Some(Wandered {
        index: kept[landed],
        gap: thinly_known(nodes[landed].concepts, &counts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn about(names: &[&str]) -> Vec<Concept> {
        names
            .iter()
            .map(|name| Concept {
                name: name.to_string(),
                aliases: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn she_is_curious_where_she_knows_a_little_not_nothing_or_plenty() {
        let guitar = about(&["吉他"]);
        let cat = about(&["猫", "年糕"]);
        let memories = [
            Document {
                text: "最近在学吉他",
                concepts: &guitar,
            },
            Document {
                text: "养了一只猫叫年糕",
                concepts: &cat,
            },
            Document {
                text: "年糕上周打了疫苗",
                concepts: &cat,
            },
            Document {
                text: "年糕怕吸尘器",
                concepts: &cat,
            },
            Document {
                text: "年糕喜欢晒太阳",
                concepts: &cat,
            },
        ];
        assert_eq!(
            gap_in(&memories, "今天吉他弹了一小时"),
            Some(("吉他".into(), 1))
        );
        assert_eq!(
            gap_in(&memories, "猫今天好乖"),
            None,
            "she knows plenty about the cat"
        );
        assert_eq!(gap_in(&memories, "明日预报"), None, "nothing named");
        assert_eq!(gap_in(&[], "吉他"), None);
    }

    #[test]
    fn the_least_known_concept_of_a_memory_is_the_gap() {
        let concepts = [about(&["猫", "Mochi"]), about(&["mochi"]), about(&["猫"])];
        let counts = concept_counts(concepts.iter().map(Vec::as_slice));
        assert_eq!(counts.get("mochi"), Some(&2));
        assert_eq!(counts.get("猫"), Some(&2));
        let one = about(&["吉他", "猫"]);
        let counts = concept_counts([one.as_slice(), &concepts[0]]);
        assert_eq!(thinly_known(&one, &counts), Some(("吉他".into(), 1)));
        assert_eq!(thinly_known(&about(&["茶"]), &counts), None, "never seen");
    }

    #[test]
    fn what_she_knows_most_about_is_always_in_mind() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-18T12:00:00+09:00").unwrap();
        let day = chrono::Duration::days(1);
        let cat = about(&["年糕"]);
        let work = about(&["工作"]);
        let guitar = about(&["吉他"]);
        let row = |id: &'static str, content: &'static str, concepts, source, days, uses| Row {
            id,
            content,
            concepts,
            source,
            created_at: now - day * days,
            access_count: uses,
            last_accessed_at: None,
        };
        let rows = [
            row("a", "年糕上周打了疫苗", &cat, "chat", 1, 0),
            row("b", "在学吉他", &guitar, "chat", 1, 0),
            row("c", "养了一只猫叫年糕", &cat, "chat", 200, 9),
            row("d", "年糕怕吸尘器", &cat, "chat", 30, 0),
            row("e", "在设计公司上班", &work, "chat", 100, 0),
            row("f", "工作要加班", &work, "chat_note", 1, 0),
            row("g", "换了组，工作轻松了", &work, "chat", 20, 0),
            row("h", "工作在涩谷", &work, "chat", 40, 0),
        ];
        let chosen = always_known(&rows, now);
        // Four lasting memories of the cat, three of work (the short note
        // does not count); the guitar is only a little known.
        assert_eq!(chosen.len(), 2);
        assert_eq!(rows[chosen[0]].content, "年糕上周打了疫苗");
        assert_eq!(rows[chosen[1]].content, "换了组，工作轻松了");
        // Two of the cat are still only a little.
        assert!(always_known(&rows[..3], now).is_empty());
        assert!(always_known(&[], now).is_empty());
    }
}
