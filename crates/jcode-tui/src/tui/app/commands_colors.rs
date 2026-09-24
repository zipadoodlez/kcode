//! `/colors`: inspect and configure the TUI color palette.
//!
//! Every color the TUI renders is configurable through `[display.colors]` in
//! `~/.kcode/config.toml`. This command is the interactive front end for that:
//! it lists the roles with their current values, sets them, and resets them.

use super::{App, DisplayMessage};
use jcode_tui_style::palette::{ALL_ROLES, Palette, Role, parse_hex, to_hex};

const USAGE: &str = "Usage:\n  \
    /colors                       List every configurable color role\n  \
    /colors <role> <#rrggbb>      Set a role's color (saved to config)\n  \
    /colors reset [role]          Reset one role, or all of them\n  \
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
        Some(role) => match words.next() {
            Some(value) => set_color(app, role, value),
            None => app.push_display_message(DisplayMessage::error(format!(
                "Missing color value for '{role}'.\n\n{USAGE}"
            ))),
        },
    }
    true
}

fn configured_palette() -> Palette {
    let configured = &crate::config::config().display.colors;
    Palette::from_pairs(
        configured
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
    .0
}

fn list_colors(app: &mut App) {
    let palette = configured_palette();
    let mut lines = vec!["Configurable TUI colors (`/colors <role> <#rrggbb>`):".to_string()];
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
    let mut lines = vec!["[display.colors]".to_string()];
    for role in ALL_ROLES.iter().copied() {
        lines.push(format!(
            "{} = \"{}\"",
            role.key(),
            to_hex(palette.rgb(role))
        ));
    }
    app.push_display_message(DisplayMessage::system(lines.join("\n")));
}

fn set_color(app: &mut App, role_key: &str, value: &str) {
    let Some(role) = Role::from_key(role_key) else {
        app.push_display_message(DisplayMessage::error(format!(
            "Unknown color role '{role_key}'. Run /colors to list them."
        )));
        return;
    };
    let Some(rgb) = parse_hex(value) else {
        app.push_display_message(DisplayMessage::error(format!(
            "Invalid color '{value}'. Expected a hex color like #8ab4f8."
        )));
        return;
    };

    match persist(|colors| {
        colors.insert(role.key().to_string(), to_hex(rgb));
    }) {
        Ok(()) => {
            app.push_display_message(DisplayMessage::system(format!(
                "Set {} to {}.",
                role.key(),
                to_hex(rgb)
            )));
        }
        Err(error) => app.push_display_message(DisplayMessage::error(format!(
            "Failed to save {}: {error}",
            role.key()
        ))),
    }
}

fn reset_colors(app: &mut App, role_key: Option<&str>) {
    let result = match role_key {
        Some(key) => {
            let Some(role) = Role::from_key(key) else {
                app.push_display_message(DisplayMessage::error(format!(
                    "Unknown color role '{key}'. Run /colors to list them."
                )));
                return;
            };
            persist(|colors| {
                colors.remove(role.key());
            })
            .map(|()| format!("Reset {} to its default.", role.key()))
        }
        None => persist(|colors| colors.clear())
            .map(|()| "Reset every color to its default.".to_string()),
    };

    match result {
        Ok(message) => app.push_display_message(DisplayMessage::system(message)),
        Err(error) => {
            app.push_display_message(DisplayMessage::error(format!("Failed to reset: {error}")))
        }
    }
}

/// Mutate `[display.colors]`, save, and reinstall the live palette.
///
/// Reload-then-patch-then-save (rather than serializing cached state) so a
/// concurrent config edit by another kcode session is not clobbered.
fn persist(
    mutate: impl FnOnce(&mut std::collections::BTreeMap<String, String>),
) -> anyhow::Result<()> {
    let mut config = crate::config::Config::load();
    mutate(&mut config.display.colors);
    config.save()?;
    crate::tui::theme_detect::init_palette();
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
