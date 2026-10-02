//! The close's machine-readable half.
//!
//! A closer hands this in with its words (`{"id","result","artifact"}` is the record
//! the store keeps on the row that owns the work), and it is what a row's own turn
//! reads back as the results of the work under it (`bridge::upstream_context`). It
//! lives here rather than in the engine: the engine owns ownership, status and
//! edges, while this is the shape two sides must agree on, the closer and the row
//! that integrates the close.

use serde::{Deserialize, Serialize};

/// The typed handoff artifact written with a close. This is the machine-readable
/// half that travels with the row's own words to the row that owns the work.
///
/// Forcing an agent to enumerate what it did *not* check is what makes thin work
/// structurally visible, so that field is rendered forward with the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffArtifact {
    /// The deliverable summary (findings for explore, what shipped for implement).
    #[serde(default)]
    pub findings: String,
    /// References, not claims: file:line, commit refs, paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_cases_considered: Vec<String>,
    /// Verify results for code-style nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_confidence_scalar"
    )]
    pub confidence: Option<String>,
    /// Explicit unexplored surface, rendered forward for the row that integrates
    /// this close.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub what_i_did_not_check: Vec<String>,
}

impl HandoffArtifact {
    /// A minimal artifact for tests.
    pub fn brief(findings: impl Into<String>) -> Self {
        Self {
            findings: findings.into(),
            ..Self::default()
        }
    }

    /// Render this artifact as one section of a prompt's context. `kind` is the
    /// node's word for the work when the caller still has the node; a section built
    /// from a closed row's record has no kind, because the row is gone and the store
    /// keeps no engine vocabulary.
    ///
    /// Critically this includes `edge_cases_considered` and `what_i_did_not_check`:
    /// the work is rendered forward with what was *not* checked, so dropping those
    /// fields here would hide that surface.
    pub fn render_section(&self, id: &str, kind: Option<&str>) -> String {
        let mut body = match kind {
            Some(kind) => format!("## {id} ({kind})\n"),
            None => format!("## {id}\n"),
        };
        if !self.findings.trim().is_empty() {
            body.push_str(&self.findings);
            body.push('\n');
        }
        if !self.evidence.is_empty() {
            body.push_str(&format!("Evidence: {}\n", self.evidence.join("; ")));
        }
        if !self.edge_cases_considered.is_empty() {
            body.push_str(&format!(
                "Edge cases considered: {}\n",
                self.edge_cases_considered.join("; ")
            ));
        }
        if let Some(validation) = &self.validation {
            body.push_str(&format!("Validation: {validation}\n"));
        }
        if !self.open_questions.is_empty() {
            body.push_str(&format!(
                "Open questions: {}\n",
                self.open_questions.join("; ")
            ));
        }
        if let Some(confidence) = &self.confidence {
            body.push_str(&format!("Confidence: {confidence}\n"));
        }
        if !self.what_i_did_not_check.is_empty() {
            body.push_str(&format!(
                "What was not checked: {}\n",
                self.what_i_did_not_check.join("; ")
            ));
        }
        body
    }
}

/// Deserialize `confidence` from either a JSON string or a bare number.
/// Agents frequently emit `"confidence": 0.8` instead of `"0.8"`; rejecting
/// that with a serde type error is pointless friction, so numbers are
/// stringified.
fn de_confidence_scalar<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Number(f64),
        Bool(bool),
    }
    Ok(
        Option::<Scalar>::deserialize(deserializer)?.map(|scalar| match scalar {
            Scalar::Text(text) => text,
            Scalar::Number(number) => number.to_string(),
            Scalar::Bool(flag) => flag.to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_accepts_numeric_confidence_json() {
        // Agents emit {"confidence": 0.8}; the deserializer must coerce, not reject.
        let artifact: HandoffArtifact = serde_json::from_str(
            r#"{"findings":"x","confidence":0.8,"what_i_did_not_check":["y"]}"#,
        )
        .expect("numeric confidence should deserialize");
        assert_eq!(artifact.confidence.as_deref(), Some("0.8"));
        let artifact: HandoffArtifact =
            serde_json::from_str(r#"{"findings":"x","confidence":"low"}"#).unwrap();
        assert_eq!(artifact.confidence.as_deref(), Some("low"));
    }

    /// A section renders what the closer listed, and a section with no kind (a
    /// record built from a closed row) keeps the id alone.
    #[test]
    fn a_section_renders_the_artifact_it_was_given() {
        let mut artifact = HandoffArtifact::brief("API lives in foo.rs");
        artifact.evidence = vec!["crates/foo/api.rs:12".into()];
        artifact.what_i_did_not_check = vec!["nothing material".into()];

        let section = artifact.render_section("api", None);
        assert!(section.starts_with("## api\n"));
        assert!(section.contains("API lives in foo.rs"));
        assert!(section.contains("crates/foo/api.rs:12"));
        assert!(section.contains("What was not checked: nothing material"));
        assert!(
            HandoffArtifact::brief("x")
                .render_section("api", Some("implement"))
                .starts_with("## api (implement)\n")
        );
    }
}
