pub mod color;
pub mod theme;
pub mod theme_mode;

pub use color::{ColorCapability, clear_buf, color_capability, has_truecolor, indexed_to_rgb, rgb};
pub use theme_mode::{
    ThemeMode, adapt_buffer, adapt_buffer_for_theme, adapt_color_for_theme, is_light_theme,
    set_theme_mode, theme_mode,
};
