//! Lenient serde helpers for string-valued configuration enums.
//!
//! A single unrecognized enum value used to abort parsing of the *entire*
//! `config.toml`, so `Config::load` silently fell back to `Config::default()`
//! and every unrelated setting looked like it was ignored (issue #689: one
//! unparseable enum line also disabled `centered`, `idle_animation`, and
//! `show_thinking`). Degrading the one field that is
//! actually wrong is far better than discarding the user's configuration.

use serde::Deserialize;

/// Deserialize a string-valued enum, falling back to its `Default` when the
/// value is not recognized.
pub(crate) fn lenient_enum<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let raw = String::deserialize(deserializer)?;
    let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(raw.as_str());
    Ok(T::deserialize(value).unwrap_or_default())
}

/// `Option` variant of [`lenient_enum`]: an unrecognized value becomes `None`
/// ("not configured") rather than a config-wide parse failure.
pub(crate) fn lenient_optional_enum<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let Some(raw) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(raw.as_str());
    Ok(T::deserialize(value).ok())
}

/// Regression tests for issue #689: an unrecognized value in one `[display]`
/// enum field must not discard the rest of the user's configuration.
#[cfg(test)]
mod tests {
    use crate::{
        DiffDisplayMode, DisplayConfig, MarkdownSpacingMode, OverscrollStatusMode,
    };

    fn parse(json: &str) -> DisplayConfig {
        serde_json::from_str(json).expect("display config must still parse")
    }

    #[test]
    fn unknown_enum_value_keeps_the_rest_of_the_display_config() {
        let display = parse(
            r#"{
                "centered": true,
                "idle_animation": true,
                "show_thinking": true,
                "diff_mode": "totally-bogus"
            }"#,
        );

        assert!(display.centered, "unrelated settings must survive");
        assert!(display.idle_animation);
        assert!(display.show_thinking);
        assert_eq!(display.diff_mode, DiffDisplayMode::default());
    }

    #[test]
    fn unknown_reasoning_display_falls_back_without_losing_other_fields() {
        let display = parse(r#"{"centered": true, "reasoning_display": "nope"}"#);
        assert!(display.centered);
        assert!(
            !display.has_explicit_reasoning_display(),
            "an unparseable value means 'not configured'"
        );
    }

    #[test]
    fn unknown_modes_fall_back_to_defaults() {
        let display = parse(
            r#"{"centered": true, "diff_mode": "nope",
                "markdown_spacing": "nope", "overscroll_status": "nope"}"#,
        );
        assert!(display.centered);
        assert_eq!(display.diff_mode, DiffDisplayMode::default());
        assert_eq!(display.markdown_spacing, MarkdownSpacingMode::default());
        assert_eq!(display.overscroll_status, OverscrollStatusMode::default());
    }

    /// Valid values must still round-trip (leniency must not swallow them).
    #[test]
    fn valid_values_are_unaffected() {
        let display = parse(r#"{"markdown_spacing": "document", "diff_mode": "full-inline"}"#);
        assert_eq!(display.markdown_spacing, MarkdownSpacingMode::Document);
        assert_eq!(display.diff_mode, DiffDisplayMode::FullInline);
    }
}
