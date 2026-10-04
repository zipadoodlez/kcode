use super::*;

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

#[test]
fn the_common_forms_translate() {
    let (params, quiet) = parse(&argv(&["-rn", "needle", "."]))
        .expect("`grep -rn` is the form this front end exists for");
    assert_eq!(params.mode, "grep");
    assert_eq!(params.query.as_deref(), Some("needle"));
    assert_eq!(params.path.as_deref(), Some("."));
    assert_eq!(params.regex, Some(false));
    assert_eq!(params.paths_only, Some(false));
    assert!(!quiet);
}

#[test]
fn flags_and_globs_map_onto_the_query() {
    let (params, _) = parse(&argv(&["-rnE", "needle|thread", "src", "--include=*.rs"]))
        .expect("regex and include translate");
    assert_eq!(params.regex, Some(true));
    assert_eq!(params.glob.as_deref(), Some("*.rs"));

    let (params, _) = parse(&argv(&["-rln", "needle", "."])).expect("-l asks for names only");
    assert_eq!(params.paths_only, Some(true));

    let (_, quiet) =
        parse(&argv(&["-qrn", "needle"])).expect("-q with -r searches the working directory");
    assert!(quiet);

    let (params, _) = parse(&argv(&["-e", "needle", "-rn"])).expect("-e names the pattern");
    assert_eq!(params.query.as_deref(), Some("needle"));

    // One named file without `-r` is a plain single-file search.
    let (params, _) =
        parse(&argv(&["needle", "src/main.rs"])).expect("a single named file translates");
    assert_eq!(params.path.as_deref(), Some("src/main.rs"));
}

#[test]
fn everything_else_stays_greps() {
    for invocation in [
        // Flags the engine does not model.
        vec!["-i", "-rn", "needle", "."],
        vec!["-c", "-rn", "needle", "."],
        vec!["-w", "-rn", "needle", "."],
        vec!["-o", "-rn", "needle", "."],
        vec!["-A", "3", "-rn", "needle", "."],
        vec!["--exclude-dir=target", "-rn", "needle", "."],
        // Two roots, and a `-` root, which is stdin.
        vec!["-rn", "needle", ".", "src"],
        vec!["-rn", "needle", "-"],
        // A pattern with no path and no `-r` is grep's stdin.
        vec!["needle"],
        // Nothing to search with.
        vec!["-rn"],
    ] {
        assert!(
            parse(&argv(&invocation[..])).is_none(),
            "{invocation:?} must stay grep's"
        );
    }
}

#[test]
fn a_match_exits_zero_and_a_miss_exits_one() {
    let dir = std::env::temp_dir().join(format!("kcode-search-shim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("sample.rs"), "fn needle_marker() {}\n").expect("write sample");
    let root = dir.to_string_lossy().to_string();

    assert_eq!(run(&argv(&["-rn", "needle_marker", &root])), 0);
    assert_eq!(run(&argv(&["-rn", "absent_marker_xyz", &root])), 1);

    std::fs::remove_dir_all(&dir).ok();
}
