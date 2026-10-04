//! The `grep(1)`-shaped front end behind the shell shim.
//!
//! The agent's prior is `grep -rn`, so the bash tool points `grep` here: the
//! argv becomes the same query the `kgrep` tool builds, and the search lands in
//! the engine instead of a raw dump. Only the argv subset [`parse`] accepts is
//! translated; every other invocation is handed to the real `grep(1)`
//! untouched, which is what makes shadowing the command safe.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kgrep::lexical;
use kgrep::model::Verb;
use kgrep::packet::render_grep_text;

use super::args::query_from_params;
use super::{
    KgrepInput, budget_from_params, exact_search_file_path, filter_packet_to_exact_file,
    normalized_kgrep_glob_owned,
};
use crate::tool::ToolContext;
use crate::tool::bash::shell_single_quote;

/// The only short flags translated. Anything else, including `-i`, `-c`, `-v`,
/// `-o`, `-w`, `-A/-B/-C` and `-P`, leaves the call to `grep(1)`.
const TRANSLATED_SHORT: &str = "rRnEFGlq";

/// The `BASH_ENV` prelude that points `grep` here, written beside the scratch
/// directory once per process.
///
/// A tool shell sources it, which defines one function and touches nothing else.
/// The front end is total, so an argv it does not model reaches the real
/// `grep(1)` and shadowing the command cannot break a script; a caller that
/// wants the binary says so with the shell's own `command grep`.
///
/// `KCODE_SEARCH_SHIM` overrides the binary the function calls, which is how a
/// test stands in a stub. Unset, it is this process's own executable, so the
/// prelude cannot outlive the binary that wrote it.
pub fn bash_env(dir: &Path) -> Option<PathBuf> {
    static PRELUDE: OnceLock<Option<PathBuf>> = OnceLock::new();
    PRELUDE.get_or_init(|| write_bash_env(dir)).clone()
}

fn write_bash_env(dir: &Path) -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?;
    let body = format!(
        "grep() {{ \"${{KCODE_SEARCH_SHIM:-{}}}\" __search-shim \"$@\"; }}\n",
        shell_single_quote(&binary.to_string_lossy())
    );
    let path = dir.join("search-shim.bash");
    std::fs::write(&path, body).ok()?;
    Some(path)
}

/// Answer one `grep` invocation. The exit code is `grep`'s convention (0 match,
/// 1 none, 2 error) because callers branch on it.
pub fn run(argv: &[String]) -> i32 {
    match parse(argv) {
        Some((params, quiet)) => search(&params, quiet).unwrap_or_else(|| real_grep(argv)),
        None => real_grep(argv),
    }
}

/// The search, or `None` when the engine will not answer it, which sends the
/// whole invocation to `grep(1)` instead.
fn search(params: &KgrepInput, quiet: bool) -> Option<i32> {
    let context = ToolContext::for_cli(std::env::current_dir().ok());
    let query = query_from_params(params, &context).ok()?;
    if !matches!(&query.verb, Verb::Lexical { .. }) {
        return None;
    }
    let packet = lexical::run_grep(&query, budget_from_params(params)).ok()?;
    let packet = filter_packet_to_exact_file(
        packet,
        exact_search_file_path(&context, params.path.as_deref()).as_deref(),
    );
    if !quiet {
        println!("{}", render_grep_text(&packet));
    }
    Some(if packet.total_matches > 0 { 0 } else { 1 })
}

/// Hand the invocation to `grep(1)` unchanged. This is the fallback that makes
/// the shim total: an unmodelled flag, a second root, a stdin pipe or an engine
/// error all land here. `grep` resolves through `PATH`, so this must stay a
/// plain process spawn and never a PATH shim, or it would call itself.
fn real_grep(argv: &[String]) -> i32 {
    match std::process::Command::new("grep").args(argv).status() {
        Ok(status) => status.code().unwrap_or(2),
        Err(_) => 127,
    }
}

/// The query this argv means, or `None` when `grep(1)` should answer it.
fn parse(argv: &[String]) -> Option<(KgrepInput, bool)> {
    let mut glob = None;
    let mut regex = false;
    let mut paths_only = false;
    let mut quiet = false;
    let mut recursive = false;
    let mut pattern: Option<String> = None;
    let mut paths: Vec<&str> = Vec::new();
    let mut options_done = false;
    let mut index = 0;

    while index < argv.len() {
        let arg = argv[index].as_str();
        index += 1;

        if options_done || !arg.starts_with('-') {
            if pattern.is_none() {
                pattern = Some(arg.to_string());
            } else {
                paths.push(arg);
            }
            continue;
        }

        match arg {
            "--" => options_done = true,
            // `-` is stdin, which the engine does not read.
            "-" => return None,
            "--include" => {
                let value = argv.get(index)?;
                index += 1;
                glob = normalized_kgrep_glob_owned(Some(value.as_str()));
            }
            _ if arg.starts_with("--include=") => {
                glob = normalized_kgrep_glob_owned(Some(&arg["--include=".len()..]));
            }
            _ if arg.starts_with("--") => return None,
            _ => {
                for flag in arg[1..].chars() {
                    if !TRANSLATED_SHORT.contains(flag) {
                        return None;
                    }
                    match flag {
                        'r' | 'R' => recursive = true,
                        'n' => {}
                        'E' => regex = true,
                        'F' | 'G' => regex = false,
                        'l' => paths_only = true,
                        'q' => quiet = true,
                        _ => return None,
                    }
                }
            }
        }
    }

    let query = pattern?;
    // A second root has no single engine root, and no root at all without `-r`
    // is `grep`'s own stdin case.
    if paths.len() > 1 || (paths.is_empty() && !recursive) {
        return None;
    }

    Some((
        KgrepInput {
            mode: "grep".to_string(),
            query: Some(query),
            path: paths.first().map(|path| path.to_string()),
            glob,
            regex: Some(regex),
            paths_only: Some(paths_only),
            ..KgrepInput::default()
        },
        quiet,
    ))
}

#[cfg(test)]
#[path = "shim_tests.rs"]
mod tests;
