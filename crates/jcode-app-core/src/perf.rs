use std::sync::OnceLock;

pub struct SystemProfile {
    pub load_avg_1m: Option<f64>,
    pub cpu_count: Option<usize>,
    pub available_memory_mb: Option<u64>,
    pub total_memory_mb: Option<u64>,
    pub is_ssh: bool,
    pub is_wsl: bool,
    pub terminal: String,
    /// True when the host terminal is known to corrupt its GPU glyph atlas
    /// under heavy per-cell color/redraw churn (the macOS 26 "garbled glyphs"
    /// bug seen in the VS Code integrated terminal and Apple Terminal, where
    /// letters like n/m/r/w get re-rendered as stale boxes). When set we run a
    /// "glyph-safe" policy that suppresses decorative per-cell color animation
    /// and caps full-frame repaints to keep the atlas stable.
    pub fragile_glyph_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiPerfPolicy {
    pub redraw_fps: u32,
    pub enable_focus_change: bool,
    pub enable_mouse_capture: bool,
    pub enable_keyboard_enhancement: bool,
    pub simplified_model_picker: bool,
    pub linked_side_panel_refresh_interval: std::time::Duration,
}

impl SystemProfile {
    pub fn load_ratio(&self) -> Option<f64> {
        match (self.load_avg_1m, self.cpu_count) {
            (Some(load), Some(cpus)) if cpus > 0 => Some(load / cpus as f64),
            _ => None,
        }
    }

    pub fn memory_pressure(&self) -> Option<f64> {
        match (self.available_memory_mb, self.total_memory_mb) {
            (Some(avail), Some(total)) if total > 0 => Some(1.0 - (avail as f64 / total as f64)),
            _ => None,
        }
    }

    pub fn is_windows_terminal(&self) -> bool {
        self.terminal == "windows-terminal"
    }

    pub fn is_windows_terminal_family(&self) -> bool {
        matches!(
            self.terminal.as_str(),
            "windows-terminal" | "cmd" | "conhost"
        )
    }

    pub fn is_wsl_windows_terminal(&self) -> bool {
        self.is_wsl && self.is_windows_terminal()
    }
}

static PROFILE: OnceLock<SystemProfile> = OnceLock::new();

pub fn profile() -> &'static SystemProfile {
    PROFILE.get_or_init(detect)
}

pub fn tui_policy() -> TuiPerfPolicy {
    tui_policy_for(profile(), &crate::config::config().display)
}

pub fn tui_policy_for(
    profile: &SystemProfile,
    display: &crate::config::DisplayConfig,
) -> TuiPerfPolicy {
    let mut redraw_fps = display.redraw_fps.clamp(1, 120);
    let mut enable_focus_change = true;
    let enable_mouse_capture = display.mouse_capture;
    let mut enable_keyboard_enhancement = true;
    let mut simplified_model_picker = false;
    let mut linked_side_panel_refresh_interval = std::time::Duration::from_millis(250);

    // Glyph-safe mode for terminals with a fragile GPU glyph atlas (macOS 26
    // VS Code integrated terminal / Apple Terminal). The primary fix lives in
    // `jcode-tui-style`: colors are quantized to the 256-palette there, which
    // bounds the distinct (glyph, color) atlas keys (#330). Here we only trim
    // full-frame repaint pressure as cheap insurance.
    if profile.fragile_glyph_cache {
        redraw_fps = redraw_fps.min(30);
    }

    if profile.is_wsl {
        redraw_fps = redraw_fps.min(30);
        linked_side_panel_refresh_interval = std::time::Duration::from_millis(500);
    }

    if profile.is_wsl_windows_terminal() {
        redraw_fps = redraw_fps.min(20);
        enable_focus_change = false;
        enable_keyboard_enhancement = false;
        simplified_model_picker = true;
        linked_side_panel_refresh_interval = std::time::Duration::from_millis(1000);
    }

    TuiPerfPolicy {
        redraw_fps,
        enable_focus_change,
        enable_mouse_capture,
        enable_keyboard_enhancement,
        simplified_model_picker,
        linked_side_panel_refresh_interval,
    }
}

pub fn init_background() {
    std::thread::spawn(|| {
        let p = PROFILE.get_or_init(detect);
        crate::logging::info(&format!(
            "perf: terminal={} ssh={} wsl={} glyph_safe={} load={} cpus={} mem_avail={}MB mem_total={}MB",
            p.terminal,
            p.is_ssh,
            p.is_wsl,
            p.fragile_glyph_cache,
            p.load_avg_1m
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "?".into()),
            p.cpu_count
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            p.available_memory_mb
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            p.total_memory_mb
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
        ));
    });
}

fn detect() -> SystemProfile {
    let is_ssh = std::env::var("SSH_CONNECTION").is_ok() || std::env::var("SSH_TTY").is_ok();
    let is_wsl = detect_wsl();
    let terminal = detect_terminal();
    let (load_avg_1m, cpu_count) = detect_load();
    let (available_memory_mb, total_memory_mb) = detect_memory();

    SystemProfile {
        load_avg_1m,
        cpu_count,
        available_memory_mb,
        total_memory_mb,
        is_ssh,
        is_wsl,
        fragile_glyph_cache: detect_fragile_glyph_cache(&terminal),
        terminal,
    }
}

/// Detect terminals whose GPU glyph atlas corrupts under heavy per-cell
/// color/redraw churn. On macOS 26 (Tahoe) the VS Code integrated terminal
/// (xterm.js) and Apple Terminal exhibit the "garbled glyphs" bug where a
/// fixed set of similar-shaped letters (n/m/r/w/...) get re-rendered as stale
/// cached boxes once the atlas overflows. Anthropic shipped the same class of
/// bug for Claude Code (anthropics/claude-code#60831, #61562) with the
/// `gpuAcceleration: off` workaround; we instead reduce the churn that
/// surfaces it. GPU-robust terminals (Ghostty, iTerm2, kitty, WezTerm,
/// Alacritty) are unaffected and excluded.
fn detect_fragile_glyph_cache(terminal: &str) -> bool {
    // Opt-out / opt-in override for users who want to force the behavior.
    if let Ok(raw) = std::env::var("JCODE_GLYPH_SAFE_MODE") {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => return true,
            "0" | "false" | "no" | "off" => return false,
            _ => {}
        }
    }

    // Only macOS surfaces this; other platforms render these terminals fine.
    if !cfg!(target_os = "macos") {
        return false;
    }

    matches!(terminal, "vscode" | "apple_terminal")
}

fn detect_wsl() -> bool {
    if std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSLENV").is_ok() {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(v) = std::fs::read_to_string("/proc/version") {
            let lower = v.to_ascii_lowercase();
            if lower.contains("microsoft") || lower.contains("wsl") {
                return true;
            }
        }
    }
    false
}

fn detect_terminal() -> String {
    if std::env::var("WT_SESSION").is_ok() {
        return "windows-terminal".to_string();
    }
    if std::env::var("WEZTERM_EXECUTABLE").is_ok() || std::env::var("WEZTERM_PANE").is_ok() {
        return "wezterm".to_string();
    }
    if std::env::var("KITTY_PID").is_ok() {
        return "kitty".to_string();
    }
    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
        return "ghostty".to_string();
    }
    if std::env::var("ALACRITTY_WINDOW_ID").is_ok() {
        return "alacritty".to_string();
    }
    if let Ok(tp) = std::env::var("TERM_PROGRAM") {
        return tp.to_lowercase();
    }
    "unknown".to_string()
}

#[cfg(target_os = "linux")]
fn detect_load() -> (Option<f64>, Option<usize>) {
    let load = std::fs::read_to_string("/proc/loadavg").ok().and_then(|s| {
        s.split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
    });

    let cpus = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .map(|s| s.matches("processor\t:").count())
        .filter(|&c| c > 0)
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()));

    (load, cpus)
}

#[cfg(target_os = "macos")]
fn detect_load() -> (Option<f64>, Option<usize>) {
    let load = {
        let mut loadavg: [libc::c_double; 3] = [0.0; 3];
        let n = unsafe { libc::getloadavg(loadavg.as_mut_ptr(), 1) };
        if n >= 1 { Some(loadavg[0]) } else { None }
    };
    let cpus = std::thread::available_parallelism().ok().map(|n| n.get());
    (load, cpus)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn detect_load() -> (Option<f64>, Option<usize>) {
    let cpus = std::thread::available_parallelism().ok().map(|n| n.get());
    (None, cpus)
}

#[cfg(target_os = "linux")]
fn detect_memory() -> (Option<u64>, Option<u64>) {
    let contents = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;

    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = parse_meminfo_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_meminfo_kb(rest);
        }
        if total_kb.is_some() && available_kb.is_some() {
            break;
        }
    }

    (available_kb.map(|k| k / 1024), total_kb.map(|k| k / 1024))
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kb(s: &str) -> Option<u64> {
    s.split_whitespace().next()?.parse().ok()
}

#[cfg(target_os = "macos")]
fn detect_memory() -> (Option<u64>, Option<u64>) {
    let total = {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let name = c"hw.memsize";
        let ret = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut size as *mut u64 as *mut libc::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret == 0 {
            Some(size / (1024 * 1024))
        } else {
            None
        }
    };

    // macOS doesn't have a simple "available" metric like Linux's MemAvailable.
    // vm_stat gives pages free + inactive but parsing it adds complexity.
    // For tier detection, total memory is sufficient on macOS.
    (None, total)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn detect_memory() -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_for(terminal: &str, is_wsl: bool, fragile_glyph_cache: bool) -> SystemProfile {
        SystemProfile {
            load_avg_1m: Some(0.2),
            cpu_count: Some(8),
            available_memory_mb: Some(8192),
            total_memory_mb: Some(16384),
            is_ssh: false,
            is_wsl,
            terminal: terminal.to_string(),
            fragile_glyph_cache,
        }
    }

    #[test]
    fn test_tui_policy_keeps_native_defaults() {
        let profile = profile_for("kitty", false, false);
        let mut display = crate::config::DisplayConfig::default();
        display.mouse_capture = true;
        display.redraw_fps = 48;
        let policy = tui_policy_for(&profile, &display);
        assert_eq!(policy.redraw_fps, 48);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
        assert!(!policy.simplified_model_picker);
        assert!(policy.enable_mouse_capture);
        assert_eq!(
            policy.linked_side_panel_refresh_interval,
            std::time::Duration::from_millis(250)
        );
    }

    #[test]
    fn test_tui_policy_caps_wsl_windows_terminal() {
        let profile = profile_for("windows-terminal", true, false);
        let mut display = crate::config::DisplayConfig::default();
        display.mouse_capture = true;
        display.redraw_fps = 60;
        let policy = tui_policy_for(&profile, &display);
        assert_eq!(policy.redraw_fps, 20);
        assert!(!policy.enable_focus_change);
        assert!(!policy.enable_keyboard_enhancement);
        assert!(policy.simplified_model_picker);
        assert!(policy.enable_mouse_capture);
        assert_eq!(
            policy.linked_side_panel_refresh_interval,
            std::time::Duration::from_millis(1000)
        );
    }

    #[test]
    fn test_tui_policy_caps_generic_wsl_without_disabling_terminal_features() {
        let profile = profile_for("wezterm", true, false);
        let mut display = crate::config::DisplayConfig::default();
        display.mouse_capture = false;
        display.redraw_fps = 60;
        let policy = tui_policy_for(&profile, &display);
        assert_eq!(policy.redraw_fps, 30);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
        assert!(!policy.simplified_model_picker);
        assert!(!policy.enable_mouse_capture);
        assert_eq!(
            policy.linked_side_panel_refresh_interval,
            std::time::Duration::from_millis(500)
        );
    }

    #[test]
    fn test_glyph_safe_mode_caps_redraw_without_disabling_terminal_features() {
        // VS Code integrated terminal / Apple Terminal on macOS 26 corrupt the
        // GPU glyph atlas under truecolor color churn (#330). The root-cause fix
        // is color quantization in jcode-tui-style; the policy only trims
        // full-frame repaint pressure.
        let profile = profile_for("vscode", false, true);
        let mut display = crate::config::DisplayConfig::default();
        display.redraw_fps = 60;
        let policy = tui_policy_for(&profile, &display);
        assert_eq!(policy.redraw_fps, 30);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
    }

    #[test]
    fn test_non_fragile_terminal_keeps_full_redraw_rate() {
        let profile = profile_for("ghostty", false, false);
        let mut display = crate::config::DisplayConfig::default();
        display.redraw_fps = 60;
        assert_eq!(tui_policy_for(&profile, &display).redraw_fps, 60);
    }

    #[test]
    fn test_detect_runs() {
        let p = detect();
        assert!(!p.terminal.is_empty());
    }

    #[test]
    fn test_profile_accessors() {
        let p = profile_for("kitty", false, false);
        assert!((p.load_ratio().unwrap() - 0.025).abs() < 0.001);
        assert!((p.memory_pressure().unwrap() - 0.5).abs() < 0.01);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_detect_fragile_glyph_cache_targets_macos_terminals() {
        // Env override must not leak between cases.
        let prev = std::env::var("JCODE_GLYPH_SAFE_MODE").ok();
        unsafe {
            std::env::remove_var("JCODE_GLYPH_SAFE_MODE");
        }
        assert!(detect_fragile_glyph_cache("vscode"));
        assert!(detect_fragile_glyph_cache("apple_terminal"));
        assert!(!detect_fragile_glyph_cache("ghostty"));
        assert!(!detect_fragile_glyph_cache("iterm.app"));
        assert!(!detect_fragile_glyph_cache("kitty"));
        if let Some(prev) = prev {
            unsafe {
                std::env::set_var("JCODE_GLYPH_SAFE_MODE", prev);
            }
        }
    }
}
