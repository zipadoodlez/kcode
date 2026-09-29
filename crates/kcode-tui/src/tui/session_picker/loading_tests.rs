use super::*;
use std::path::Path;

struct EnvVarGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set_path(key: &'static str, value: &std::path::Path) -> Self {
        let prev = std::env::var_os(key);
        crate::env::set_var(key, value);
        Self { key, prev }
    }

    fn set_str(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        crate::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.prev.take() {
            crate::env::set_var(self.key, prev);
        } else {
            crate::env::remove_var(self.key);
        }
    }
}

fn write_picker_snapshot(path: &Path, has_messages: bool) {
    let body = if has_messages {
        "{\"messages\":[{\"role\":\"user\"}]}"
    } else {
        "{\"messages\": []}"
    };
    std::fs::write(path, body).expect("write picker snapshot");
}

#[test]
fn collect_recent_session_stems_keeps_empty_snapshot_with_journal_history() {
    let temp = tempfile::tempdir().expect("temp dir");
    let stem = "session_alpha_1770000000000";
    write_picker_snapshot(&temp.path().join(format!("{stem}.json")), false);
    std::fs::write(
        temp.path().join(format!("{stem}.journal.jsonl")),
        "{\"append_messages\":[{\"role\":\"user\"}]}",
    )
    .expect("write journal");

    let stems = collect_recent_session_stems(temp.path(), 1).expect("collect stems");
    assert_eq!(stems, vec![stem.to_string()]);
}

#[test]
fn collect_recent_session_stems_expands_candidate_window_past_recent_empty_stubs() {
    let temp = tempfile::tempdir().expect("temp dir");

    for idx in 0..30 {
        let stem = format!("session_empty_{}", 1770000000030u64 - idx as u64);
        write_picker_snapshot(&temp.path().join(format!("{stem}.json")), false);
    }

    let older_stem = "session_full_1770000000000";
    write_picker_snapshot(&temp.path().join(format!("{older_stem}.json")), true);

    let stems = collect_recent_session_stems(temp.path(), 1).expect("collect stems");
    assert_eq!(stems, vec![older_stem.to_string()]);
}

#[test]
fn trivial_hidden_only_snapshot_detector_skips_system_stub() {
    let bytes = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"<system-reminder>boot</system-reminder>"}],"display_role":"system"}]}"#;
    assert!(snapshot_bytes_look_trivial_hidden_only(bytes));
}

#[test]
fn trivial_hidden_only_snapshot_detector_keeps_visible_message() {
    let bytes = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]}"#;
    assert!(!snapshot_bytes_look_trivial_hidden_only(bytes));
}

#[test]
fn trivial_hidden_only_snapshot_detector_keeps_system_plus_visible_message() {
    let bytes = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"<system-reminder>boot</system-reminder>"}],"display_role":"system"},{"role":"assistant","content":[{"type":"text","text":"visible"}]}]}"#;
    assert!(!snapshot_bytes_look_trivial_hidden_only(bytes));
}

#[test]
fn cached_grouped_sessions_round_trip_from_disk() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    let _scan_limit = EnvVarGuard::set_str("KCODE_SESSION_PICKER_MAX_SESSIONS", "100");
    let _include_saved = EnvVarGuard::set_str("KCODE_SESSION_PICKER_INCLUDE_OLD_SAVED", "0");

    let sessions_dir = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("create sessions dir");
    let now = chrono::Utc::now();
    let session = SessionInfo {
        id: "session_cache_test_1770000000000".to_string(),
        parent_id: None,
        short_name: "cache-test".to_string(),
        icon: "🧪".to_string(),
        title: "Cache test".to_string(),
        message_count: 1,
        user_message_count: 1,
        assistant_message_count: 0,
        created_at: now,
        last_message_time: now,
        last_active_at: Some(now),
        working_dir: Some("/tmp/cache-test".to_string()),
        model: None,
        provider_key: None,
        is_canary: false,
        is_debug: false,
        saved: false,
        save_label: None,
        status: SessionStatus::Closed,
        needs_catchup: false,
        estimated_tokens: 0,
        first_user_prompt: None,
        messages_preview: Vec::new(),
        search_index: "cache test".to_string(),
        server_name: None,
        server_icon: None,
    };
    let cache = GroupedSessionListDiskCache {
        version: SESSION_LIST_DISK_CACHE_VERSION,
        generated_at: now,
        sessions_dir,
        scan_limit: session_scan_limit(),
        include_old_saved_sessions: include_old_saved_sessions_on_initial_load(),
        server_groups: Vec::new(),
        orphan_sessions: vec![session],
    };

    let path = session_list_disk_cache_path().expect("cache path");
    crate::storage::write_json_fast(&path, &cache).expect("write cache");

    let (_groups, orphans) = load_cached_sessions_grouped().expect("load cache");
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].id, "session_cache_test_1770000000000");
    assert_eq!(orphans[0].title, "Cache test");
}

#[test]
fn load_sessions_labels_a_session_by_its_memorable_name() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    let session_id = "session_fox_1770000000000";

    let mut session = Session::create_with_id(
        session_id.to_string(),
        None,
        Some("Generated first prompt".to_string()),
    );
    session.append_stored_message(crate::session::StoredMessage {
        id: "msg1".to_string(),
        role: crate::message::Role::User,
        content: vec![crate::message::ContentBlock::Text {
            text: "please plan the release".to_string(),
            cache_control: None,
        }],
        display_role: None,
        timestamp: None,
        tool_duration_ms: None,
        token_usage: None,
    });
    session.save().expect("save session");
    invalidate_session_list_cache();

    let sessions = load_sessions().expect("load sessions");
    let loaded = sessions
        .iter()
        .find(|session| session.id == session_id)
        .expect("session present");
    assert_eq!(loaded.title, "fox");
}

#[test]
fn load_sessions_includes_saved_sessions_beyond_scan_limit() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    let _scan_limit = EnvVarGuard::set_str("KCODE_SESSION_PICKER_MAX_SESSIONS", "50");

    let mut saved_session = Session::create_with_id(
        "session_saved_beyond_scan_limit".to_string(),
        Some("/tmp/saved-beyond-scan".to_string()),
        Some("Saved Beyond Scan".to_string()),
    );
    saved_session.mark_saved(Some("Pinned Session".to_string()));
    saved_session.append_stored_message(crate::session::StoredMessage {
        id: "saved-msg".to_string(),
        role: crate::message::Role::User,
        content: vec![crate::message::ContentBlock::Text {
            text: "keep this bookmarked session visible".to_string(),
            cache_control: None,
        }],
        display_role: None,
        timestamp: None,
        tool_duration_ms: None,
        token_usage: None,
    });
    saved_session.save().expect("save saved session");

    for idx in 0..55 {
        let mut session = Session::create_with_id(
            format!("session_newer_unsaved_{idx:03}"),
            Some(format!("/tmp/newer-unsaved-{idx:03}")),
            Some(format!("Newer Unsaved {idx:03}")),
        );
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg-{idx}"),
            role: crate::message::Role::User,
            content: vec![crate::message::ContentBlock::Text {
                text: format!("newer unsaved session {idx:03}"),
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
        session.save().expect("save unsaved session");
    }
    invalidate_session_list_cache();

    let sessions = load_sessions().expect("load sessions");
    assert!(
        sessions
            .iter()
            .any(|session| session.id == "session_saved_beyond_scan_limit"),
        "saved sessions should remain visible even when the recency scan limit is full"
    );
}

#[test]
fn load_sessions_preserves_snapshot_saved_when_journal_meta_omits_saved() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());

    let mut session = Session::create_with_id(
        "session_saved_legacy_journal".to_string(),
        Some("/tmp/saved-legacy-journal".to_string()),
        Some("Saved Legacy Journal".to_string()),
    );
    session.mark_saved(Some("Legacy Saved".to_string()));
    session.append_stored_message(crate::session::StoredMessage {
        id: "saved-legacy-msg".to_string(),
        role: crate::message::Role::User,
        content: vec![crate::message::ContentBlock::Text {
            text: "saved session with old journal metadata".to_string(),
            cache_control: None,
        }],
        display_role: None,
        timestamp: None,
        tool_duration_ms: None,
        token_usage: None,
    });
    session.save().expect("save saved session");

    let snapshot = crate::session::session_path(&session.id).expect("session path");
    let journal = crate::session::session_journal_path_from_snapshot(&snapshot);
    std::fs::write(
        journal,
        format!(
            r#"{{"meta":{{"updated_at":{}}}}}
"#,
            serde_json::to_string(&chrono::Utc::now()).expect("updated_at json")
        ),
    )
    .expect("write legacy journal");
    invalidate_session_list_cache();

    let sessions = load_sessions().expect("load sessions");
    let loaded = sessions
        .iter()
        .find(|session| session.id == "session_saved_legacy_journal")
        .expect("legacy saved session visible");
    assert!(
        loaded.saved,
        "missing journal saved field must not clear snapshot saved state"
    );
}

#[test]
fn saved_metadata_detection_scans_tail_without_full_json_parse() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("session_large_saved.json");
    let large_messages = "x".repeat((super::SAVED_METADATA_TAIL_SCAN_BYTES as usize) + 16_384);
    std::fs::write(
        &path,
        format!(
            r#"{{"id":"session_large_saved","messages":[{{"role":"user","content":"{large_messages}"}}],"saved"  : true}}"#
        ),
    )
    .expect("write session");

    assert!(super::session_snapshot_or_journal_has_saved_metadata(&path));
}

#[test]
fn raw_content_system_reminder_detection_handles_arrays_strings_and_unicode() {
    let raw_string: Box<serde_json::value::RawValue> =
        serde_json::from_str(r#""   <system-reminder>\nlegacy""#).expect("raw string");
    assert!(raw_content_starts_with_system_reminder(&raw_string));

    let raw_content_array: Box<serde_json::value::RawValue> =
        serde_json::from_str(r#"[{"type":"text","text":"<system-reminder>\n# Session Context"}]"#)
            .expect("raw array");
    assert!(raw_content_starts_with_system_reminder(&raw_content_array));

    let long_unicode = "─".repeat(3000);
    let raw_long_tool_result: Box<serde_json::value::RawValue> = serde_json::from_str(&format!(
        r#"[{{"type":"tool_result","content":{}}}]"#,
        serde_json::to_string(&long_unicode).expect("json string")
    ))
    .expect("raw long unicode");
    assert!(!raw_content_starts_with_system_reminder(
        &raw_long_tool_result
    ));
}

#[test]
fn session_matches_query_searches_kcode_transcript_contents() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());

    let mut session = Session::create_with_id(
        "session_transcript_search".to_string(),
        Some("/tmp/transcript-search".to_string()),
        Some("Transcript Search".to_string()),
    );
    session.append_stored_message(crate::session::StoredMessage {
        id: "msg1".to_string(),
        role: crate::message::Role::User,
        content: vec![crate::message::ContentBlock::Text {
            text: "please find the zebra needle hidden in transcript text".to_string(),
            cache_control: None,
        }],
        display_role: None,
        timestamp: None,
        tool_duration_ms: None,
        token_usage: None,
    });
    session.save().expect("save session");

    let sessions = load_sessions().expect("load sessions");
    let loaded = sessions
        .iter()
        .find(|candidate| candidate.id == "session_transcript_search")
        .expect("session present");

    assert!(loaded.search_index.contains("zebra needle"));
    assert!(loaded.messages_preview.is_empty());
    assert!(session_matches_query(loaded, "zebra needle"));
    assert!(session_matches_query(loaded, "ZEBRA NEEDLE"));
    assert!(!session_matches_query(loaded, "missing transcript phrase"));
}

#[test]
fn kcode_search_index_keeps_late_turns_after_reaching_its_budget() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    invalidate_session_list_cache();

    let mut session = Session::create_with_id(
        "session_late_transcript_search".to_string(),
        Some("/tmp/transcript-search".to_string()),
        Some("Late Transcript Search".to_string()),
    );
    for index in 0..12 {
        let text = if index == 11 {
            format!("{} final-turn-platypus", "x".repeat(7_500))
        } else {
            format!("turn-{index} {}", "x".repeat(7_500))
        };
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg{index}"),
            role: crate::message::Role::User,
            content: vec![crate::message::ContentBlock::Text {
                text,
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
    }
    session.save().expect("save session");

    let sessions = load_sessions().expect("load sessions");
    let loaded = sessions
        .iter()
        .find(|candidate| candidate.id == "session_late_transcript_search")
        .expect("session present");
    assert!(loaded.search_index.len() <= INITIAL_TRANSCRIPT_SEARCH_BUDGET_BYTES + 256);
    assert!(session_matches_picker_query(loaded, "final-turn-platypus"));
    invalidate_session_list_cache();
}

#[test]
fn raw_search_excerpt_samples_suffix_of_one_long_message_without_splitting_utf8() {
    let prefix = "opening-message-needle";
    let suffix = "晚い-message-needle";
    let serialized = serde_json::to_string(&format!("{prefix} {} {suffix}", "─".repeat(6_000)))
        .expect("serialize content");
    let raw: Box<serde_json::value::RawValue> =
        serde_json::from_str(&serialized).expect("raw value");

    let excerpt =
        raw_value_search_excerpt(&raw, MESSAGE_SEARCH_EXCERPT_BYTES).expect("search excerpt");
    assert!(excerpt.contains(prefix));
    assert!(excerpt.contains(suffix));
    assert!(excerpt.len() <= MESSAGE_SEARCH_EXCERPT_BYTES);
}

#[test]
#[ignore = "developer benchmark: times real /resume loading phases"]
fn benchmark_real_resume_loading_phases() {
    invalidate_session_list_cache();

    let sessions_dir = storage::kcode_dir().expect("kcode dir").join("sessions");
    let scan_limit = session_scan_limit();
    let candidate_limit = session_candidate_window(scan_limit);

    let phase_start = std::time::Instant::now();
    let candidates = if sessions_dir.exists() {
        collect_recent_session_candidates(&sessions_dir, candidate_limit)
            .expect("collect recent session candidates")
    } else {
        Vec::new()
    };
    let collect_candidates_elapsed = phase_start.elapsed();

    let mut sessions = Vec::new();
    let mut skipped_empty = 0usize;
    let mut summary_errors = 0usize;
    let phase_start = std::time::Instant::now();
    for stem in &candidates {
        if sessions.len() >= scan_limit {
            let saved = sessions_dir.join(format!("{stem}.json"));
            if !session_snapshot_or_journal_has_saved_metadata(&saved) {
                continue;
            }
        }

        let path = sessions_dir.join(format!("{stem}.json"));
        match load_session_summary(&path) {
            Ok(summary) if summary.messages.visible_message_count > 0 => {
                sessions.push((stem.clone(), summary));
            }
            Ok(_) => skipped_empty += 1,
            Err(_) => summary_errors += 1,
        }
    }
    let kcode_summary_elapsed = phase_start.elapsed();

    let phase_start = std::time::Instant::now();
    let all_sessions = load_sessions().expect("load sessions");
    let load_sessions_elapsed = phase_start.elapsed();

    invalidate_session_list_cache();
    let phase_start = std::time::Instant::now();
    let (groups, orphans) = load_sessions_grouped().expect("load grouped sessions");
    let grouped_elapsed = phase_start.elapsed();

    let snapshot_count = std::fs::read_dir(&sessions_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry.file_name().to_str().is_some_and(|name| {
                        name.ends_with(".json") && !name.ends_with(".journal.json")
                    })
                })
                .count()
        })
        .unwrap_or_default();

    eprintln!(
        concat!(
            "real resume phases: scan_limit={} candidate_limit={} snapshot_count={} ",
            "candidate_count={} collect_candidates={}ms ",
            "kcode_summary={}ms kcode_loaded={} skipped_empty={} summary_errors={} ",
            "load_sessions={}ms/{} load_sessions_grouped={}ms groups={} orphans={}"
        ),
        scan_limit,
        candidate_limit,
        snapshot_count,
        candidates.len(),
        collect_candidates_elapsed.as_millis(),
        kcode_summary_elapsed.as_millis(),
        sessions.len(),
        skipped_empty,
        summary_errors,
        load_sessions_elapsed.as_millis(),
        all_sessions.len(),
        grouped_elapsed.as_millis(),
        groups.len(),
        orphans.len(),
    );
}

#[test]
#[ignore = "developer benchmark: scans the real KCODE_HOME session directory"]
fn benchmark_real_resume_loading_reports_timings() {
    invalidate_session_list_cache();

    let load_start = std::time::Instant::now();
    let sessions = load_sessions().expect("load real sessions");
    let load_elapsed = load_start.elapsed();

    invalidate_session_list_cache();
    let grouped_start = std::time::Instant::now();
    let grouped = load_sessions_grouped().expect("load real grouped sessions");
    let grouped_elapsed = grouped_start.elapsed();
    let grouped_count = grouped
        .0
        .iter()
        .map(|group| group.sessions.len())
        .sum::<usize>()
        + grouped.1.len();

    eprintln!(
        "real resume bench: load_sessions={}ms count={} load_sessions_grouped={}ms grouped_count={} server_groups={} orphan_sessions={}",
        load_elapsed.as_millis(),
        sessions.len(),
        grouped_elapsed.as_millis(),
        grouped_count,
        grouped.0.len(),
        grouped.1.len()
    );
}

#[test]
fn benchmark_resume_loading_reports_timings() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());

    let sessions_dir = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    for idx in 0..120 {
        let mut session = Session::create_with_id(
            format!("session_resume_bench_{idx:03}"),
            Some(format!("/tmp/resume-bench-{idx:03}")),
            Some(format!("Resume Bench {idx:03}")),
        );
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg-{idx}-1"),
            role: crate::message::Role::User,
            content: vec![crate::message::ContentBlock::Text {
                text: format!("session {idx:03} says benchmark transcript token zebra-{idx:03}"),
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg-{idx}-2"),
            role: crate::message::Role::Assistant,
            content: vec![crate::message::ContentBlock::Text {
                text: "assistant reply for benchmark coverage".to_string(),
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
        session.save().expect("save benchmark session");
    }

    let load_start = std::time::Instant::now();
    let sessions = load_sessions().expect("load sessions");
    let load_elapsed = load_start.elapsed();

    let group_start = std::time::Instant::now();
    let grouped = load_sessions_grouped().expect("load grouped sessions");
    let group_elapsed = group_start.elapsed();

    assert!(sessions.len() >= 100);
    assert!(!grouped.0.is_empty() || !grouped.1.is_empty());

    eprintln!(
        "resume bench: load_sessions={}ms load_sessions_grouped={}ms count={}",
        load_elapsed.as_millis(),
        group_elapsed.as_millis(),
        sessions.len()
    );
}

#[test]
fn parallel_fill_skips_many_recent_empty_sessions_to_reach_scan_limit() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    let _scan_limit = EnvVarGuard::set_str("KCODE_SESSION_PICKER_MAX_SESSIONS", "50");

    let sessions_dir = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions_dir).expect("create sessions dir");

    let push_message = |session: &mut Session, text: &str| {
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg-{text}"),
            role: crate::message::Role::User,
            content: vec![crate::message::ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
    };

    // Many recent but empty sessions (no visible messages) that the parallel
    // two-phase fill must skip while still collecting `scan_limit` real ones.
    for idx in 0..200 {
        let mut session = Session::create_with_id(
            format!("session_empty_{}", 1_790_000_000_000u64 + idx as u64),
            Some(format!("/tmp/empty-{idx:03}")),
            Some(format!("Empty {idx:03}")),
        );
        session.save().expect("save empty session");
    }
    // Older but non-empty sessions that should fill the list despite being less
    // recent than the empty stubs above.
    for idx in 0..60 {
        let mut session = Session::create_with_id(
            format!("session_full_{}", 1_780_000_000_000u64 + idx as u64),
            Some(format!("/tmp/full-{idx:03}")),
            Some(format!("Full {idx:03}")),
        );
        push_message(&mut session, &format!("real content {idx:03}"));
        session.save().expect("save full session");
    }

    invalidate_session_list_cache();
    let sessions = load_sessions().expect("load sessions");
    let visible: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|s| s.id.starts_with("session_full_"))
        .collect();
    assert_eq!(
        visible.len(),
        50,
        "expected exactly scan_limit non-empty sessions, got {}",
        visible.len()
    );
    assert!(
        !sessions.iter().any(|s| s.id.starts_with("session_empty_")),
        "empty sessions must be filtered out of the loaded list"
    );
}

#[test]
fn hidden_debug_sessions_do_not_consume_default_resume_budget() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());
    let _scan_limit = EnvVarGuard::set_str("KCODE_SESSION_PICKER_MAX_SESSIONS", "50");

    let push_message = |session: &mut Session, text: &str| {
        session.append_stored_message(crate::session::StoredMessage {
            id: format!("msg-{text}"),
            role: crate::message::Role::User,
            content: vec![crate::message::ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
            display_role: None,
            timestamp: None,
            tool_duration_ms: None,
            token_usage: None,
        });
    };

    // Write ordinary sessions first so their filesystem mtimes are older than
    // the hidden debug burst below, matching the reported real-world ordering.
    for idx in 0..60 {
        let mut session = Session::create_with_id(
            format!("session_regular_{}", 1_780_000_000_000u64 + idx as u64),
            Some(format!("/tmp/regular-{idx:03}")),
            Some(format!("Regular {idx:03}")),
        );
        session.is_debug = false;
        session.is_canary = false;
        push_message(&mut session, &format!("regular content {idx:03}"));
        session.save().expect("save regular session");
    }

    // These newer self-dev/worker sessions are hidden by default. Previously the
    // loader stopped after the first 50, leaving no ordinary Kcode sessions for
    // the picker even though older resumable sessions existed.
    for idx in 0..75 {
        let mut session = Session::create_with_id(
            format!("session_debug_{}", 1_790_000_000_000u64 + idx as u64),
            Some(format!("/tmp/debug-{idx:03}")),
            Some(format!("Debug {idx:03}")),
        );
        session.is_debug = true;
        push_message(&mut session, &format!("debug content {idx:03}"));
        session.save().expect("save debug session");
    }

    invalidate_session_list_cache();
    let sessions = load_sessions().expect("load sessions");
    let regular_count = sessions.iter().filter(|session| !session.is_debug).count();
    let debug_count = sessions.iter().filter(|session| session.is_debug).count();

    assert_eq!(
        regular_count, 50,
        "ordinary sessions should fill the visible budget"
    );
    assert_eq!(
        debug_count, 50,
        "debug sessions should retain their own bounded budget"
    );
}

#[test]
fn session_matches_picker_query_requires_all_tokens_order_independent() {
    let _env_lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let _home = EnvVarGuard::set_path("KCODE_HOME", temp.path());

    let mut session = Session::create_with_id(
        "session_token_match".to_string(),
        Some("/tmp/token-match".to_string()),
        Some("Token Match".to_string()),
    );
    session.append_stored_message(crate::session::StoredMessage {
        id: "msg1".to_string(),
        role: crate::message::Role::User,
        content: vec![crate::message::ContentBlock::Text {
            text: "please deploy the production api gateway now".to_string(),
            cache_control: None,
        }],
        display_role: None,
        timestamp: None,
        tool_duration_ms: None,
        token_usage: None,
    });
    session.save().expect("save session");

    let sessions = load_sessions().expect("load sessions");
    let loaded = sessions
        .iter()
        .find(|candidate| candidate.id == "session_token_match")
        .expect("session present");

    // All tokens present, any order -> match (the old contiguous-substring matcher
    // would have failed on reordered / non-adjacent words).
    assert!(session_matches_picker_query(loaded, "api deploy"));
    assert!(session_matches_picker_query(loaded, "deploy api"));
    assert!(session_matches_picker_query(loaded, "  DEPLOY   Gateway  "));
    // A token that doesn't appear anywhere -> no match, even if others do.
    assert!(!session_matches_picker_query(loaded, "deploy staging"));
    // Empty query matches everything.
    assert!(session_matches_picker_query(loaded, "   "));
}
