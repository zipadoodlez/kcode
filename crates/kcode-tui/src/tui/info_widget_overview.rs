use super::info_widget::{AuthMethod, InfoWidgetData, UsageProvider};

/// Height the overview box wants: its compact sections when it has any and they
/// fit the available inner height, zero otherwise.
pub(crate) fn overview_height(data: &InfoWidgetData, inner_height: u16) -> u16 {
    let height = compact_overview_height(data);
    if height == 0 || height > inner_height {
        return 0;
    }
    height
}

fn compact_context_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.context_info
        && info.total_chars > 0
    {
        return 1;
    }
    0
}

fn compact_model_height(data: &InfoWidgetData) -> u16 {
    if data.model.is_some() {
        let mut lines = 1u16;
        let has_provider = data
            .provider_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_some();
        if has_provider || data.auth_method != AuthMethod::Unknown {
            lines += 1;
        }
        // Mirror render_model_info: a blank session name alone produces no line.
        let has_session_line = data.session_count.is_some()
            || data
                .session_name
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty());
        if has_session_line {
            lines += 1;
        }
        lines
    } else {
        0
    }
}

fn compact_background_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.background_info
        && info.running_count > 0
    {
        let task_lines = info.running_tasks.len().min(3) as u16;
        let overflow_line = u16::from(info.running_tasks.len() > 3);
        return 1 + task_lines + overflow_line;
    }
    0
}

fn compact_usage_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.usage_info
        && info.available
    {
        // Must mirror render_usage_compact exactly, otherwise the compact
        // overview page either clips its last lines or reserves blank rows.
        if matches!(info.provider, UsageProvider::CostBased) {
            // Single "$cost · tokens" line.
            return 1;
        }
        // Subscription-style providers render an optional provider label plus
        // whichever primary, secondary, and Spark windows are actually present.
        let label = info.provider.label();
        let label_line = u16::from(!label.is_empty());
        let primary_line = u16::from(info.primary_limit_label.is_some());
        let secondary_line = u16::from(info.secondary_limit_label.is_some());
        let spark_line = u16::from(info.spark.is_some());
        return label_line + primary_line + secondary_line + spark_line;
    }
    0
}

fn compact_kv_cache_height(data: &InfoWidgetData) -> u16 {
    if data.cache_hit_info.is_some() { 1 } else { 0 }
}

fn compact_git_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.git_info
        && info.is_interesting()
    {
        return 1;
    }
    0
}

fn compact_overview_height(data: &InfoWidgetData) -> u16 {
    compact_model_height(data)
        + compact_context_height(data)
        + compact_background_height(data)
        + compact_usage_height(data)
        + compact_kv_cache_height(data)
        + compact_git_height(data)
}

#[cfg(test)]
mod tests {
    use super::overview_height;
    use crate::tui::info_widget::InfoWidgetData;

    #[test]
    fn overview_height_is_zero_when_a_section_does_not_fit() {
        let data = InfoWidgetData {
            model: Some("gpt-test".to_string()),
            ..Default::default()
        };
        assert_eq!(overview_height(&data, 8), 1);
        assert_eq!(overview_height(&data, 0), 0);
    }
}
