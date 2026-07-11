//! Input preprocessing applied before markdown parsing.
//!
//! pulldown-cmark's math extension is aggressive: any `$`-delimited run can be
//! treated as inline math, so plain currency like `$5` or `$5x$` gets parsed as
//! math. The TUI renderer guards against this by escaping `$`-then-digit into
//! `\$` before parsing. We mirror that behavior here so the shared core matches
//! the authoritative renderer.

const DISPLAY_MATH_ENVIRONMENTS: &[&str] = &[
    "equation",
    "equation*",
    "displaymath",
    "align",
    "align*",
    "aligned",
    "aligned*",
    "gather",
    "gather*",
    "gathered",
    "multline",
    "multline*",
    "split",
    "eqnarray",
    "eqnarray*",
    "matrix",
    "smallmatrix",
    "array",
    "pmatrix",
    "bmatrix",
    "Bmatrix",
    "vmatrix",
    "Vmatrix",
    "cases",
    "cases*",
];

/// Normalize common LaTeX math containers into pulldown-cmark's `$` / `$$`
/// syntax before markdown parsing.
///
/// This recognizes `\(...\)`, `\[...\]`, standalone display-math
/// environments, and fenced `math`/`latex`/`tex`/`katex` blocks. Generic code
/// spans and code fences are intentionally left byte-for-byte unchanged.
pub fn normalize_latex_math(text: &str) -> String {
    let fenced = normalize_math_fences(text);
    normalize_latex_delimiters_and_environments(&fenced)
}

fn normalize_math_fences(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < lines.len() {
        let Some((indent, marker, marker_len, info)) = opening_fence(lines[index]) else {
            out.push_str(lines[index]);
            if index + 1 < lines.len() {
                out.push('\n');
            }
            index += 1;
            continue;
        };

        let Some(close_offset) = lines[index + 1..]
            .iter()
            .position(|line| closing_fence(line, marker, marker_len))
        else {
            out.push_str(lines[index]);
            if index + 1 < lines.len() {
                out.push('\n');
            }
            index += 1;
            continue;
        };
        let close_index = index + 1 + close_offset;

        if is_math_fence_info(info) {
            let body = lines[index + 1..close_index].join("\n");
            let body = strip_outer_display_delimiters(&body);
            out.push_str(indent);
            out.push_str("$$\n");
            out.push_str(body.trim_matches('\n'));
            out.push('\n');
            out.push_str(indent);
            out.push_str("$$");
            if close_index + 1 < lines.len() {
                out.push('\n');
            }
        } else {
            for (offset, line) in lines[index..=close_index].iter().enumerate() {
                out.push_str(line);
                let is_closing_line = index + offset == close_index;
                if close_index + 1 < lines.len() || !is_closing_line {
                    out.push('\n');
                }
            }
        }
        index = close_index + 1;
    }

    out
}

fn strip_outer_display_delimiters(body: &str) -> &str {
    let trimmed = body.trim();
    if let Some(inner) = trimmed
        .strip_prefix("\\[")
        .and_then(|value| value.strip_suffix("\\]"))
    {
        return inner;
    }
    if let Some(inner) = trimmed
        .strip_prefix("$$")
        .and_then(|value| value.strip_suffix("$$"))
    {
        return inner;
    }
    body
}

fn opening_fence(line: &str) -> Option<(&str, char, usize, &str)> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = &line[..indent_len];
    if indent
        .chars()
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum::<usize>()
        > 3
    {
        return None;
    }
    let rest = &line[indent_len..];
    let marker = rest.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let marker_len = rest.chars().take_while(|ch| *ch == marker).count();
    if marker_len < 3 {
        return None;
    }
    let marker_bytes = marker.len_utf8() * marker_len;
    Some((indent, marker, marker_len, rest[marker_bytes..].trim()))
}

fn closing_fence(line: &str, marker: char, minimum_len: usize) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let run = trimmed.chars().take_while(|ch| *ch == marker).count();
    run >= minimum_len && trimmed[run * marker.len_utf8()..].trim().is_empty()
}

fn is_math_fence_info(info: &str) -> bool {
    let language = info
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['{', '}', '.'])
        .to_ascii_lowercase();
    matches!(language.as_str(), "math" | "latex" | "tex" | "katex")
}

fn normalize_latex_delimiters_and_environments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    let mut inline_ticks = 0usize;
    let mut fence: Option<(char, usize)> = None;
    let mut line_start = true;
    let mut leading_columns = 0usize;
    let mut math_dollars = 0usize;
    let mut wrapped_environment: Option<(String, usize)> = None;
    let mut literal_environment: Option<(String, usize)> = None;

    while index < text.len() {
        let rest = &text[index..];
        let ch = rest.chars().next().expect("index stays on a char boundary");

        if ch == '\n' {
            out.push(ch);
            index += 1;
            line_start = true;
            leading_columns = 0;
            continue;
        }

        if line_start && matches!(ch, ' ' | '\t') {
            leading_columns += if ch == '\t' { 4 } else { 1 };
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if inline_ticks == 0 && line_start && leading_columns <= 3 && matches!(ch, '`' | '~') {
            let run = rest
                .chars()
                .take_while(|candidate| *candidate == ch)
                .count();
            if run >= 3 {
                let is_close = fence.is_some_and(|(marker, len)| marker == ch && run >= len);
                if fence.is_none() || is_close {
                    fence = if is_close { None } else { Some((ch, run)) };
                }
                out.push_str(&rest[..run * ch.len_utf8()]);
                index += run * ch.len_utf8();
                line_start = false;
                continue;
            }
        }
        line_start = false;

        if fence.is_some() {
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if ch == '`' {
            let run = rest
                .chars()
                .take_while(|candidate| *candidate == '`')
                .count();
            if inline_ticks == 0 {
                inline_ticks = run;
            } else if inline_ticks == run {
                inline_ticks = 0;
            }
            out.push_str(&rest[..run]);
            index += run;
            continue;
        }
        if inline_ticks > 0 {
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if let Some((name, depth)) = literal_environment.as_mut() {
            if let Some((begin_name, marker_len)) = environment_marker(rest, "begin")
                && begin_name == name
            {
                *depth += 1;
                out.push_str(&rest[..marker_len]);
                index += marker_len;
                continue;
            }
            if let Some((end_name, marker_len)) = environment_marker(rest, "end")
                && end_name == name
            {
                *depth -= 1;
                out.push_str(&rest[..marker_len]);
                index += marker_len;
                if *depth == 0 {
                    literal_environment = None;
                }
                continue;
            }
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if ch == '$' && !is_escaped_at(text, index) {
            let run = rest
                .chars()
                .take_while(|candidate| *candidate == '$')
                .count()
                .min(2);
            let looks_like_currency = run == 1
                && rest[1..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_digit());
            if !looks_like_currency {
                math_dollars = if math_dollars == run {
                    0
                } else if math_dollars == 0 {
                    run
                } else {
                    math_dollars
                };
            }
            out.push_str(&rest[..run]);
            index += run;
            continue;
        }

        if math_dollars == 0
            && rest.starts_with("\\(")
            && !is_escaped_at(text, index)
            && has_unescaped_delimiter(&rest[2..], "\\)")
        {
            out.push('$');
            math_dollars = 1;
            index += 2;
            continue;
        }
        if math_dollars == 1 && rest.starts_with("\\)") && !is_escaped_at(text, index) {
            out.push('$');
            math_dollars = 0;
            index += 2;
            continue;
        }
        if math_dollars == 0
            && rest.starts_with("\\[")
            && !is_escaped_at(text, index)
            && has_unescaped_delimiter(&rest[2..], "\\]")
            && !looks_like_escaped_markdown_link(rest)
        {
            out.push_str("$$");
            math_dollars = 2;
            index += 2;
            continue;
        }
        if math_dollars == 2 && rest.starts_with("\\]") && !is_escaped_at(text, index) {
            out.push_str("$$");
            math_dollars = 0;
            index += 2;
            continue;
        }

        if ch == '\\'
            && math_dollars == 0
            && wrapped_environment.is_none()
            && let Some((name, marker_len)) = environment_marker(rest, "begin")
            && DISPLAY_MATH_ENVIRONMENTS.contains(&name)
        {
            if has_matching_environment_end(rest, name) {
                out.push_str("$$\n");
                out.push_str(&rest[..marker_len]);
                wrapped_environment = Some((name.to_string(), 1));
            } else {
                out.push_str(&rest[..marker_len]);
                literal_environment = Some((name.to_string(), 1));
            }
            index += marker_len;
            continue;
        }

        if let Some((name, depth)) = wrapped_environment.as_mut() {
            if let Some((begin_name, marker_len)) = environment_marker(rest, "begin")
                && begin_name == name
            {
                *depth += 1;
                out.push_str(&rest[..marker_len]);
                index += marker_len;
                continue;
            }
            if let Some((end_name, marker_len)) = environment_marker(rest, "end")
                && end_name == name
            {
                *depth -= 1;
                out.push_str(&rest[..marker_len]);
                index += marker_len;
                if *depth == 0 {
                    out.push_str("\n$$");
                    wrapped_environment = None;
                }
                continue;
            }
        }

        out.push(ch);
        index += ch.len_utf8();
    }

    out
}

fn environment_marker<'a>(rest: &'a str, command: &str) -> Option<(&'a str, usize)> {
    let prefix = format!("\\{command}{{");
    let after_prefix = rest.strip_prefix(&prefix)?;
    let close = after_prefix.find('}')?;
    let name = &after_prefix[..close];
    Some((name, prefix.len() + close + 1))
}

fn has_matching_environment_end(source: &str, name: &str) -> bool {
    let begin_marker = format!("\\begin{{{name}}}");
    let end_marker = format!("\\end{{{name}}}");
    let mut search = begin_marker.len();
    let mut depth = 1usize;
    while search < source.len() {
        let next_begin = source[search..]
            .find(&begin_marker)
            .map(|offset| search + offset);
        let next_end = source[search..]
            .find(&end_marker)
            .map(|offset| search + offset);
        match (next_begin, next_end) {
            (_, None) => return false,
            (Some(begin), Some(end)) if begin < end => {
                depth += 1;
                search = begin + begin_marker.len();
            }
            (_, Some(end)) => {
                depth -= 1;
                if depth == 0 {
                    return true;
                }
                search = end + end_marker.len();
            }
        }
    }
    false
}

fn is_escaped_at(text: &str, index: usize) -> bool {
    let preceding = text[..index]
        .chars()
        .rev()
        .take_while(|ch| *ch == '\\')
        .count();
    preceding % 2 == 1
}

fn has_unescaped_delimiter(text: &str, delimiter: &str) -> bool {
    find_unescaped_delimiter(text, delimiter).is_some()
}

fn find_unescaped_delimiter(text: &str, delimiter: &str) -> Option<usize> {
    let mut search_from = 0usize;
    while let Some(offset) = text[search_from..].find(delimiter) {
        let index = search_from + offset;
        if !is_escaped_at(text, index) {
            return Some(index);
        }
        search_from = index + delimiter.len();
    }
    None
}

fn looks_like_escaped_markdown_link(rest: &str) -> bool {
    let Some(close) = find_unescaped_delimiter(&rest[2..], "\\]") else {
        return false;
    };
    rest[2 + close + 2..].starts_with('(')
}

/// Escape dollar signs that look like currency amounts (`$` immediately
/// followed by an ASCII digit) into `\$`, so the math extension does not treat
/// them as inline math. Dollars inside inline code spans and fenced code blocks
/// are left untouched, and already-escaped `\$` is preserved. Display-math
/// `$$` runs are passed through unchanged.
pub fn escape_currency_dollars(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut code_fence: Option<(char, usize)> = None;
    let mut inline_code_len: usize = 0;
    let mut at_line_start = true;
    let mut leading_spaces = 0;

    let count_marker = |chars: &[char], start: usize, marker: char| {
        let mut j = start;
        while j < chars.len() && chars[j] == marker {
            j += 1;
        }
        j - start
    };

    let is_escaped = |chars: &[char], pos: usize| {
        let mut backslashes = 0usize;
        let mut j = pos;
        while j > 0 {
            if chars[j - 1] != '\\' {
                break;
            }
            backslashes += 1;
            j -= 1;
        }
        backslashes % 2 == 1
    };

    while i < len {
        let c = chars[i];

        if c == '\n' {
            at_line_start = true;
            leading_spaces = 0;
            out.push('\n');
            i += 1;
            continue;
        }

        if at_line_start && (c == ' ' || c == '\t') {
            leading_spaces += if c == '\t' { 4 } else { 1 };
            out.push(c);
            i += 1;
            continue;
        }

        let marker_run = if matches!(c, '`' | '~') {
            count_marker(&chars, i, c)
        } else {
            0
        };
        let maybe_fence = inline_code_len == 0 && marker_run >= 3;
        if maybe_fence && at_line_start && leading_spaces <= 3 {
            let run = marker_run;
            let line_end = chars[i + run..]
                .iter()
                .position(|ch| *ch == '\n')
                .map(|offset| i + run + offset)
                .unwrap_or(len);
            let trailing_is_blank = chars[i + run..line_end].iter().all(|ch| ch.is_whitespace());
            let is_close = code_fence.is_some_and(|(marker, minimum)| {
                marker == c && run >= minimum && trailing_is_blank
            });
            if code_fence.is_none() || is_close {
                code_fence = if is_close { None } else { Some((c, run)) };
            }
            for _ in 0..run {
                out.push(c);
            }
            i += run;
            at_line_start = false;
            leading_spaces = 0;
            continue;
        }

        if code_fence.is_some() {
            out.push(c);
            i += 1;
            at_line_start = false;
            leading_spaces = 0;
            continue;
        }

        if c == '`' {
            let run = count_marker(&chars, i, '`');
            if inline_code_len > 0 {
                if run == inline_code_len {
                    inline_code_len = 0;
                }
                for _ in 0..run {
                    out.push('`');
                }
                i += run;
                at_line_start = false;
                leading_spaces = 0;
                continue;
            }

            inline_code_len = run;
            for _ in 0..run {
                out.push('`');
            }
            i += run;
            at_line_start = false;
            leading_spaces = 0;
            continue;
        }

        if at_line_start {
            at_line_start = false;
        }

        if c == ' ' || c == '\t' {
            out.push(c);
            i += 1;
            continue;
        }

        if inline_code_len > 0 {
            out.push(c);
            i += 1;
            continue;
        }

        if c == '$' && i + 1 < len && chars[i + 1] == '$' {
            out.push_str("$$");
            i += 2;
            continue;
        }

        if c == '$' && i + 1 < len && chars[i + 1].is_ascii_digit() {
            if is_escaped(&chars, i) {
                out.push('$');
            } else {
                out.push_str("\\$");
            }
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{escape_currency_dollars, normalize_latex_math};

    #[test]
    fn escapes_currency() {
        assert_eq!(escape_currency_dollars("$5"), "\\$5");
        assert_eq!(escape_currency_dollars("$5x$"), "\\$5x$");
    }

    #[test]
    fn leaves_real_math() {
        assert_eq!(escape_currency_dollars("$a+b$"), "$a+b$");
    }

    #[test]
    fn passes_through_display_math() {
        assert_eq!(escape_currency_dollars("$$x=5$$"), "$$x=5$$");
    }

    #[test]
    fn skips_inline_code() {
        assert_eq!(escape_currency_dollars("`$5`"), "`$5`");
    }

    #[test]
    fn skips_fenced_code() {
        let input = "```\n$5\n```";
        assert_eq!(escape_currency_dollars(input), input);
        let tilde = "~~~text\n$5\n~~~";
        assert_eq!(escape_currency_dollars(tilde), tilde);
        let longer = "````text\n``` is content and $5\n````";
        assert_eq!(escape_currency_dollars(longer), longer);
    }

    #[test]
    fn preserves_already_escaped() {
        assert_eq!(escape_currency_dollars("\\$5"), "\\$5");
    }

    #[test]
    fn normalizes_parenthesized_and_bracketed_math() {
        assert_eq!(
            normalize_latex_math(r"Inline \(x^2 + \alpha\) done."),
            r"Inline $x^2 + \alpha$ done."
        );
        assert_eq!(
            normalize_latex_math("\\[\n\\frac{x+1}{y}\n\\]"),
            "$$\n\\frac{x+1}{y}\n$$"
        );
    }

    #[test]
    fn normalizes_standalone_display_environments() {
        let input = "before\n\\begin{align*}\nx &= 1 \\\\\ny &= 2\n\\end{align*}\nafter";
        let normalized = normalize_latex_math(input);
        assert!(normalized.contains("$$\n\\begin{align*}"), "{normalized}");
        assert!(normalized.contains("\\end{align*}\n$$"), "{normalized}");

        let nested = r"\begin{matrix}\begin{matrix}a\end{matrix}\end{matrix}";
        assert_eq!(normalize_latex_math(nested), format!("$$\n{nested}\n$$"));

        let unmatched = r"\begin{matrix}\begin{matrix}a\end{matrix}";
        assert_eq!(normalize_latex_math(unmatched), unmatched);
    }

    #[test]
    fn normalizes_explicit_math_fences() {
        assert_eq!(
            normalize_latex_math("```math\n\\frac{a}{b}\n```"),
            "$$\n\\frac{a}{b}\n$$"
        );
        assert_eq!(
            normalize_latex_math("~~~latex\n\\[\nx^2\n\\]\n~~~"),
            "$$\nx^2\n$$"
        );
    }

    #[test]
    fn preserves_literal_code_and_malformed_delimiters() {
        let inline = r"Use `\(x^2\)` literally";
        let fenced = "```rust\nlet latex = r\"\\[x\\]\";\n```";
        let plain_fenced = "```\nlet x = 1;\n```";
        let escaped_link = r"a \[link\](https://example.com) reference";
        let malformed = r"unfinished \[x^2";
        assert_eq!(normalize_latex_math(inline), inline);
        assert_eq!(normalize_latex_math(fenced), fenced);
        assert_eq!(normalize_latex_math(plain_fenced), plain_fenced);
        assert_eq!(normalize_latex_math(escaped_link), escaped_link);
        assert_eq!(normalize_latex_math(malformed), malformed);
    }

    #[test]
    fn currency_does_not_hide_later_latex_math() {
        assert_eq!(
            normalize_latex_math(r"Costs $35. Then \[x^2\]."),
            r"Costs $35. Then $$x^2$$."
        );
    }
}
