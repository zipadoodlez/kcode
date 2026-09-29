//! # kcode-render-core
//!
//! Backend-neutral LaTeX math for the TUI markdown renderer.
//!
//! [`math`] turns a TeX math body into terminal-friendly Unicode, inline and
//! display. [`preprocess`] normalizes the delimiter spellings pulldown-cmark
//! does not recognize (`\(...\)`, `\[...\]`, standalone display environments,
//! and fenced `math`/`latex`/`tex`/`katex` blocks) into its `$`/`$$` form so the
//! ordinary markdown parser can see them.
//!
//! Both halves are pure: no ratatui, no document model, no backend palette.

pub mod math;
pub mod preprocess;

pub use math::{render_display_latex, render_inline_latex};
pub use preprocess::{escape_currency_dollars, normalize_latex_math};
