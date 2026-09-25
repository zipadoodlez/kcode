//! Fully user-configurable TUI colors.
//!
//! The TUI's colors come from two places:
//!
//! 1. Named semantic roles ([`Role`]) used by `theme.rs` accessors
//!    (`user_color()`, `ai_color()`, ...).
//! 2. Hundreds of ad hoc `rgb(r, g, b)` literals scattered across widgets.
//!
//! Both funnel through this module. Once per frame,
//! [`crate::display::adapt_buffer_for_display`] rewrites any buffer color
//! that is exactly a role's default onto that role's configured color, and maps
//! ratatui's named colors to the role they conventionally stand for. Ad hoc
//! `rgb(...)` literals carry no role, so they are not configurable: give a shade
//! a role if it needs to follow `/colors`. An unconfigured palette is
//! byte-identical to the historical hard-coded look.
//!
//! Configuration lives in `~/.kcode/config.toml`:
//!
//! ```toml
//! [display.colors]
//! user = "#8ab4f8"
//! ai = "#81c784"
//! accent = "#ba8bff"
//! ```

use ratatui::style::Color;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// A semantic color slot in the TUI.
///
/// Every role has a built-in default equal to the historical hard-coded value,
/// so adding a role never changes the default look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// User message text accent.
    User,
    /// Assistant message accent.
    Ai,
    /// Tool row label color.
    Tool,
    /// Clickable file paths.
    FileLink,
    /// Low-emphasis text (hints, separators).
    Dim,
    /// Primary brand accent (headers, highlights).
    Accent,
    /// System / harness notices.
    System,
    /// Queued-prompt indicator.
    Queued,
    /// ASAP-priority indicator.
    Asap,
    /// Pending / not-yet-run indicator.
    Pending,
    /// User message foreground text.
    UserText,
    /// User message panel background.
    UserBg,
    /// Assistant message foreground text.
    AiText,
    /// Header session icon.
    HeaderIcon,
    /// Header model/agent name.
    HeaderName,
    /// Header session id.
    HeaderSession,
    /// Success / additions.
    Success,
    /// Warnings.
    Warning,
    /// Errors / deletions.
    Error,
    /// Informational highlights.
    Info,
    /// Borders and rules.
    Border,
    /// Selected row background.
    SelectionBg,
}

/// All roles, in declaration order. Used by the `/colors` listing and its
/// completions so a new role is automatically covered by both.
pub const ALL_ROLES: &[Role] = &[
    Role::User,
    Role::Ai,
    Role::Tool,
    Role::FileLink,
    Role::Dim,
    Role::Accent,
    Role::System,
    Role::Queued,
    Role::Asap,
    Role::Pending,
    Role::UserText,
    Role::UserBg,
    Role::AiText,
    Role::HeaderIcon,
    Role::HeaderName,
    Role::HeaderSession,
    Role::Success,
    Role::Warning,
    Role::Error,
    Role::Info,
    Role::Border,
    Role::SelectionBg,
];

/// A base16-shaped palette slot.
///
/// Sixteen values, like a published base16 theme. Roles default to slots per
/// [`default_slot_for`], so pasting a theme's sixteen hex values recolors most
/// of the UI at once. A role can still be overridden individually through
/// `[display.colors]`, which layers on top of the slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    /// base00 - unused by default (no background role).
    Bg,
    /// base01 - user message panel background.
    BgAlt,
    /// base02 - selected row background.
    BgSelection,
    /// base03 - borders and rules, low-emphasis text.
    Comment,
    /// base04 - dim foreground (system/queued facts).
    FgDim,
    /// base05 - primary foreground text.
    Fg,
    /// base06 - unused by default.
    FgBright,
    /// base07 - unused by default.
    BgBright,
    /// base08 - errors.
    Red,
    /// base09 - ASAP indicator.
    Orange,
    /// base0A - warnings and pending.
    Yellow,
    /// base0B - success and assistant.
    Green,
    /// base0C - info and tool labels.
    Cyan,
    /// base0D - user, file links, session id.
    Blue,
    /// base0E - accent and header icon.
    Purple,
    /// base0F - unused by default.
    Brown,
}

/// All slots in base16 order.
pub const ALL_SLOTS: &[Slot] = &[
    Slot::Bg,
    Slot::BgAlt,
    Slot::BgSelection,
    Slot::Comment,
    Slot::FgDim,
    Slot::Fg,
    Slot::FgBright,
    Slot::BgBright,
    Slot::Red,
    Slot::Orange,
    Slot::Yellow,
    Slot::Green,
    Slot::Cyan,
    Slot::Blue,
    Slot::Purple,
    Slot::Brown,
];

const SLOT_BASE16_KEYS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08",
    "base09", "base0a", "base0b", "base0c", "base0d", "base0e", "base0f",
];

const SLOT_NAMES: [&str; 16] = [
    "bg", "bg_alt", "bg_selection", "comment", "fg_dim", "fg", "fg_bright", "bg_bright", "red",
    "orange", "yellow", "green", "cyan", "blue", "purple", "brown",
];

impl Slot {
    /// Index into [`ALL_SLOTS`] / the key tables.
    pub const fn index(self) -> usize {
        match self {
            Slot::Bg => 0,
            Slot::BgAlt => 1,
            Slot::BgSelection => 2,
            Slot::Comment => 3,
            Slot::FgDim => 4,
            Slot::Fg => 5,
            Slot::FgBright => 6,
            Slot::BgBright => 7,
            Slot::Red => 8,
            Slot::Orange => 9,
            Slot::Yellow => 10,
            Slot::Green => 11,
            Slot::Cyan => 12,
            Slot::Blue => 13,
            Slot::Purple => 14,
            Slot::Brown => 15,
        }
    }

    /// Canonical `[display.palette]` key (`bg`..`brown`).
    pub const fn key(self) -> &'static str {
        SLOT_NAMES[self.index()]
    }

    /// The base16 name (`base00`..`base0f`), also accepted in config.
    pub const fn base16_key(self) -> &'static str {
        SLOT_BASE16_KEYS[self.index()]
    }

    /// Look up a slot by its friendly name or base16 name.
    pub fn from_key(key: &str) -> Option<Slot> {
        let normalized = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        if let Some(index) = SLOT_NAMES.iter().position(|name| *name == normalized) {
            return Some(ALL_SLOTS[index]);
        }
        SLOT_BASE16_KEYS
            .iter()
            .position(|name| *name == normalized)
            .map(|index| ALL_SLOTS[index])
    }

    /// Built-in default color, taken from the role this slot primarily feeds.
    pub const fn default_rgb(self) -> (u8, u8, u8) {
        match self {
            Slot::Bg => (24, 24, 30),
            Slot::BgAlt => Role::UserBg.default_rgb(),
            Slot::BgSelection => Role::SelectionBg.default_rgb(),
            Slot::Comment => Role::Border.default_rgb(),
            Slot::FgDim => Role::Pending.default_rgb(),
            Slot::Fg => Role::UserText.default_rgb(),
            Slot::FgBright => (255, 255, 255),
            Slot::BgBright => (70, 70, 78),
            Slot::Red => Role::Error.default_rgb(),
            Slot::Orange => Role::Queued.default_rgb(),
            Slot::Yellow => Role::Warning.default_rgb(),
            Slot::Green => Role::Success.default_rgb(),
            Slot::Cyan => Role::Info.default_rgb(),
            Slot::Blue => Role::User.default_rgb(),
            Slot::Purple => Role::Accent.default_rgb(),
            Slot::Brown => (140, 90, 60),
        }
    }
}

/// The slot each role defaults to.
///
/// Adding a role means naming an existing slot (adding a slot changes the theme
/// contract and drops base16 portability).
pub const fn default_slot_for(role: Role) -> Slot {
    match role {
        Role::UserBg => Slot::BgAlt,
        Role::SelectionBg => Slot::BgSelection,
        Role::Dim | Role::Border => Slot::Comment,
        Role::System | Role::Queued => Slot::FgDim,
        Role::UserText | Role::AiText | Role::HeaderName => Slot::Fg,
        Role::Error => Slot::Red,
        Role::Asap => Slot::Orange,
        Role::Warning | Role::Pending => Slot::Yellow,
        Role::Success | Role::Ai => Slot::Green,
        Role::Info | Role::Tool => Slot::Cyan,
        Role::User | Role::FileLink | Role::HeaderSession => Slot::Blue,
        Role::Accent | Role::HeaderIcon => Slot::Purple,
    }
}

impl Role {
    /// Stable config key (also the `/colors` name).
    pub const fn key(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Ai => "ai",
            Role::Tool => "tool",
            Role::FileLink => "file_link",
            Role::Dim => "dim",
            Role::Accent => "accent",
            Role::System => "system",
            Role::Queued => "queued",
            Role::Asap => "asap",
            Role::Pending => "pending",
            Role::UserText => "user_text",
            Role::UserBg => "user_bg",
            Role::AiText => "ai_text",
            Role::HeaderIcon => "header_icon",
            Role::HeaderName => "header_name",
            Role::HeaderSession => "header_session",
            Role::Success => "success",
            Role::Warning => "warning",
            Role::Error => "error",
            Role::Info => "info",
            Role::Border => "border",
            Role::SelectionBg => "selection_bg",
        }
    }

    /// Look up a role by its config key (case/separator insensitive).
    pub fn from_key(key: &str) -> Option<Role> {
        let normalized = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        ALL_ROLES
            .iter()
            .copied()
            .find(|role| role.key() == normalized)
    }

    /// Built-in default RGB, matching jcode's historical hard-coded palette.
    pub const fn default_rgb(self) -> (u8, u8, u8) {
        match self {
            Role::User => (138, 180, 248),
            Role::Ai => (129, 199, 132),
            Role::Tool => (120, 120, 120),
            Role::FileLink => (180, 200, 255),
            Role::Dim => (80, 80, 80),
            Role::Accent => (186, 139, 255),
            Role::System => (255, 170, 220),
            Role::Queued => (255, 193, 7),
            Role::Asap => (110, 210, 255),
            Role::Pending => (140, 140, 140),
            Role::UserText => (245, 245, 255),
            Role::UserBg => (35, 40, 50),
            Role::AiText => (220, 220, 215),
            Role::HeaderIcon => (120, 210, 230),
            Role::HeaderName => (190, 210, 235),
            Role::HeaderSession => (255, 255, 255),
            Role::Success => (100, 200, 100),
            Role::Warning => (255, 200, 100),
            Role::Error => (255, 100, 100),
            Role::Info => (140, 180, 255),
            Role::Border => (100, 100, 110),
            Role::SelectionBg => (60, 60, 80),
        }
    }

    /// Whether this role is used as a background. Backgrounds are graded on
    /// different readability criteria than foreground text.
    pub const fn is_background(self) -> bool {
        matches!(self, Role::UserBg | Role::SelectionBg)
    }

    /// Baked light-terminal default.
    ///
    /// Generated once by the code that used to flip and contrast-repair the
    /// dark palette per frame on a light terminal. That transform is gone: a
    /// light terminal is now this palette, selected up front. Each value is the
    /// transform's output for the role on a `#e0e0e0` surface (backgrounds take
    /// the flip only, foregrounds the flip plus the 7:1 contrast repair).
    /// Regenerating needs the original math, which lives in git history.
    pub const fn light_rgb(self) -> (u8, u8, u8) {
        match self {
            Role::User => (7, 49, 117),
            Role::Ai => (36, 80, 38),
            Role::Tool => (71, 71, 71),
            Role::FileLink => (0, 20, 75),
            Role::Dim => (71, 71, 71),
            Role::Accent => (47, 0, 116),
            Role::System => (85, 0, 50),
            Role::Queued => (91, 68, 0),
            Role::Asap => (0, 76, 111),
            Role::Pending => (71, 71, 71),
            Role::UserText => (0, 0, 10),
            Role::UserBg => (205, 210, 220),
            Role::AiText => (40, 40, 35),
            Role::HeaderIcon => (17, 78, 92),
            Role::HeaderName => (20, 40, 65),
            Role::HeaderSession => (0, 0, 0),
            Role::Success => (29, 81, 29),
            Role::Warning => (99, 64, 0),
            Role::Error => (148, 0, 0),
            Role::Info => (0, 40, 115),
            Role::Border => (70, 70, 78),
            Role::SelectionBg => (175, 175, 195),
        }
    }
}

/// A complete set of role colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    entries: [(u8, u8, u8); ALL_ROLES.len()],
    /// Which roles the user explicitly configured. Only overridden roles
    /// participate in literal remapping, so a default palette is a no-op.
    overridden: [bool; ALL_ROLES.len()],
}

impl Default for Palette {
    fn default() -> Self {
        let mut entries = [(0, 0, 0); ALL_ROLES.len()];
        for (slot, role) in entries.iter_mut().zip(ALL_ROLES) {
            *slot = role.default_rgb();
        }
        Self {
            entries,
            overridden: [false; ALL_ROLES.len()],
        }
    }
}

/// Index of `role` within [`ALL_ROLES`].
///
/// A missing role would be a programming error (a variant added without listing
/// it), but this sits on the render path, so it must not panic a live session
/// over a palette lookup. Falling back to index 0 renders one role with another
/// role's color, which is cosmetic; `every_role_is_indexable` catches the
/// mistake in CI instead.
fn index_of(role: Role) -> usize {
    ALL_ROLES
        .iter()
        .position(|candidate| *candidate == role)
        .unwrap_or(0)
}

impl Palette {
    /// The baked light-terminal palette: every role replaced by [`Role::light_rgb`].
    ///
    /// Every entry counts as configured, so the display pass repaints the whole
    /// UI from the dark defaults this palette replaces.
    pub fn light() -> Self {
        let mut palette = Self::default();
        for role in ALL_ROLES.iter().copied() {
            palette.set(role, role.light_rgb());
        }
        palette
    }

    /// RGB for `role`.
    pub fn rgb(&self, role: Role) -> (u8, u8, u8) {
        self.entries[index_of(role)]
    }

    /// Ratatui color for `role`, quantized for 256-color terminals.
    pub fn color(&self, role: Role) -> Color {
        let (r, g, b) = self.rgb(role);
        crate::color::rgb(r, g, b)
    }

    /// Whether the user explicitly set `role`.
    pub fn is_overridden(&self, role: Role) -> bool {
        self.overridden[index_of(role)]
    }

    /// Override `role`.
    pub fn set(&mut self, role: Role, rgb: (u8, u8, u8)) {
        let index = index_of(role);
        self.entries[index] = rgb;
        self.overridden[index] = true;
    }

    /// Whether any role is overridden (fast path guard for remapping).
    pub fn has_overrides(&self) -> bool {
        self.overridden.iter().any(|flag| *flag)
    }

    /// Apply a base16 slot: every role that defaults to `slot` takes `rgb`.
    ///
    /// Roles are marked overridden so the display pass remaps them. A role set
    /// explicitly through `[display.colors]` afterwards wins.
    pub fn apply_slot(&mut self, slot: Slot, rgb: (u8, u8, u8)) {
        for role in ALL_ROLES.iter().copied() {
            if default_slot_for(role) == slot {
                self.set(role, rgb);
            }
        }
    }

    /// RGB for `slot` in this palette: the first overridden role that maps to
    /// it, else the slot default.
    pub fn slot_rgb(&self, slot: Slot) -> (u8, u8, u8) {
        for role in ALL_ROLES.iter().copied() {
            if default_slot_for(role) == slot && self.is_overridden(role) {
                return self.rgb(role);
            }
        }
        slot.default_rgb()
    }

    /// Build a palette from `slot = "#rrggbb"` pairs layered over `base`.
    pub fn from_slot_pairs_over<'a, I>(base: Self, pairs: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut palette = base;
        let mut errors = Vec::new();
        for (key, value) in pairs {
            match (Slot::from_key(key), parse_hex(value)) {
                (Some(slot), Some(rgb)) => palette.apply_slot(slot, rgb),
                (None, _) => errors.push(format!("unknown palette slot '{key}'")),
                (Some(slot), None) => errors.push(format!(
                    "invalid color '{value}' for '{}' (expected #rrggbb)",
                    slot.key()
                )),
            }
        }
        (palette, errors)
    }

    /// Build a palette from `key = "#rrggbb"` pairs, returning per-entry
    /// errors instead of failing the whole palette so one typo cannot make the
    /// TUI unstyled.
    pub fn from_pairs<'a, I>(pairs: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        Self::from_pairs_over(Self::default(), pairs)
    }

    /// [`Self::from_pairs`] layered over `base`, so a preset (such as
    /// [`Self::light`]) can be the starting point for user overrides.
    pub fn from_pairs_over<'a, I>(base: Self, pairs: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut palette = base;
        let mut errors = Vec::new();
        for (key, value) in pairs {
            match (Role::from_key(key), parse_hex(value)) {
                (Some(role), Some(rgb)) => palette.set(role, rgb),
                (None, _) => errors.push(format!("unknown color role '{key}'")),
                (Some(role), None) => errors.push(format!(
                    "invalid color '{value}' for '{}' (expected #rrggbb)",
                    role.key()
                )),
            }
        }
        (palette, errors)
    }
}

/// Parse `#rrggbb`, `#rgb`, or a bare `rrggbb` hex string.
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().trim_start_matches('#');
    // `from_str_radix` failing here just means "not a hex color", which callers
    // surface to the user as an invalid-color message, so the error value itself
    // carries nothing extra.
    let byte = |slice: &str| u8::from_str_radix(slice, 16).ok();
    match hex.len() {
        3 => {
            let mut chars = hex.chars();
            let mut next = || {
                let c = chars.next()?;
                byte(&format!("{c}{c}"))
            };
            Some((next()?, next()?, next()?))
        }
        6 => Some((byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?)),
        _ => None,
    }
}

/// Format an RGB triple as `#rrggbb`.
pub fn to_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

static ACTIVE: RwLock<Option<Palette>> = RwLock::new(None);

/// Lock-free fast path for the per-cell substitution, which sits on the render
/// hot path. Palettes without overrides (the default) must not pay a lock.
static HAS_OVERRIDES: AtomicBool = AtomicBool::new(false);

/// Install the active palette. Called once at startup from config, and again
/// when the user changes colors at runtime.
pub fn set_palette(palette: Palette) {
    // Recover from a poisoned lock rather than silently keeping the old
    // palette: a panic elsewhere must not leave the user's configured colors
    // permanently unapplied for the rest of the session.
    let mut active = ACTIVE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *active = Some(palette);
    drop(active);
    HAS_OVERRIDES.store(palette.has_overrides(), Ordering::Relaxed);
}

/// The active palette.
///
/// Before configuration is loaded this is the built-in palette, which is what
/// every historical call site rendered, so an unconfigured session looks exactly
/// as it always has.
pub fn palette() -> Palette {
    // `unwrap_or_default` here is the built-in palette, not a swallowed error:
    // `None` means "config has not been loaded yet", and the default palette is
    // exactly what every historical call site rendered.
    (*ACTIVE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner()))
    .unwrap_or_default()
}

/// Avoid locking/copying the palette on the default render path.
pub(crate) fn configured_palette() -> Option<Palette> {
    HAS_OVERRIDES.load(Ordering::Relaxed).then(palette)
}

/// Resolve an override from the original, native-palette color.
///
/// A color is attributed only when it *is* a role's default (or a named color
/// with a role), so an override can never recolor a color some other role
/// carries. Ad hoc `rgb(...)` literals are unattributed and returned as-is:
/// give a shade a role if it should follow `/colors`.
pub(crate) fn configured_native_color(palette: &Palette, color: Color) -> Option<Color> {
    let rgb = match color {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(index) => crate::color::indexed_to_rgb(index),
        named => {
            let mapped = remap_named_with(palette, named);
            return (mapped != named).then_some(mapped);
        }
    };
    // Only the role whose own default this is may claim it.
    let role = ALL_ROLES
        .iter()
        .copied()
        .find(|role| role.default_rgb() == rgb && palette.is_overridden(*role))?;
    let (r, g, b) = palette.rgb(role);
    Some(crate::color::rgb(r, g, b))
}

/// Resolve a role to a renderable color.
///
/// This deliberately returns the role's *default* color, not the configured
/// one: substitution happens once per frame in
/// [`crate::display::adapt_buffer_for_display`]. Returning the configured
/// color here would let
/// the same cell be remapped twice (once by the accessor, once by the buffer
/// pass), which compounds the hue/lightness offsets.
pub fn role_color(role: Role) -> Color {
    let (r, g, b) = role.default_rgb();
    crate::color::rgb(r, g, b)
}

/// Map a terminal-named color (`Color::White`, `Color::Red`, ...) onto the
/// configured palette.
///
/// Widgets also use ratatui's named colors, which carry no RGB for literal
/// matching. Named colors are mapped to the semantic role they conventionally
/// stand for, and left untouched when that role is not configured, so default
/// behavior is unchanged.
pub fn remap_named_with(palette: &Palette, color: Color) -> Color {
    let role = match color {
        Color::Red | Color::LightRed => Role::Error,
        Color::Green | Color::LightGreen => Role::Success,
        Color::Yellow | Color::LightYellow => Role::Warning,
        Color::Blue | Color::LightBlue => Role::Info,
        Color::Magenta | Color::LightMagenta => Role::Accent,
        Color::Cyan | Color::LightCyan => Role::HeaderIcon,
        Color::White => Role::AiText,
        Color::Gray | Color::DarkGray => Role::Dim,
        // Used as a panel/inverse background rather than as text.
        Color::Black => Role::UserBg,
        // `Reset` is the terminal's own default and must stay untouched, which
        // is what lets the user's real background show through.
        _ => return color,
    };
    if !palette.is_overridden(role) {
        return color;
    }
    let (r, g, b) = palette.rgb(role);
    crate::color::rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_unique_key_and_roundtrips() {
        let mut keys = std::collections::HashSet::new();
        for role in ALL_ROLES.iter().copied() {
            assert!(keys.insert(role.key()), "duplicate key {}", role.key());
            assert_eq!(Role::from_key(role.key()), Some(role));
            // Accept the friendlier dashed/uppercase spellings too.
            assert_eq!(Role::from_key(&role.key().replace('_', "-")), Some(role));
        }
    }

    /// `index_of` falls back instead of panicking on the render path, so the
    /// invariant it relies on is enforced here.
    #[test]
    fn every_role_is_indexable() {
        for (expected, role) in ALL_ROLES.iter().copied().enumerate() {
            assert_eq!(
                index_of(role),
                expected,
                "{} must be listed in ALL_ROLES at its own index",
                role.key()
            );
        }
    }

    #[test]
    fn parses_hex_in_common_spellings() {
        assert_eq!(parse_hex("#8ab4f8"), Some((138, 180, 248)));
        assert_eq!(parse_hex("8AB4F8"), Some((138, 180, 248)));
        assert_eq!(parse_hex("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn default_palette_matches_historical_values() {
        let palette = Palette::default();
        assert_eq!(palette.rgb(Role::User), (138, 180, 248));
        assert!(!palette.has_overrides());
    }

    /// The contract, in one place: only a color that *is* a role's default is
    /// attributed to that role. Ad hoc shades carry no role and are left alone,
    /// which is the trade this design makes: configurable means role-tagged.
    #[test]
    fn only_exact_role_defaults_follow_an_override() {
        let mut palette = Palette::default();
        palette.set(Role::Warning, (80, 220, 120));
        let (r, g, b) = Role::Warning.default_rgb();
        assert_eq!(
            configured_native_color(&palette, Color::Rgb(r, g, b)),
            Some(crate::color::rgb(80, 220, 120))
        );
        // An amber shade near the warning default is a plain literal, not the role.
        assert_eq!(
            configured_native_color(&palette, Color::Rgb(200, 150, 70)),
            None
        );
        // An unconfigured palette attributes nothing, so the default frame is
        // untouched.
        assert_eq!(
            configured_native_color(&Palette::default(), Color::Rgb(r, g, b)),
            None
        );
    }

    #[test]
    fn from_pairs_reports_errors_without_dropping_valid_entries() {
        let (palette, errors) = Palette::from_pairs([
            ("accent", "#ff0000"),
            ("bogus", "#00ff00"),
            ("ai", "not-a-color"),
        ]);
        assert_eq!(palette.rgb(Role::Accent), (255, 0, 0));
        assert_eq!(palette.rgb(Role::Ai), Role::Ai.default_rgb());
        assert_eq!(errors.len(), 2);
    }
}

#[cfg(test)]
mod named_colors {
    use super::*;

    /// Every ratatui named color the TUI actually uses must map to a role,
    /// except `Reset`.
    ///
    /// Named colors carry no RGB for literal matching to work with, so an
    /// unmapped one is a color the user simply cannot change. `Color::Black`
    /// was exactly that until this test existed.
    #[test]
    fn every_named_color_used_by_the_tui_is_configurable() {
        let used = [
            Color::White,
            Color::Black,
            Color::Gray,
            Color::DarkGray,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
        ];

        // Override every role so the only reason a color stays put is that it
        // has no mapping at all.
        let mut palette = Palette::default();
        for role in ALL_ROLES.iter().copied() {
            let (r, g, b) = role.default_rgb();
            palette.set(role, (r.wrapping_add(40), g, b));
        }

        for color in used {
            assert_ne!(
                remap_named_with(&palette, color),
                color,
                "{color:?} is used by the TUI but maps to no role, so a user cannot change it"
            );
        }
    }

    /// `Reset` must survive: it is how the terminal's own background shows
    /// through, on both light and dark themes.
    #[test]
    fn reset_is_never_substituted() {
        let mut palette = Palette::default();
        for role in ALL_ROLES.iter().copied() {
            palette.set(role, (1, 2, 3));
        }
        assert_eq!(remap_named_with(&palette, Color::Reset), Color::Reset);
    }
}

#[cfg(test)]
mod default_palette_is_frozen {
    use super::*;

    /// The exact hand-tuned palette kcode has always shipped.
    ///
    /// This is a deliberate, redundant copy of [`Role::default_rgb`]. It exists
    /// so the shipped look cannot drift: the repair pass consumes these values,
    /// and it would be easy to "improve" a default while tuning it. Any change
    /// here is a change to what every existing user sees on launch, so it must be
    /// a deliberate edit to this table rather than a side effect of tooling work.
    ///
    /// Values were chosen by hand and are not derived from any metric.
    const HAND_TUNED: &[(Role, (u8, u8, u8))] = &[
        (Role::User, (138, 180, 248)),
        (Role::Ai, (129, 199, 132)),
        (Role::Tool, (120, 120, 120)),
        (Role::FileLink, (180, 200, 255)),
        (Role::Dim, (80, 80, 80)),
        (Role::Accent, (186, 139, 255)),
        (Role::System, (255, 170, 220)),
        (Role::Queued, (255, 193, 7)),
        (Role::Asap, (110, 210, 255)),
        (Role::Pending, (140, 140, 140)),
        (Role::UserText, (245, 245, 255)),
        (Role::UserBg, (35, 40, 50)),
        (Role::AiText, (220, 220, 215)),
        (Role::HeaderIcon, (120, 210, 230)),
        (Role::HeaderName, (190, 210, 235)),
        (Role::HeaderSession, (255, 255, 255)),
        (Role::Success, (100, 200, 100)),
        (Role::Warning, (255, 200, 100)),
        (Role::Error, (255, 100, 100)),
        (Role::Info, (140, 180, 255)),
        (Role::Border, (100, 100, 110)),
        (Role::SelectionBg, (60, 60, 80)),
    ];

    #[test]
    fn every_role_keeps_its_hand_tuned_default() {
        for (role, expected) in HAND_TUNED.iter().copied() {
            assert_eq!(
                role.default_rgb(),
                expected,
                "{} changed from its hand-tuned default {expected:?}. If this is intentional, \
                 update HAND_TUNED too and understand that every existing user's colors change.",
                role.key()
            );
        }
        assert_eq!(
            HAND_TUNED.len(),
            ALL_ROLES.len(),
            "a role was added or removed without recording its hand-tuned default"
        );
    }

    /// An unconfigured palette must resolve to exactly that table, so the
    /// default *experience* is the hand-tuned one and not merely the constants.
    #[test]
    fn unconfigured_palette_resolves_to_the_hand_tuned_table() {
        let palette = Palette::default();
        for (role, expected) in HAND_TUNED.iter().copied() {
            assert_eq!(palette.rgb(role), expected, "{}", role.key());
            assert!(
                !palette.is_overridden(role),
                "{} must not be marked as user-configured by default",
                role.key()
            );
        }
        assert!(
            !palette.has_overrides(),
            "the default palette must claim no overrides, or literal remapping would engage"
        );
    }

    /// The shipped default must stay immutable: every palette operation takes
    /// `&Palette`, and a future refactor could plausibly reach for the global.
    #[test]
    fn default_palette_is_immutable() {
        let before = Palette::default();
        assert_eq!(
            Palette::default(),
            before,
            "the default palette must be immutable"
        );
        for (role, expected) in HAND_TUNED.iter().copied() {
            assert_eq!(role.default_rgb(), expected, "{}", role.key());
        }
    }

    /// A published base16 theme pasted into `[display.palette]` must recolor the
    /// roles mapped to each slot, and a per-role override must still win.
    #[test]
    fn base16_slots_recolor_their_roles() {
        let (slotted, errors) = Palette::from_slot_pairs_over(
            Palette::default(),
            [("base08", "#ff0000"), ("blue", "#0000ff")],
        );
        assert!(errors.is_empty(), "{errors:?}");
        // `base08` (red) feeds Error; `blue` (base0D) feeds User and FileLink.
        assert_eq!(slotted.rgb(Role::Error), (255, 0, 0));
        assert_eq!(slotted.rgb(Role::User), (0, 0, 255));
        assert_eq!(slotted.rgb(Role::FileLink), (0, 0, 255));
        // A slot the theme does not set keeps the built-in default.
        assert_eq!(slotted.rgb(Role::Success), Role::Success.default_rgb());
        assert_eq!(slotted.slot_rgb(Slot::Blue), (0, 0, 255));

        // A per-role override layers on top of the slot.
        let (palette, errors) = Palette::from_pairs_over(slotted, [("error", "#00ff00")]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(palette.rgb(Role::Error), (0, 255, 0));
    }
}
