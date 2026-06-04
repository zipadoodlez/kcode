use crate::tui::{TuiState, color_support::rgb};
use ratatui::{prelude::*, widgets::Paragraph};
use std::cell::RefCell;
use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

const IDLE_VARIANTS: &[&str] = &["donut", "orbit_rings"];

// Pure math kernels (3D samplers, glyph chooser, HSV->RGB) live in the
// dependency-free `jcode-tui-anim` crate, which is pinned to opt-level = 3 in
// all profiles so these trig-heavy loops stay optimized even in debug/selfdev
// builds. They are imported under their original names so the call sites and
// tests below are unchanged.
use jcode_tui_anim::{
    hsv_to_rgb, sample_black_hole, sample_donut, sample_gyroscope, sample_orbit_rings,
    shape_char_3x3,
};

fn animation_seed() -> u64 {
    static SEED: OnceLock<u64> = OnceLock::new();
    *SEED.get_or_init(|| {
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        hasher.finish()
    })
}

fn normalized_animation_name(name: &str) -> String {
    name.trim().to_lowercase().replace(['-', ' '], "_")
}

fn expand_disabled_animation_names<I>(names: I) -> HashSet<String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut disabled: HashSet<String> = names
        .into_iter()
        .map(|name| normalized_animation_name(name.as_ref()))
        .collect();

    if disabled.contains("three_rings") || disabled.contains("three-rings") {
        disabled.insert("three_rings".to_string());
        disabled.insert("gyroscope".to_string());
    }
    if disabled.contains("gyroscope") {
        disabled.insert("three_rings".to_string());
    }

    disabled
}

fn disabled_animation_names() -> HashSet<String> {
    expand_disabled_animation_names(crate::config::config().display.disabled_animations.iter())
}

fn choose_animation_variant_from_disabled<'a>(
    variants: &'a [&'a str],
    salt: u64,
    disabled: &HashSet<String>,
) -> &'a str {
    let available: Vec<&str> = variants
        .iter()
        .copied()
        .filter(|name| !disabled.contains(&normalized_animation_name(name)))
        .collect();

    let pool = if available.is_empty() {
        variants
    } else {
        &available
    };
    let idx = ((animation_seed() ^ salt) as usize) % pool.len();
    pool[idx]
}

fn choose_animation_variant<'a>(variants: &'a [&'a str], salt: u64) -> &'a str {
    let disabled = disabled_animation_names();
    choose_animation_variant_from_disabled(variants, salt, &disabled)
}

struct IdleBuffers {
    hit: Vec<bool>,
    lum_map: Vec<f32>,
    z_buf: Vec<f32>,
    size: usize,
}

impl IdleBuffers {
    fn new() -> Self {
        Self {
            hit: Vec::new(),
            lum_map: Vec::new(),
            z_buf: Vec::new(),
            size: 0,
        }
    }

    fn resize_and_clear(&mut self, len: usize) {
        if self.size != len {
            self.hit.resize(len, false);
            self.lum_map.resize(len, 0.0);
            self.z_buf.resize(len, 0.0);
            self.size = len;
        }
        self.hit.fill(false);
        self.lum_map.fill(0.0);
        self.z_buf.fill(0.0);
    }
}

thread_local! {
    static IDLE_BUF: RefCell<IdleBuffers> = RefCell::new(IdleBuffers::new());
}

pub(super) fn draw_idle_animation(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    if area.width < 4 || area.height < 2 {
        return;
    }

    let elapsed = app.animation_elapsed();
    let cw = area.width as usize;
    let ch = area.height as usize;

    const SUB_X: usize = 3;
    const SUB_Y: usize = 3;
    let sw = cw * SUB_X;
    let sh = ch * SUB_Y;

    IDLE_BUF.with(|cell| {
        let mut bufs = cell.borrow_mut();
        bufs.resize_and_clear(sw * sh);
        let bufs = &mut *bufs;

        let variant = idle_animation_variant();
        match variant {
            "donut" => sample_donut(
                elapsed,
                sw,
                sh,
                &mut bufs.hit,
                &mut bufs.lum_map,
                &mut bufs.z_buf,
            ),
            "orbit_rings" => sample_orbit_rings(
                elapsed,
                sw,
                sh,
                &mut bufs.hit,
                &mut bufs.lum_map,
                &mut bufs.z_buf,
            ),
            "black_hole" => sample_black_hole(
                elapsed,
                sw,
                sh,
                &mut bufs.hit,
                &mut bufs.lum_map,
                &mut bufs.z_buf,
            ),
            _ => sample_gyroscope(
                elapsed,
                sw,
                sh,
                &mut bufs.hit,
                &mut bufs.lum_map,
                &mut bufs.z_buf,
            ),
        }

        let hit = &bufs.hit;
        let lum_map = &bufs.lum_map;

        let time_hue = elapsed * 40.0;
        let centered = app.centered_mode();
        let align = if centered {
            Alignment::Center
        } else {
            Alignment::Left
        };

        let lines: Vec<Line<'static>> = (0..ch)
            .map(|row| {
                let spans: Vec<Span<'static>> = (0..cw)
                    .map(|col| {
                        let mut pattern = 0u16;
                        let mut total_lum = 0.0f32;
                        let mut hit_count = 0u32;

                        for sy in 0..SUB_Y {
                            for sx in 0..SUB_X {
                                let px = col * SUB_X + sx;
                                let py = row * SUB_Y + sy;
                                let idx = py * sw + px;
                                if hit[idx] {
                                    pattern |= 1 << (sy * SUB_X + sx);
                                    total_lum += lum_map[idx];
                                    hit_count += 1;
                                }
                            }
                        }

                        if hit_count == 0 {
                            Span::raw(" ")
                        } else {
                            let avg_lum = total_lum / hit_count as f32;
                            let coverage = hit_count as f32 / (SUB_X * SUB_Y) as f32;
                            let t = (avg_lum + 1.0) * 0.5;
                            let ch = shape_char_3x3(pattern, t);

                            let hue = (time_hue + t * 160.0) % 360.0;
                            let hue = if hue < 0.0 { hue + 360.0 } else { hue };

                            let sat = 0.5 + t * 0.4;
                            let val = (0.10 + t * t * 0.90) * (0.55 + coverage * 0.45);
                            let (r, g, b) = hsv_to_rgb(hue, sat, val);
                            Span::styled(String::from(ch), Style::default().fg(rgb(r, g, b)))
                        }
                    })
                    .collect();
                Line::from(spans).alignment(align)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), area);
    });
}

fn idle_animation_variant() -> &'static str {
    choose_animation_variant(IDLE_VARIANTS, 0x4944_4c45_414e_494d)
}

#[cfg(test)]
mod tests {
    use super::*;

    type IdleSampler = fn(f32, usize, usize, &mut [bool], &mut [f32], &mut [f32]);

    fn hit_bounds(hit: &[bool], sw: usize, sh: usize) -> Option<(usize, usize, usize, usize)> {
        let mut min_x = sw;
        let mut max_x = 0usize;
        let mut min_y = sh;
        let mut max_y = 0usize;
        let mut any = false;

        for y in 0..sh {
            for x in 0..sw {
                if hit[y * sw + x] {
                    any = true;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        any.then_some((min_x, max_x, min_y, max_y))
    }

    fn assert_idle_sampler_avoids_heavy_border_clipping(name: &str, sampler: IdleSampler) {
        let sw = 120;
        let sh = 60;

        for &elapsed in &[0.0f32, 0.8, 1.6, 2.4] {
            let mut hit = vec![false; sw * sh];
            let mut lum_map = vec![0.0; sw * sh];
            let mut z_buf = vec![0.0; sw * sh];
            sampler(elapsed, sw, sh, &mut hit, &mut lum_map, &mut z_buf);

            let (_min_x, _max_x, _min_y, _max_y) =
                hit_bounds(&hit, sw, sh).unwrap_or_else(|| panic!("{name} should draw pixels"));
            let lit_pixels = hit.iter().filter(|&&value| value).count();
            let border_pixels = hit
                .iter()
                .enumerate()
                .filter(|(idx, value)| {
                    if !**value {
                        return false;
                    }
                    let x = idx % sw;
                    let y = idx / sw;
                    x == 0 || x + 1 == sw || y == 0 || y + 1 == sh
                })
                .count();

            assert!(
                border_pixels * 12 < lit_pixels.max(1),
                "{name} at t={elapsed} is too clipped at viewport border: border_pixels={border_pixels}, lit_pixels={lit_pixels}"
            );
        }
    }

    fn assert_idle_sampler_stays_off_border_on_small_viewports(name: &str, sampler: IdleSampler) {
        let sizes = [(90usize, 36usize), (108, 42), (120, 48)];

        for &(sw, sh) in &sizes {
            for &elapsed in &[0.0f32, 0.8, 1.6, 2.4] {
                let mut hit = vec![false; sw * sh];
                let mut lum_map = vec![0.0; sw * sh];
                let mut z_buf = vec![0.0; sw * sh];
                sampler(elapsed, sw, sh, &mut hit, &mut lum_map, &mut z_buf);

                let (min_x, max_x, min_y, max_y) =
                    hit_bounds(&hit, sw, sh).unwrap_or_else(|| panic!("{name} should draw pixels"));

                assert!(
                    min_x > 0 && max_x + 1 < sw && min_y > 0 && max_y + 1 < sh,
                    "{name} at t={elapsed} touches border on small viewport {sw}x{sh}: bounds=({min_x}..={max_x}, {min_y}..={max_y})"
                );
            }
        }
    }

    #[test]
    fn idle_variants_exclude_retired_variants() {
        assert!(!IDLE_VARIANTS.contains(&"knot"));
        assert!(!IDLE_VARIANTS.contains(&"black_hole"));
    }

    #[test]
    fn idle_variants_keep_normal_donut_and_exclude_cube() {
        assert!(IDLE_VARIANTS.contains(&"donut"));
        assert!(!IDLE_VARIANTS.contains(&"pulse_donut"));
        assert!(IDLE_VARIANTS.contains(&"orbit_rings"));
        assert!(!IDLE_VARIANTS.contains(&"three_rings"));
        assert!(!IDLE_VARIANTS.contains(&"cube"));
    }

    #[test]
    fn disabling_three_rings_also_disables_gyroscope_alias() {
        let disabled = expand_disabled_animation_names(["three_rings"]);
        assert!(disabled.contains("three_rings"));
        assert!(disabled.contains("gyroscope"));
    }

    #[test]
    fn variant_selection_avoids_disabled_entries_when_possible() {
        let disabled = expand_disabled_animation_names(["donut", "three_rings"]);
        let variant = choose_animation_variant_from_disabled(IDLE_VARIANTS, 7, &disabled);
        assert_ne!(variant, "donut");
        assert_ne!(variant, "three_rings");
    }

    #[test]
    fn idle_animation_samplers_avoid_heavy_border_clipping() {
        assert_idle_sampler_avoids_heavy_border_clipping("donut", sample_donut);
        assert_idle_sampler_avoids_heavy_border_clipping("three_rings", sample_gyroscope);
        assert_idle_sampler_avoids_heavy_border_clipping("orbit_rings", sample_orbit_rings);
    }

    #[test]
    fn three_rings_fit_small_viewports_without_touching_border() {
        assert_idle_sampler_stays_off_border_on_small_viewports("three_rings", sample_gyroscope);
    }
}
