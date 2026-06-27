//! Visual preview of the swarm gallery layout against mock streams.
//!
//! Run with: `cargo run --profile selfdev -p jcode-tui-render --example swarm_gallery_preview`

use jcode_tui_render::swarm_tiles::{SwarmGalleryConfig, SwarmTile, render_swarm_gallery};
use ratatui::prelude::*;

fn accent(status: &str) -> Color {
    match status {
        "running" => Color::Rgb(255, 200, 100),
        "thinking" => Color::Rgb(140, 180, 255),
        "done" => Color::Rgb(100, 200, 100),
        "blocked" => Color::Rgb(255, 170, 80),
        "failed" => Color::Rgb(255, 100, 100),
        _ => Color::Rgb(140, 140, 150),
    }
}

fn mk(name: &str, role: Option<&str>, status: &str, body: &[&str]) -> SwarmTile {
    let mut t = SwarmTile::new(name, status, accent(status))
        .with_body(body.iter().map(|s| s.to_string()).collect());
    if let Some(r) = role {
        t = t.with_role_glyph(r);
    }
    t
}

fn print_lines(label: &str, lines: &[Line<'static>]) {
    println!("\n=== {label} ===");
    for line in lines {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        println!("{text}");
    }
}

fn main() {
    let header = Line::from(Span::styled(
        "🐝 swarm · 3 agents running",
        Style::default().fg(Color::Rgb(255, 200, 100)),
    ));

    let three = vec![
        mk(
            "researcher",
            Some("★"),
            "thinking",
            &[
                "Searching the codebase for the auth flow...",
                "Found 12 candidate files.",
                "Reading crates/jcode-app-core/src/auth.rs",
                "The OAuth callback is handled in handle_login()",
                "Now cross-referencing the token refresh path.",
            ],
        ),
        mk(
            "implementer",
            None,
            "running",
            &[
                "Editing crates/jcode-base/src/config.rs",
                "Added swarm_spawn_mode = inline",
                "Running cargo check...",
                "warning: unused import `Foo`",
                "Fixing the import.",
            ],
        ),
        mk(
            "reviewer",
            None,
            "done",
            &["Reviewed 4 files.", "No blocking issues found.", "LGTM ✓"],
        ),
    ];

    let cfg = SwarmGalleryConfig::default();
    print_lines(
        "3 agents @ width 100",
        &render_swarm_gallery(&three, 100, &cfg, Some(header.clone())),
    );
    print_lines(
        "3 agents @ width 60",
        &render_swarm_gallery(&three, 60, &cfg, Some(header.clone())),
    );

    let six: Vec<SwarmTile> = (0..6)
        .map(|i| {
            let status = ["running", "thinking", "done", "blocked"][i % 4];
            mk(
                &format!("agent-{i}"),
                None,
                status,
                &[
                    &format!("step {i}.1 doing work"),
                    &format!("step {i}.2 still going"),
                    &format!("step {i}.3 almost there"),
                ],
            )
        })
        .collect();
    print_lines(
        "6 agents @ width 120",
        &render_swarm_gallery(&six, 120, &cfg, Some(header.clone())),
    );

    let many: Vec<SwarmTile> = (0..12)
        .map(|i| {
            mk(
                &format!("worker-{i:02}"),
                None,
                "running",
                &["...", "working"],
            )
        })
        .collect();
    let tight = SwarmGalleryConfig {
        max_height: 12,
        ..Default::default()
    };
    print_lines(
        "12 agents @ width 120, height 12",
        &render_swarm_gallery(&many, 120, &tight, Some(header)),
    );
}
