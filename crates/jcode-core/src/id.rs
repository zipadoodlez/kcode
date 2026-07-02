use chrono::Utc;

pub fn new_id(prefix: &str) -> String {
    let ts = Utc::now().timestamp_millis();
    let rand: u64 = rand::random();
    format!("{}_{}_{}", prefix, ts, rand)
}

/// Server/location names with their icons.
///
/// Servers now use location nouns while sessions use client/entity nouns,
/// producing names like "harbor fox" or "observatory otter".
///
/// Icon constraints match `SESSION_NAMES`: single codepoints with default
/// emoji presentation (no VS16), see the comment there.
const SERVER_MODIFIERS: &[(&str, &str)] = &[
    // Natural places
    ("cove", "🌊"),
    ("grove", "🌳"),
    ("meadow", "🌾"),
    ("marsh", "🌿"),
    ("lake", "🛶"),
    ("river", "🚣"),
    ("creek", "💧"),
    ("brook", "💧"),
    ("cliff", "🧗"),
    ("peak", "🗻"),
    ("summit", "🚠"),
    ("forest", "🌲"),
    ("garden", "🌷"),
    ("island", "🌴"),
    ("desert", "🌵"),
    ("beach", "🏄"),
    // Built places
    ("harbor", "⚓"),
    ("camp", "⛺"),
    ("forge", "🔥"),
    ("citadel", "🏯"),
    ("station", "🚉"),
    ("observatory", "🔭"),
    ("workshop", "🔨"),
    ("lighthouse", "🗼"),
    ("temple", "⛪"),
    ("castle", "🏰"),
    ("bridge", "🌉"),
    ("fountain", "⛲"),
    ("stadium", "🎪"),
    ("factory", "🏭"),
    ("pagoda", "🛕"),
    ("hut", "🛖"),
];

/// Session/client names with their icons.
const SESSION_NAMES: &[(&str, &str)] = &[
    // Animals and client entities. Every emoji here is a single, widely-supported
    // codepoint (Unicode <= 12.0, no ZWJ sequences) with *default emoji
    // presentation* (no VS16 / U+FE0F needed). Text-default codepoints that rely
    // on VS16 render as monochrome outlines or tofu in macOS window titles
    // (Ghostty/Terminal tab and titlebar fonts ignore the selector), so they are
    // banned by `session_icons_render_as_single_safe_glyphs`.
    ("ant", "🐜"),
    ("bat", "🦇"),
    ("bee", "🐝"),
    ("bird", "🐦"),
    ("bug", "🐛"),
    ("cat", "🐱"),
    ("chicken", "🐔"),
    ("chick", "🐥"),
    ("chipmunk", "🌰"),
    ("cow", "🐄"),
    ("crocodile", "🐊"),
    ("cricket", "🦗"),
    ("dog", "🐕"),
    ("dove", "🤍"),
    ("eagle", "🦅"),
    ("fish", "🐟"),
    ("fox", "🦊"),
    ("giraffe", "🦒"),
    ("hamster", "🐹"),
    ("ladybug", "🐞"),
    ("lobster", "🦞"),
    ("mosquito", "🦟"),
    ("owl", "🦉"),
    ("ox", "🐂"),
    ("pig", "🐷"),
    ("rat", "🐀"),
    ("ram", "🐏"),
    ("rooster", "🐓"),
    ("shrimp", "🦐"),
    ("sauropod", "🦕"),
    ("blowfish", "🐡"),
    ("buffalo", "🐃"),
    ("butterfly", "🦋"),
    ("badger", "🦡"),
    ("bear", "🐻"),
    ("crab", "🦀"),
    ("deer", "🦌"),
    ("duck", "🦆"),
    ("frog", "🐸"),
    ("goat", "🐐"),
    ("lion", "🦁"),
    ("wolf", "🐺"),
    ("horse", "🐴"),
    ("koala", "🐨"),
    ("llama", "🦙"),
    ("mouse", "🐭"),
    ("otter", "🦦"),
    ("panda", "🐼"),
    ("peacock", "🦚"),
    ("penguin", "🐧"),
    ("shark", "🦈"),
    ("sheep", "🐑"),
    ("sloth", "🦥"),
    ("snail", "🐌"),
    ("snake", "🐍"),
    ("spider", "🧶"),
    ("squid", "🦑"),
    ("swan", "🦢"),
    ("t-rex", "🦖"),
    ("tiger", "🐯"),
    ("turkey", "🦃"),
    ("whale", "🐋"),
    ("turtle", "🐢"),
    ("rabbit", "🐰"),
    ("parrot", "🦜"),
    ("jaguar", "🐆"),
    ("lizard", "🦎"),
    ("monkey", "🐒"),
    ("gorilla", "🦍"),
    ("orangutan", "🦧"),
    ("camel", "🐫"),
    ("elephant", "🐘"),
    ("rhino", "🦏"),
    ("hippo", "🦛"),
    ("boar", "🐗"),
    ("unicorn", "🦄"),
    ("kangaroo", "🦘"),
    ("hedgehog", "🦔"),
    ("skunk", "🦨"),
    ("raccoon", "🦝"),
    ("flamingo", "🦩"),
    ("dolphin", "🐬"),
    ("octopus", "🐙"),
    ("scorpion", "🦂"),
    ("zebra", "🦓"),
    ("stallion", "🐎"),
    ("dromedary", "🐪"),
    ("hog", "🐖"),
    ("kitten", "🐈"),
    ("poodle", "🐩"),
    ("hare", "🐇"),
    ("vole", "🐁"),
    ("dragon", "🐉"),
    ("humpback", "🐳"),
    ("guppy", "🐠"),
    ("nautilus", "🐚"),
    ("hatchling", "🐣"),
    ("wyvern", "🐲"),
    ("calf", "🐮"),
    ("macaque", "🐵"),
    ("tigress", "🐅"),
];

/// Get an emoji icon for a session/client name word.
pub fn session_icon(name: &str) -> &'static str {
    SESSION_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, icon)| *icon)
        .unwrap_or("💫")
}

/// Get an emoji icon for a server/location name word.
pub fn server_icon(name: &str) -> &'static str {
    SERVER_MODIFIERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, icon)| *icon)
        .unwrap_or("🔮")
}

/// Generate a memorable server name using a location noun.
/// Returns (full_id, short_name) where:
/// - full_id is the storage identifier like "server_blazing_1234567890_deadbeefcafebabe"
/// - short_name is the memorable part like "blazing"
pub fn new_memorable_server_id() -> (String, String) {
    let ts = Utc::now().timestamp_millis();
    let rand: u64 = rand::random();

    // Use the random value to pick a location noun.
    let idx = (rand as usize) % SERVER_MODIFIERS.len();
    let (word, _) = SERVER_MODIFIERS[idx];

    let short_name = word.to_string();
    let full_id = format!("server_{}_{ts}_{rand:016x}", word);

    (full_id, short_name)
}

/// Try to extract the memorable name from a server ID
/// e.g., "server_blazing_1234567890_deadbeefcafebabe" -> Some("blazing")
#[cfg(test)]
pub fn extract_server_name(server_id: &str) -> Option<&str> {
    if let Some(rest) = server_id.strip_prefix("server_")
        && let Some(pos) = rest.find('_')
    {
        return Some(&rest[..pos]);
    }
    None
}

/// Generate a memorable session name
/// Returns (full_id, short_name) where:
/// - full_id is the storage identifier like "session_fox_1234567890_deadbeefcafebabe"
/// - short_name is the memorable part like "fox"
pub fn new_memorable_session_id() -> (String, String) {
    let ts = Utc::now().timestamp_millis();
    let rand: u64 = rand::random();

    // Use the random value to pick a word
    let idx = (rand as usize) % SESSION_NAMES.len();
    let (word, _) = SESSION_NAMES[idx];

    let short_name = word.to_string();
    let full_id = format!("session_{}_{ts}_{rand:016x}", word);

    (full_id, short_name)
}

/// Try to extract the memorable name from a session ID
/// e.g., "session_fox_1234567890_deadbeefcafebabe" -> Some("fox")
pub fn extract_session_name(session_id: &str) -> Option<&str> {
    if let Some(rest) = session_id.strip_prefix("session_") {
        // Session names are the first token after the prefix.
        // This supports both old IDs (session_name_ts) and new IDs
        // with an added random suffix (session_name_ts_rand).
        if let Some(pos) = rest.find('_') {
            return Some(&rest[..pos]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_memorable_session_id() {
        let (full_id, short_name) = new_memorable_session_id();

        // Full ID should start with "session_"
        assert!(full_id.starts_with("session_"));

        // Short name should be non-empty
        assert!(!short_name.is_empty());

        // Full ID should contain the short name
        assert!(full_id.contains(&short_name));

        // Short name should have a specific icon (not default)
        let icon = session_icon(&short_name);
        assert_ne!(
            icon, "💫",
            "Name '{}' should have a specific icon",
            short_name
        );
    }

    #[test]
    fn test_extract_session_name() {
        assert_eq!(extract_session_name("session_fox_1234567890"), Some("fox"));
        assert_eq!(
            extract_session_name("session_fox_1234567890_deadbeefcafebabe"),
            Some("fox")
        );
        assert_eq!(
            extract_session_name("session_blue-whale_1234567890"),
            Some("blue-whale")
        );
        assert_eq!(
            extract_session_name("session_blue-whale_1234567890_deadbeefcafebabe"),
            Some("blue-whale")
        );
        assert_eq!(
            extract_session_name("session_1234567890_9876543210"),
            Some("1234567890")
        );
        assert_eq!(extract_session_name("invalid"), None);
        assert_eq!(extract_session_name("session_"), None);
    }

    #[test]
    fn test_unique_session_ids() {
        let ids: std::collections::HashSet<String> =
            (0..512).map(|_| new_memorable_session_id().0).collect();
        assert_eq!(
            ids.len(),
            512,
            "session IDs should stay unique in tight bursts"
        );
    }

    #[test]
    fn test_all_names_have_icons() {
        for (name, expected_icon) in SESSION_NAMES {
            let icon = session_icon(name);
            assert_eq!(icon, *expected_icon, "Icon mismatch for '{}'", name);
            assert_ne!(icon, "💫", "Name '{}' should have a specific icon", name);
        }
    }

    /// Returns true for emoji that commonly fail to render as a single glyph on
    /// older terminal fonts or in window titles: ZWJ sequences (split into
    /// pieces), codepoints added in Unicode 13.0 or later (rendered as tofu
    /// boxes on fonts that predate them), and VS16 variation sequences
    /// (text-default codepoints + U+FE0F, which macOS window/tab title fonts
    /// render as monochrome outlines or tofu because the title renderer
    /// ignores the emoji-presentation selector - the Ghostty-on-macOS bug).
    /// We avoid a broad block range here because the Supplemental Symbols
    /// block mixes safe Unicode 11/12 emoji (otter, sloth) with risky Unicode
    /// 13+ ones (mammoth, beaver), so we list the unsafe codepoints
    /// explicitly.
    fn is_fragile_emoji(emoji: &str) -> bool {
        // Unicode 13.0+ additions in the Supplemental Symbols block (U+1F900..U+1F9FF).
        const UNSAFE_SUPPLEMENTAL: &[u32] = &[
            0x1F9A3, // 🦣 mammoth (13.0)
            0x1F9A4, // 🦤 dodo (13.0)
            0x1F9AB, // 🦫 beaver (13.0)
            0x1F9AC, // 🦬 bison (13.0)
            0x1F9AD, // 🦭 seal (13.0)
        ];
        emoji.chars().any(|c| {
            let cp = c as u32;
            c == '\u{200D}'
                // VS16: emoji needing it are text-default and misrender in titles.
                || c == '\u{FE0F}'
                // Symbols and Pictographs Extended-A (entirely Unicode 13+).
                || (0x1FA70..=0x1FAFF).contains(&cp)
                || UNSAFE_SUPPLEMENTAL.contains(&cp)
        })
    }

    #[test]
    fn session_icons_render_as_single_safe_glyphs() {
        for (name, emoji) in SESSION_NAMES {
            assert!(
                !is_fragile_emoji(emoji),
                "session name '{}' uses fragile emoji '{}' (ZWJ or Unicode 13+); \
                 pick a single widely-supported codepoint instead",
                name,
                emoji
            );
        }
    }

    #[test]
    fn session_names_and_icons_are_unique() {
        let mut names = std::collections::HashSet::new();
        let mut icons = std::collections::HashSet::new();
        for (name, emoji) in SESSION_NAMES {
            assert!(names.insert(*name), "duplicate session name '{}'", name);
            assert!(
                icons.insert(*emoji),
                "duplicate session icon '{}' (reused by '{}')",
                emoji,
                name
            );
        }
    }

    #[test]
    fn server_icons_render_as_single_safe_glyphs() {
        for (name, emoji) in SERVER_MODIFIERS {
            assert!(
                !is_fragile_emoji(emoji),
                "server name '{}' uses fragile emoji '{}' (ZWJ or Unicode 13+); \
                 pick a single widely-supported codepoint instead",
                name,
                emoji
            );
        }
    }

    #[test]
    fn test_new_memorable_server_id() {
        let (full_id, short_name) = new_memorable_server_id();

        // Full ID should start with "server_"
        assert!(full_id.starts_with("server_"));

        // Short name should be non-empty
        assert!(!short_name.is_empty());

        // Full ID should contain the short name
        assert!(full_id.contains(&short_name));

        // Short name should have a specific icon (not default)
        let icon = server_icon(&short_name);
        assert_ne!(
            icon, "🔮",
            "Modifier '{}' should have a specific icon",
            short_name
        );
    }

    #[test]
    fn test_extract_server_name() {
        assert_eq!(
            extract_server_name("server_blazing_1234567890"),
            Some("blazing")
        );
        assert_eq!(
            extract_server_name("server_blazing_1234567890_deadbeefcafebabe"),
            Some("blazing")
        );
        assert_eq!(
            extract_server_name("server_rising_1234567890"),
            Some("rising")
        );
        assert_eq!(extract_server_name("invalid"), None);
        assert_eq!(extract_server_name("server_"), None);
    }

    #[test]
    fn test_unique_server_ids() {
        let ids: std::collections::HashSet<String> =
            (0..256).map(|_| new_memorable_server_id().0).collect();
        assert_eq!(
            ids.len(),
            256,
            "server IDs should stay unique in tight bursts"
        );
    }

    #[test]
    fn test_all_modifiers_have_icons() {
        for (name, expected_icon) in SERVER_MODIFIERS {
            let icon = server_icon(name);
            assert_eq!(icon, *expected_icon, "Icon mismatch for '{}'", name);
            assert_ne!(
                icon, "🔮",
                "Modifier '{}' should have a specific icon",
                name
            );
        }
    }
}
