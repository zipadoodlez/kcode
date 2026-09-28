//! Shared typo-resistant fuzzy matching adapters for TUI slash commands.

pub(crate) fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    kcode_fuzzy::command_fuzzy_score(needle, haystack)
}

pub(crate) fn fuzzy_match_positions(needle: &str, haystack: &str) -> Vec<usize> {
    kcode_fuzzy::command_fuzzy_match_positions(needle, haystack)
}

#[cfg(test)]
mod tests {
    #[test]
    fn slash_commands_tolerate_interior_typos() {
        assert!(kcode_fuzzy::command_fuzzy_match("/conifg", "/config").is_some());
        assert!(kcode_fuzzy::command_fuzzy_match("/comapct", "/compact").is_some());
        assert!(kcode_fuzzy::command_fuzzy_match("/memroy", "/memory").is_some());
    }

    #[test]
    fn slash_commands_remain_anchored() {
        assert!(kcode_fuzzy::command_fuzzy_match("/g", "/config").is_none());
        assert!(kcode_fuzzy::command_fuzzy_match("/g", "/goals").is_some());
    }
}
