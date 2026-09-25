//! `/colors`: inspect and configure the TUI color palette.
//!
//! Two layers in `~/.kcode/config.toml`:
//!
//! - `[display.palette]` - 16 base16 slots (`base00`..`base0f`, or the friendly
//!   names `bg`..`brown`). A published base16 theme pasted here recolors most of
//!   the UI at once.
//! - `[display.colors]` - per-role overrides, layered on top of the slots.
//!
//! This command is the interactive front end for both.

use super::{App, DisplayMessage};
use jcode_tui_style::palette::{
    ALL_ROLES, ALL_SLOTS, Palette, Role, Slot, default_slot_for, parse_hex, to_hex,
};

const USAGE: &str = "Usage:\n  \
    /colors                       List slots and color roles\n  \
    /colors <key> <#rrggbb>       Set a slot (base00..base0f) or role (saved to config)\n  \
    /colors reset [key]           Reset one key, or everything\n  \
    /colors export                Print the palette as config TOML";

pub(super) fn handle_colors_command(app: &mut App, trimmed: &str) -> bool {
    let Some(rest) = trimmed
        .strip_prefix("/colors")
        .or_else(|| trimmed.strip_prefix("/color"))
    else {
        return false;
    };
    // Only claim the exact command or `command <args>`, never `/colorsomething`.
    if !rest.is_empty() && !rest.starts_with(' ') {
        return false;
    }
    let rest = rest.trim();

    let mut words = rest.split_whitespace();
    match words.next() {
        None | Some("list") => list_colors(app),
        Some("export") => export_colors(app),
        Some("reset") => reset_colors(app, words.next()),
        Some(key) => match words.next() {
            Some(value) => set_color(app, key, value),
            None => app.push_display_message(DisplayMessage::error(format!(
                "Missing color value for '{key}'.\n\n{USAGE}"
            ))),
        },
    }
    true
}

/// The palette the TUI is currently rendering: slots then role overrides.
fn configured_palette() -> Palette {
    let config = crate::config::config();
    let (palette, _) = Palette::from_slot_pairs_over(
        Palette::default(),
        config
            .display
            .palette
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    Palette::from_pairs_over(
        palette,
        config
            .display
            .colors
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
    .0
}

fn list_colors(app: &mut App) {
    let palette = configured_palette();
    let mut lines = vec!["base16 palette slots (`/colors <slot> <#rrggbb>`):".to_string()];
    for slot in ALL_SLOTS.iter().copied() {
        let roles: Vec<&str> = ALL_ROLES
            .iter()
            .copied()
            .filter(|role| default_slot_for(*role) == slot)
            .map(Role::key)
            .collect();
        let shared = if roles.is_empty() {
            "(unused by default)".to_string()
        } else {
            roles.join(", ")
        };
        lines.push(format!(
            "  {:<10} {:<8} {}  -> {}",
            slot.key(),
            slot.base16_key(),
            to_hex(palette.slot_rgb(slot)),
            shared
        ));
    }
    lines.push(String::new());
    lines.push("Color roles (`/colors <role> <#rrggbb>`):".to_string());
    for role in ALL_ROLES.iter().copied() {
        let rgb = palette.rgb(role);
        let marker = if palette.is_overridden(role) {
            " (custom)"
        } else {
            ""
        };
        lines.push(format!("  {:<16} {}{}", role.key(), to_hex(rgb), marker));
    }
    lines.push(String::new());
    lines.push(
        "Only role-tagged colors follow this setting. Ad hoc shades used by individual widgets \
         carry no role and are left alone: give a shade a role if it should follow /colors."
            .to_string(),
    );
    app.push_display_message(DisplayMessage::system(lines.join("\n")));
}

fn export_colors(app: &mut App) {
    let palette = configured_palette();
    let mut lines = vec!["[display.palette]".to_string()];
    for slot in ALL_SLOTS.iter().copied() {
        lines.push(format!("{} = \"{}\"", slot.key(), to_hex(palette.slot_rgb(slot))));
    }
    lines.push(String::new());
    lines.push("[display.colors]".to_string());
    for role in ALL_ROLES.iter().copied() {
        lines.push(format!("{} = \"{}\"", role.key(), to_hex(palette.rgb(role))));
    }
    app.push_display_message(DisplayMessage::system(lines.join("\n")));
}

fn set_color(app: &mut App, key: &str, value: &str) {
    let Some(rgb) = parse_hex(value) else {
        app.push_display_message(DisplayMessage::error(format!(
            "Invalid color '{value}'. Expected a hex color like #8ab4f8."
        )));
        return;
    };
    let value = to_hex(rgb);

    if let Some(slot) = Slot::from_key(key) {
        match persist(|_, slots| {
            slots.insert(slot.key().to_string(), value.clone());
        }) {
            Ok(()) => app.push_display_message(DisplayMessage::system(format!(
                "Set slot {} ({}) to {}.",
                slot.key(),
                slot.base16_key(),
                to_hex(rgb)
            ))),
            Err(error) => app.push_display_message(DisplayMessage::error(format!(
                "Failed to save {}: {error}",
                slot.key()
            ))),
        }
        return;
    }

    let Some(role) = Role::from_key(key) else {
        app.push_display_message(DisplayMessage::error(format!(
            "Unknown color slot or role '{key}'. Run /colors to list them."
        )));
        return;
    };
    match persist(|colors, _| {
        colors.insert(role.key().to_string(), value.clone());
    }) {
        Ok(()) => app.push_display_message(DisplayMessage::system(format!(
            "Set {} to {}.",
            role.key(),
            to_hex(rgb)
        ))),
        Err(error) => app.push_display_message(DisplayMessage::error(format!(
            "Failed to save {}: {error}",
            role.key()
        ))),
    }
}

fn reset_colors(app: &mut App, key: Option<&str>) {
    let remove = |colors: &mut std::collections::BTreeMap<String, String>,
                  slots: &mut std::collections::BTreeMap<String, String>| {
        if let Some(key) = key {
            if let Some(slot) = Slot::from_key(key) {
                slots.remove(slot.key());
            } else if let Some(role) = Role::from_key(key) {
                colors.remove(role.key());
            }
        } else {
            colors.clear();
            slots.clear();
        }
    };
    let message = match key {
        Some(key) => format!("Reset {key} to its default."),
        None => "Reset every color to its default.".to_string(),
    };
    match persist(|colors, slots| remove(colors, slots)) {
        Ok(()) => app.push_display_message(DisplayMessage::system(message)),
        Err(error) => {
            app.push_display_message(DisplayMessage::error(format!("Failed to reset: {error}")))
        }
    }
}

/// Mutate `[display.colors]` and `[display.palette]`, save, and reinstall the
/// live palette.
///
/// Reload-then-patch-then-save (rather than serializing cached state) so a
/// concurrent config edit by another kcode session is not clobbered.
fn persist(
    mutate: impl FnOnce(
        &mut std::collections::BTreeMap<String, String>,
        &mut std::collections::BTreeMap<String, String>,
    ),
) -> anyhow::Result<()> {
    let mut config = crate::config::Config::load();
    mutate(&mut config.display.colors, &mut config.display.palette);
    config.save()?;
    crate::tui::palette_init::init_palette();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claims_the_colors_command() {
        // Guard against swallowing unrelated commands with a shared prefix.
        assert!(!"/colorscheme".starts_with("/colors "));
        assert!(Role::from_key("user").is_some());
        assert!(Role::from_key("not-a-role").is_none());
        assert_eq!(Slot::from_key("base05"), Some(Slot::Fg));
        assert_eq!(Slot::from_key("blue"), Some(Slot::Blue));
    }

    #[test]
    fn usage_text_documents_every_subcommand() {
        for subcommand in ["reset", "export"] {
            assert!(
                USAGE.contains(subcommand),
                "usage should document {subcommand}"
            );
        }
    }
}
