use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UsageOverlayStatus {
    Loading,
    Good,
    Warning,
    Critical,
    Error,
    Info,
}

impl UsageOverlayStatus {
    pub fn label_for_display(self) -> &'static str {
        self.label()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Good => "healthy",
            Self::Warning => "watch",
            Self::Critical => "high",
            Self::Error => "error",
            Self::Info => "info",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Loading => Color::Rgb(129, 184, 255),
            Self::Good => Color::Rgb(111, 214, 181),
            Self::Warning => Color::Rgb(255, 196, 112),
            Self::Critical => Color::Rgb(255, 146, 110),
            Self::Error => Color::Rgb(232, 134, 134),
            Self::Info => Color::Rgb(196, 170, 255),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Loading => "◌",
            Self::Good => "●",
            Self::Warning => "▲",
            Self::Critical => "◆",
            Self::Error => "✕",
            Self::Info => "○",
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UsageOverlayItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub status: UsageOverlayStatus,
    pub detail_lines: Vec<String>,
}

impl UsageOverlayItem {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        status: UsageOverlayStatus,
        detail_lines: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            status,
            detail_lines,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UsageOverlaySummary {
    pub provider_count: usize,
    pub warning_count: usize,
    pub critical_count: usize,
    pub error_count: usize,
    pub session_visible: bool,
}

pub fn item_matches_filter(item: &UsageOverlayItem, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }

    let haystack = format!(
        "{} {} {} {} {}",
        item.id,
        item.title,
        item.subtitle,
        item.status.label(),
        item.detail_lines.join(" ")
    )
    .to_lowercase();

    filter
        .split_whitespace()
        .all(|needle| haystack.contains(&needle.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_match_display_copy() {
        assert_eq!(UsageOverlayStatus::Good.label_for_display(), "healthy");
        assert_eq!(UsageOverlayStatus::Critical.icon(), "◆");
    }

    #[test]
    fn item_filter_searches_details_and_status() {
        let item = UsageOverlayItem::new(
            "claude",
            "Claude usage",
            "85% used",
            UsageOverlayStatus::Warning,
            vec!["resets tomorrow".to_string()],
        );
        assert!(item_matches_filter(&item, "watch tomorrow"));
        assert!(item_matches_filter(&item, "claude 85"));
        assert!(!item_matches_filter(&item, "openai"));
    }
}
