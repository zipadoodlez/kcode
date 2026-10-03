//! The kind a row's work is: the one vocabulary the words, the parser that reads
//! them back, and the option list a schema shows all share.
//!
//! A kind is stored as the word the writer wrote (`TaskItem::kind`), so the store
//! learns no engine type (rule 5) and this module is the only place that turns a
//! word into the engine's kind and back.

use serde::{Deserialize, Serialize};

/// The terminal action a row represents. The work list is kind-agnostic: the
/// vocabulary is the engine's, and a row writes one of these words or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Research/analysis. Artifact = findings.
    Explore,
    /// Code change. Artifact = diff/commit ref.
    Implement,
    /// Acceptance check (build/tests).
    Verify,
    /// Repair after a failed verify.
    Fix,
    /// Map-reduce rollup of a composite node's children.
    Synthesize,
    /// Adversarial gap-finder for exploration.
    Critique,
}

/// Every kind the engine knows, in the order its words are listed.
pub const KINDS: [NodeKind; 6] = [
    NodeKind::Explore,
    NodeKind::Implement,
    NodeKind::Verify,
    NodeKind::Fix,
    NodeKind::Synthesize,
    NodeKind::Critique,
];

/// A kind's word, or `None` for a kind the engine has no word for.
pub fn parse_kind(kind: Option<&str>) -> Option<NodeKind> {
    match kind.map(|k| k.trim().to_ascii_lowercase()).as_deref() {
        Some("explore") => Some(NodeKind::Explore),
        Some("implement") => Some(NodeKind::Implement),
        Some("verify") => Some(NodeKind::Verify),
        Some("fix") => Some(NodeKind::Fix),
        Some("synthesize") => Some(NodeKind::Synthesize),
        Some("critique") => Some(NodeKind::Critique),
        _ => None,
    }
}

pub fn kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Explore => "explore",
        NodeKind::Implement => "implement",
        NodeKind::Verify => "verify",
        NodeKind::Fix => "fix",
        NodeKind::Synthesize => "synthesize",
        NodeKind::Critique => "critique",
    }
}

/// The kinds as the words a caller writes: a schema's `enum`, or an error message
/// for a word the engine does not know.
pub fn kind_words() -> String {
    KINDS.map(kind_str).join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The words, the parser and the writer are one vocabulary: a schema and an
    /// error message both read `kind_words`, so a kind added to `KINDS` without a
    /// word, or a word without a kind, has to fail here.
    #[test]
    fn the_words_the_parser_and_the_writer_agree() {
        for kind in KINDS {
            assert_eq!(parse_kind(Some(kind_str(kind))), Some(kind));
        }
        assert_eq!(parse_kind(None), None, "no word is no kind, not a guess");
        assert_eq!(
            parse_kind(Some("implment")),
            None,
            "an unknown word is no kind"
        );
        assert_eq!(parse_kind(Some(" Explore ")), Some(NodeKind::Explore));
        assert_eq!(
            kind_words(),
            "explore, implement, verify, fix, synthesize, critique"
        );
    }
}
