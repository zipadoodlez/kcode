use crate::id::{extract_session_name, session_icon};
use crate::message::Role;
use crate::registry::{self, ServerInfo};
use crate::session::{self, CrashedSessionsInfo, Session, SessionStatus, StoredDisplayRole};
use crate::storage;
use anyhow::Result;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::{
    DEFAULT_SESSION_SCAN_LIMIT, MAX_SESSION_SCAN_LIMIT, MIN_SESSION_SCAN_LIMIT, PreviewMessage,
    SEARCH_CONTENT_BUDGET_BYTES, ServerGroup, SessionInfo,
};

#[cfg(test)]
const TRANSCRIPT_SEARCH_CHUNK_BYTES: usize = 64 * 1024;

fn session_scan_limit() -> usize {
    std::env::var("KCODE_SESSION_PICKER_MAX_SESSIONS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map(|n| n.clamp(MIN_SESSION_SCAN_LIMIT, MAX_SESSION_SCAN_LIMIT))
        .unwrap_or(DEFAULT_SESSION_SCAN_LIMIT)
}

fn session_candidate_window(scan_limit: usize) -> usize {
    scan_limit
        .saturating_mul(20)
        .clamp(scan_limit.max(1), 20_000)
}

fn include_old_saved_sessions_on_initial_load() -> bool {
    std::env::var("KCODE_SESSION_PICKER_INCLUDE_OLD_SAVED")
        .ok()
        .is_some_and(|raw| matches!(raw.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

const SESSION_LIST_CACHE_TTL: Duration = Duration::from_secs(5);
const SESSION_LIST_DISK_CACHE_VERSION: u32 = 2;
const SESSION_LIST_DISK_CACHE_MAX_AGE_SECONDS: i64 = 7 * 24 * 60 * 60;
const SAVED_METADATA_TAIL_SCAN_BYTES: u64 = 64 * 1024;
const INITIAL_TRANSCRIPT_SEARCH_BUDGET_BYTES: usize = 64 * 1024;
const MESSAGE_SEARCH_EXCERPT_BYTES: usize = 8 * 1024;

/// Upper bound on worker threads used to parse/stat session files in parallel.
/// The session picker load is dominated by per-file IO + JSON parsing across
/// hundreds of snapshots; fanning that work out across cores turns the cold
/// `/resume` load from a serial slog into a roughly core-count-bounded scan.
const SESSION_LOAD_MAX_THREADS: usize = 8;

/// Number of worker threads to use for a parallel pass over `item_count` items.
/// Returns 1 for tiny batches so we never pay thread-spawn overhead when there
/// is barely any work to do.
fn session_load_thread_count(item_count: usize) -> usize {
    if item_count <= 1 {
        return 1;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    cores.clamp(1, SESSION_LOAD_MAX_THREADS).min(item_count)
}

/// Map `f` over `items` across a bounded scoped thread pool, preserving input
/// order in the returned vector. Falls back to a plain serial map when only one
/// worker is warranted. `f` must be `Sync` because every worker shares it.
fn parallel_map<T, R, F>(items: Vec<T>, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let thread_count = session_load_thread_count(items.len());
    if thread_count <= 1 {
        return items.into_iter().map(f).collect();
    }

    // Partition the work into `thread_count` owned chunks so each worker can
    // take its inputs by value (no clone, no shared mutation). We remember the
    // starting offset of each chunk to stitch results back into input order.
    let chunk_size = items.len().div_ceil(thread_count);
    let mut chunks: Vec<(usize, Vec<T>)> = Vec::with_capacity(thread_count);
    let mut offset = 0usize;
    let mut remaining = items;
    while !remaining.is_empty() {
        let take = chunk_size.min(remaining.len());
        let rest = remaining.split_off(take);
        chunks.push((offset, remaining));
        offset += take;
        remaining = rest;
    }

    let f = &f;
    let mut results: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(chunks.len());
        for (start, chunk) in chunks {
            handles
                .push(scope.spawn(move || (start, chunk.into_iter().map(f).collect::<Vec<R>>())));
        }
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    });

    results.sort_by_key(|(start, _)| *start);
    let total: usize = results.iter().map(|(_, chunk)| chunk.len()).sum();
    let mut out = Vec::with_capacity(total);
    for (_, chunk) in results {
        out.extend(chunk);
    }
    out
}

#[derive(Clone)]
struct SessionListCacheEntry {
    loaded_at: Instant,
    sessions_dir: PathBuf,
    scan_limit: usize,
    sessions: Vec<SessionInfo>,
}

#[derive(Serialize, Deserialize)]
struct GroupedSessionListDiskCache {
    version: u32,
    generated_at: chrono::DateTime<chrono::Utc>,
    sessions_dir: PathBuf,
    scan_limit: usize,
    include_old_saved_sessions: bool,
    server_groups: Vec<ServerGroup>,
    orphan_sessions: Vec<SessionInfo>,
}

fn session_list_cache() -> &'static Mutex<Option<SessionListCacheEntry>> {
    static CACHE: OnceLock<Mutex<Option<SessionListCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn invalidate_session_list_cache() {
    if let Ok(mut cache) = session_list_cache().lock() {
        *cache = None;
    }
}

fn session_list_disk_cache_path() -> Result<PathBuf> {
    Ok(storage::kcode_dir()?.join("cache/session-picker-list-v2.json"))
}

fn session_list_disk_cache_is_usable(
    cache: &GroupedSessionListDiskCache,
    sessions_dir: &Path,
    scan_limit: usize,
) -> bool {
    cache.version == SESSION_LIST_DISK_CACHE_VERSION
        && cache.sessions_dir == sessions_dir
        && cache.scan_limit == scan_limit
        && cache.include_old_saved_sessions == include_old_saved_sessions_on_initial_load()
        && chrono::Utc::now()
            .signed_duration_since(cache.generated_at)
            .num_seconds()
            <= SESSION_LIST_DISK_CACHE_MAX_AGE_SECONDS
}

fn write_grouped_session_list_disk_cache(
    sessions_dir: &Path,
    scan_limit: usize,
    server_groups: &[ServerGroup],
    orphan_sessions: &[SessionInfo],
) {
    let Ok(path) = session_list_disk_cache_path() else {
        return;
    };
    let cache = GroupedSessionListDiskCache {
        version: SESSION_LIST_DISK_CACHE_VERSION,
        generated_at: chrono::Utc::now(),
        sessions_dir: sessions_dir.to_path_buf(),
        scan_limit,
        include_old_saved_sessions: include_old_saved_sessions_on_initial_load(),
        server_groups: server_groups.to_vec(),
        orphan_sessions: orphan_sessions.to_vec(),
    };
    if let Err(err) = storage::write_json_fast(&path, &cache) {
        crate::logging::debug(&format!(
            "failed to write session picker disk cache {}: {}",
            path.display(),
            err
        ));
    }
}

pub fn load_cached_sessions_grouped() -> Option<(Vec<ServerGroup>, Vec<SessionInfo>)> {
    let sessions_dir = storage::kcode_dir().ok()?.join("sessions");
    let scan_limit = session_scan_limit();
    let path = session_list_disk_cache_path().ok()?;
    let cache: GroupedSessionListDiskCache = storage::read_json(&path).ok()?;
    if !session_list_disk_cache_is_usable(&cache, &sessions_dir, scan_limit) {
        return None;
    }
    Some((cache.server_groups, cache.orphan_sessions))
}

fn push_with_byte_budget(dst: &mut String, src: &str, budget: &mut usize) {
    if *budget == 0 || src.is_empty() {
        return;
    }

    let mut end = src.len().min(*budget);
    while end > 0 && !src.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return;
    }

    dst.push_str(&src[..end]);
    *budget = budget.saturating_sub(end);
}

fn suffix_at_most(value: &str, max_bytes: usize) -> &str {
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}

/// Keep a bounded sample from both ends of a growing transcript. Keeping only
/// the first 64 KiB made `/resume` search silently blind to every later turn in
/// a long session. The first half retains titles and early prompts while the
/// second half continuously follows the newest transcript content.
fn push_sampled_search_text(dst: &mut String, src: &str, limit: usize) {
    if src.is_empty() || limit == 0 {
        return;
    }
    if dst.len().saturating_add(1).saturating_add(src.len()) <= limit {
        dst.push(' ');
        dst.push_str(src);
        return;
    }

    // An oversized first message has no existing session head to preserve. Keep
    // both ends of that message instead of retaining only its suffix.
    if dst.is_empty() {
        let head_budget = limit / 2;
        let mut head_end = src.len().min(head_budget);
        while head_end > 0 && !src.is_char_boundary(head_end) {
            head_end -= 1;
        }
        dst.push_str(&src[..head_end]);
        dst.push_str(suffix_at_most(src, limit.saturating_sub(head_end)));
        return;
    }

    let head_budget = limit / 2;
    let mut head_end = dst.len().min(head_budget);
    while head_end > 0 && !dst.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let head = dst[..head_end].to_string();

    let tail_budget = limit.saturating_sub(head.len());
    let tail = if src.len().saturating_add(1) >= tail_budget {
        suffix_at_most(src, tail_budget).to_string()
    } else {
        let old_budget = tail_budget.saturating_sub(src.len() + 1);
        let old_tail = suffix_at_most(&dst[head_end..], old_budget);
        let mut tail = String::with_capacity(old_tail.len() + 1 + src.len());
        tail.push_str(old_tail);
        tail.push(' ');
        tail.push_str(src);
        tail
    };

    dst.clear();
    dst.push_str(&head);
    dst.push_str(&tail);
}

pub(super) fn build_search_index(
    id: &str,
    short_name: &str,
    title: &str,
    working_dir: Option<&str>,
    save_label: Option<&str>,
    messages_preview: &[PreviewMessage],
) -> String {
    let mut combined = String::new();
    combined.push_str(title);
    combined.push(' ');
    combined.push_str(short_name);
    combined.push(' ');
    combined.push_str(id);

    if let Some(dir) = working_dir {
        combined.push(' ');
        combined.push_str(dir);
    }

    if let Some(label) = save_label {
        combined.push(' ');
        combined.push_str(label);
    }

    let mut budget = SEARCH_CONTENT_BUDGET_BYTES;
    for msg in messages_preview {
        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }
        combined.push(' ');
        push_with_byte_budget(&mut combined, content, &mut budget);
        if budget == 0 {
            break;
        }
    }

    combined.to_lowercase()
}

fn raw_value_search_excerpt(raw: &RawValue, budget: usize) -> Option<String> {
    if budget == 0 {
        return None;
    }
    let raw = raw.get();
    let budget = budget.min(MESSAGE_SEARCH_EXCERPT_BYTES);
    let mut excerpt = String::new();
    push_sampled_search_text(&mut excerpt, raw, budget);
    (!excerpt.is_empty()).then_some(excerpt)
}

fn raw_value_display_text(raw: &RawValue) -> Option<String> {
    fn collect_text(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect_text(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(text);
                }
            }
            _ => {}
        }
    }

    let value: serde_json::Value = serde_json::from_str(raw.get()).ok()?;
    let mut text = String::new();
    collect_text(&value, &mut text);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
pub(super) fn session_matches_query(session: &SessionInfo, query: &str) -> bool {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    if session.search_index.contains(&normalized) {
        return true;
    }

    session_transcript_contains_query(session, &normalized)
}

/// Fast in-memory matcher for interactive picker filtering.
///
/// Splits the query into whitespace-separated tokens and requires *every* token
/// to appear somewhere in the session's search index (logical AND, order
/// independent). This is far more forgiving than a single contiguous substring
/// match - `api deploy` now matches a session mentioning "deploy ... api" - while
/// staying cheap: it runs on every keystroke and only does N case-insensitive
/// substring scans over an already-lowercased index.
///
/// This intentionally avoids transcript file I/O. Transcript-backed content can
/// still become searchable after preview load because the picker refreshes the
/// session's cached `search_index` from the loaded preview.
pub(super) fn session_matches_picker_query(session: &SessionInfo, query: &str) -> bool {
    let tokens = search_query_tokens(query);
    tokens.is_empty()
        || tokens
            .iter()
            .all(|token| session.search_index.contains(token))
}

/// Split a raw query into normalized (lowercased, whitespace-trimmed) search
/// tokens. Empty/whitespace-only queries yield no tokens (match everything).
pub(super) fn search_query_tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect()
}

#[cfg(test)]
fn session_transcript_contains_query(session: &SessionInfo, query_lower: &str) -> bool {
    transcript_paths_for_session(session)
        .into_iter()
        .any(|path| file_contains_case_insensitive_query(&path, query_lower))
}

#[cfg(test)]
fn transcript_paths_for_session(session: &SessionInfo) -> Vec<PathBuf> {
    let Ok(sessions_dir) = storage::kcode_dir().map(|dir| dir.join("sessions")) else {
        return Vec::new();
    };
    vec![
        sessions_dir.join(format!("{}.json", session.id)),
        sessions_dir.join(format!("{}.journal.jsonl", session.id)),
    ]
}

#[cfg(test)]
fn file_contains_case_insensitive_query(path: &Path, query_lower: &str) -> bool {
    if query_lower.is_empty() {
        return true;
    }
    if !path.exists() {
        return false;
    }

    if query_lower.is_ascii() {
        return file_contains_ascii_case_insensitive(path, query_lower.as_bytes());
    }

    std::fs::read_to_string(path)
        .ok()
        .map(|content| content.to_lowercase().contains(query_lower))
        .unwrap_or(false)
}

#[cfg(test)]
fn file_contains_ascii_case_insensitive(path: &Path, needle_lower: &[u8]) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let overlap = needle_lower.len().saturating_sub(1);
    let mut carry = Vec::with_capacity(overlap);
    let mut buf = vec![0u8; TRANSCRIPT_SEARCH_CHUNK_BYTES];

    loop {
        let read = match reader.read(&mut buf) {
            Ok(0) => return false,
            Ok(read) => read,
            Err(_) => return false,
        };

        let mut window = Vec::with_capacity(carry.len() + read);
        window.extend_from_slice(&carry);
        window.extend_from_slice(&buf[..read]);

        if contains_ascii_case_insensitive_bytes(&window, needle_lower) {
            return true;
        }

        carry.clear();
        let keep = overlap.min(window.len());
        carry.extend_from_slice(&window[window.len().saturating_sub(keep)..]);
    }
}

#[cfg(test)]
fn contains_ascii_case_insensitive_bytes(haystack: &[u8], needle_lower: &[u8]) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle_lower.len()).any(|window| {
        window
            .iter()
            .zip(needle_lower.iter())
            .all(|(&hay, &needle)| hay.to_ascii_lowercase() == needle)
    })
}

fn build_search_index_from_summary(
    id: &str,
    short_name: &str,
    title: &str,
    working_dir: Option<&str>,
    save_label: Option<&str>,
    transcript_search_text: &str,
) -> String {
    let mut combined = String::new();
    combined.push_str(title);
    combined.push(' ');
    combined.push_str(short_name);
    combined.push(' ');
    combined.push_str(id);

    if let Some(dir) = working_dir {
        combined.push(' ');
        combined.push_str(dir);
    }

    if let Some(label) = save_label {
        combined.push(' ');
        combined.push_str(label);
    }

    if !transcript_search_text.is_empty() {
        combined.push(' ');
        combined.push_str(transcript_search_text);
    }

    combined.to_lowercase()
}

fn session_sort_key(stem: &str) -> u64 {
    for part in stem.split('_') {
        if part.len() == 13
            && part.as_bytes().iter().all(|b| b.is_ascii_digit())
            && let Ok(ts) = part.parse::<u64>()
        {
            return ts;
        }
    }

    stem.split('_')
        .rev()
        .find_map(|part| part.parse::<u64>().ok())
        .unwrap_or(0)
}

fn path_modified_sort_key(path: &Path) -> u128 {
    path.metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct SessionCandidateMeta {
    modified: u128,
    sort_key: u64,
    has_snapshot: bool,
}

impl SessionCandidateMeta {
    fn new(stem: &str) -> Self {
        Self {
            modified: 0,
            sort_key: session_sort_key(stem),
            has_snapshot: false,
        }
    }

    fn update(&mut self, modified: u128, has_snapshot: bool) {
        self.modified = self.modified.max(modified);
        self.has_snapshot |= has_snapshot;
    }
}

fn session_file_stem_for_candidate(file_name: &str) -> Option<(&str, bool)> {
    if let Some(stem) = file_name.strip_suffix(".journal.jsonl") {
        return Some((stem, false));
    }

    let stem = file_name.strip_suffix(".json")?;
    if stem.ends_with(".journal") {
        return None;
    }
    Some((stem, true))
}

#[cfg(test)]
fn value_first_text(value: &serde_json::Value) -> Option<&str> {
    match value {
        serde_json::Value::String(text) => Some(text.as_str()),
        serde_json::Value::Array(items) => items.iter().find_map(value_first_text),
        serde_json::Value::Object(map) => map.get("text").and_then(|text| text.as_str()),
        _ => None,
    }
}

#[cfg(test)]
fn message_value_is_internal_system_reminder(message: &serde_json::Value) -> bool {
    message
        .get("content")
        .and_then(value_first_text)
        .is_some_and(|text| text.trim_start().starts_with("<system-reminder>"))
}

#[cfg(test)]
fn message_value_is_visible_conversation(message: &serde_json::Value) -> bool {
    let has_display_role = message
        .get("display_role")
        .is_some_and(|value| !value.is_null());
    !has_display_role && !message_value_is_internal_system_reminder(message)
}

#[cfg(test)]
fn snapshot_has_visible_conversation(path: &Path) -> Option<bool> {
    let content = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let messages = value.get("messages")?.as_array()?;
    Some(messages.iter().any(message_value_is_visible_conversation))
}

#[cfg(test)]
fn snapshot_bytes_look_trivial_hidden_only(bytes: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    let Some(messages) = value
        .get("messages")
        .and_then(|messages| messages.as_array())
    else {
        return false;
    };
    !messages.is_empty() && !messages.iter().any(message_value_is_visible_conversation)
}

#[cfg(test)]
fn journal_has_visible_conversation(path: &Path) -> Option<bool> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut saw_parseable_line = false;
    for line in reader.lines().map_while(|line| line.ok()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        saw_parseable_line = true;
        let Some(messages) = value.get("append_messages").and_then(|v| v.as_array()) else {
            continue;
        };
        if messages.iter().any(message_value_is_visible_conversation) {
            return Some(true);
        }
    }
    saw_parseable_line.then_some(false)
}

#[cfg(test)]
fn is_empty_session_file(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return true;
    };
    let mut buf = [0u8; 300];
    let n = match file.take(300).read(&mut buf) {
        Ok(n) => n,
        Err(_) => return true,
    };
    let head = &buf[..n];
    head.windows(13).any(|w| w == b"\"messages\":[]")
        || head.windows(14).any(|w| w == b"\"messages\": []")
}

#[cfg(test)]
fn session_has_history(sessions_dir: &Path, stem: &str) -> bool {
    let snapshot_path = sessions_dir.join(format!("{stem}.json"));
    let journal_path = sessions_dir.join(format!("{stem}.journal.jsonl"));

    if journal_has_visible_conversation(&journal_path) == Some(true) {
        return true;
    }

    if let Some(has_visible) = snapshot_has_visible_conversation(&snapshot_path) {
        return has_visible;
    }

    if !is_empty_session_file(&snapshot_path) {
        return true;
    }

    journal_path
        .metadata()
        .map(|meta| meta.len() > 0)
        .unwrap_or(false)
}

fn collect_recent_session_candidates(
    sessions_dir: &Path,
    candidate_limit: usize,
) -> Result<Vec<String>> {
    // Phase 1: a single cheap `readdir` pass to enumerate candidate files. We
    // defer the per-file `stat` (the expensive part on directories with 100k+
    // session files) to a parallel pass so it does not serialize startup.
    let mut raw: Vec<(String, bool, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some((stem, has_snapshot)) = session_file_stem_for_candidate(file_name) else {
            continue;
        };
        raw.push((stem.to_string(), has_snapshot, entry.path()));
    }

    // Phase 2: stat each file's modification time in parallel.
    let stamped = parallel_map(raw, |(stem, has_snapshot, path)| {
        (stem, has_snapshot, path_modified_sort_key(&path))
    });

    // Phase 3: merge per-stem metadata (snapshot + newest journal/snapshot mtime).
    let mut by_stem: HashMap<String, SessionCandidateMeta> = HashMap::new();
    for (stem, has_snapshot, modified) in stamped {
        by_stem
            .entry(stem.clone())
            .or_insert_with(|| SessionCandidateMeta::new(&stem))
            .update(modified, has_snapshot);
    }

    let mut candidates: BinaryHeap<Reverse<(u128, u64, String)>> = BinaryHeap::new();
    for (stem, meta) in by_stem {
        if !meta.has_snapshot {
            continue;
        }
        let key = (meta.modified, meta.sort_key, stem);
        if candidates.len() < candidate_limit {
            candidates.push(Reverse(key));
            continue;
        }

        let should_replace = candidates
            .peek()
            .map(|smallest| key > smallest.0)
            .unwrap_or(true);
        if should_replace {
            candidates.pop();
            candidates.push(Reverse(key));
        }
    }

    let mut out: Vec<(u128, u64, String)> = candidates.into_iter().map(|entry| entry.0).collect();
    out.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
    });
    Ok(out.into_iter().map(|(_, _, stem)| stem).collect())
}

fn json_bytes_saved_true(bytes: &[u8]) -> bool {
    let mut search_start = 0usize;
    while let Some(relative_idx) = bytes[search_start..]
        .windows(b"\"saved\"".len())
        .position(|window| window == b"\"saved\"")
    {
        let key_idx = search_start + relative_idx + b"\"saved\"".len();
        let mut idx = key_idx;
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if bytes.get(idx) != Some(&b':') {
            search_start = key_idx;
            continue;
        }
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if bytes[idx..].starts_with(b"true") {
            return true;
        }
        search_start = key_idx;
    }
    false
}

fn file_tail_contains_saved_true(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(len) = file.metadata().map(|meta| meta.len()) else {
        return false;
    };
    let read_len = len.min(SAVED_METADATA_TAIL_SCAN_BYTES);
    if file
        .seek(SeekFrom::Start(len.saturating_sub(read_len)))
        .is_err()
    {
        return false;
    }
    let mut bytes = Vec::with_capacity(read_len as usize);
    file.take(read_len).read_to_end(&mut bytes).is_ok() && json_bytes_saved_true(&bytes)
}

fn session_snapshot_or_journal_has_saved_metadata(snapshot_path: &Path) -> bool {
    let journal_path = session::session_journal_path_from_snapshot(snapshot_path);

    // This runs across every historical session during cold `/resume` load, so
    // never parse whole journals here. Saved metadata is persisted in snapshots
    // and repeated in journal meta updates; a bounded tail scan is enough to
    // keep saved sessions discoverable without making old multi-MB journals
    // dominate startup. A later full summary load still computes the exact
    // saved state for candidates that make it into the list.
    file_tail_contains_saved_true(snapshot_path) || file_tail_contains_saved_true(&journal_path)
}

fn collect_saved_session_candidates(sessions_dir: &Path) -> Result<Vec<String>> {
    let mut saved = Vec::new();
    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some((stem, has_snapshot)) = session_file_stem_for_candidate(file_name) else {
            continue;
        };
        if !has_snapshot || stem.starts_with("imported_") {
            continue;
        }
        let path = sessions_dir.join(format!("{stem}.json"));
        if session_snapshot_or_journal_has_saved_metadata(&path) {
            saved.push(stem.to_string());
        }
    }
    saved.sort();
    saved.dedup();
    Ok(saved)
}

#[cfg(test)]
pub(super) fn collect_recent_session_stems(
    sessions_dir: &Path,
    scan_limit: usize,
) -> Result<Vec<String>> {
    let mut candidate_limit = session_candidate_window(scan_limit);

    loop {
        let candidates = collect_recent_session_candidates(sessions_dir, candidate_limit)?;
        let mut recent = Vec::with_capacity(scan_limit);
        for stem in candidates {
            if !session_has_history(sessions_dir, &stem) {
                continue;
            }
            recent.push(stem);
            if recent.len() >= scan_limit {
                break;
            }
        }

        if recent.len() >= scan_limit || candidate_limit >= MAX_SESSION_SCAN_LIMIT {
            return Ok(recent);
        }

        candidate_limit = candidate_limit
            .saturating_mul(2)
            .min(MAX_SESSION_SCAN_LIMIT);
    }
}

#[derive(Deserialize)]
struct SessionSummary {
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    last_active_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    messages: SessionMessageSummaryData,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    short_name: Option<String>,
    #[serde(default)]
    provider_key: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    is_canary: bool,
    #[serde(default)]
    is_debug: bool,
    #[serde(default)]
    saved: bool,
    #[serde(default)]
    save_label: Option<String>,
    #[serde(default)]
    status: SessionStatus,
}

#[derive(Clone, Debug, Default)]
struct SessionMessageSummaryData {
    visible_message_count: usize,
    user_message_count: usize,
    assistant_message_count: usize,
    estimated_tokens: usize,
    first_user_prompt: Option<String>,
    search_text: String,
}

impl SessionMessageSummaryData {
    fn add_message(&mut self, message: &SessionMessageSummary) {
        if !summary_message_is_visible_conversation(message) {
            return;
        }

        self.visible_message_count += 1;
        match message.role {
            Role::User => {
                self.user_message_count += 1;
                if self.first_user_prompt.is_none() {
                    self.first_user_prompt = message.content_text.clone();
                }
            }
            Role::Assistant => self.assistant_message_count += 1,
        }
        if let Some(usage) = &message.token_usage {
            self.estimated_tokens = self
                .estimated_tokens
                .saturating_add(usage.total_tokens() as usize);
        }
        if let Some(raw_content) = message.content_raw.as_deref() {
            push_sampled_search_text(
                &mut self.search_text,
                raw_content,
                INITIAL_TRANSCRIPT_SEARCH_BUDGET_BYTES,
            );
        }
    }

    fn merge(&mut self, other: Self) {
        self.visible_message_count = self
            .visible_message_count
            .saturating_add(other.visible_message_count);
        self.user_message_count = self
            .user_message_count
            .saturating_add(other.user_message_count);
        self.assistant_message_count = self
            .assistant_message_count
            .saturating_add(other.assistant_message_count);
        self.estimated_tokens = self.estimated_tokens.saturating_add(other.estimated_tokens);
        if self.first_user_prompt.is_none() {
            self.first_user_prompt = other.first_user_prompt;
        }
        push_sampled_search_text(
            &mut self.search_text,
            &other.search_text,
            INITIAL_TRANSCRIPT_SEARCH_BUDGET_BYTES,
        );
    }
}

impl<'de> Deserialize<'de> for SessionMessageSummaryData {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SessionMessageSummaryDataVisitor)
    }
}

struct SessionMessageSummaryDataVisitor;

impl<'de> Visitor<'de> for SessionMessageSummaryDataVisitor {
    type Value = SessionMessageSummaryData;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("session message summary array")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut counts = SessionMessageSummaryData::default();
        loop {
            let Some(message) = seq.next_element_seed(SessionMessageSummarySeed {
                // Keep sampling each turn even after the session-wide index is
                // full. `add_message` retains a bounded head + moving tail.
                content_excerpt_budget: MESSAGE_SEARCH_EXCERPT_BYTES,
            })?
            else {
                break;
            };
            counts.add_message(&message);
        }
        Ok(counts)
    }
}

struct SessionMessageSummarySeed {
    content_excerpt_budget: usize,
}

impl<'de> DeserializeSeed<'de> for SessionMessageSummarySeed {
    type Value = SessionMessageSummary;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(SessionMessageSummaryVisitor {
            content_excerpt_budget: self.content_excerpt_budget,
        })
    }
}

struct SessionMessageSummary {
    role: Role,
    // `/resume` only needs role/display/token metadata for the initial list.
    // Borrowing content as `RawValue` lets serde skip nested content without
    // allocating or walking it. After display metadata is known, we inspect only
    // the raw prefix for old snapshots that predate `display_role`.
    content_starts_with_system_reminder: bool,
    content_raw: Option<String>,
    content_text: Option<String>,
    display_role: Option<StoredDisplayRole>,
    token_usage: Option<SessionTokenUsageSummary>,
}

impl<'de> Deserialize<'de> for SessionMessageSummary {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(SessionMessageSummaryVisitor {
            content_excerpt_budget: MESSAGE_SEARCH_EXCERPT_BYTES,
        })
    }
}

struct SessionMessageSummaryVisitor {
    content_excerpt_budget: usize,
}

impl<'de> Visitor<'de> for SessionMessageSummaryVisitor {
    type Value = SessionMessageSummary;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("session message summary")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut role: Option<Role> = None;
        let mut content: Option<&'de RawValue> = None;
        let mut display_role: Option<StoredDisplayRole> = None;
        let mut token_usage: Option<SessionTokenUsageSummary> = None;

        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            match key.as_ref() {
                "role" => {
                    role = Some(map.next_value()?);
                }
                "content" => {
                    content = Some(map.next_value()?);
                }
                "display_role" => {
                    display_role = map.next_value()?;
                }
                "token_usage" => {
                    token_usage = map.next_value()?;
                }
                _ => {
                    let _ = map.next_value::<IgnoredAny>()?;
                }
            }
        }

        let role = role.ok_or_else(|| serde::de::Error::missing_field("role"))?;
        let content_starts_with_system_reminder = matches!(role, Role::User)
            && display_role.is_none()
            && content.is_some_and(raw_content_starts_with_system_reminder);
        let content_raw = if display_role.is_none() && !content_starts_with_system_reminder {
            content.and_then(|raw| raw_value_search_excerpt(raw, self.content_excerpt_budget))
        } else {
            None
        };
        let content_text = if display_role.is_none() && !content_starts_with_system_reminder {
            content.and_then(raw_value_display_text)
        } else {
            None
        };
        Ok(SessionMessageSummary {
            role,
            content_starts_with_system_reminder,
            content_raw,
            content_text,
            display_role,
            token_usage,
        })
    }
}

fn summary_message_is_visible_conversation(message: &SessionMessageSummary) -> bool {
    if message.display_role.is_some() {
        return false;
    }
    if message.content_starts_with_system_reminder {
        return false;
    }
    true
}

fn raw_content_starts_with_system_reminder(raw: &RawValue) -> bool {
    let raw = raw.get().trim_start();
    json_string_raw_starts_with_system_reminder(raw)
        || first_text_field_raw_starts_with_system_reminder(raw)
}

fn json_string_raw_starts_with_system_reminder(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix('"') else {
        return false;
    };

    rest.trim_start().starts_with("<system-reminder>")
}

fn first_text_field_raw_starts_with_system_reminder(raw: &str) -> bool {
    const RAW_SYSTEM_REMINDER_SEARCH_BYTES: usize = 2048;
    let mut end = raw.len().min(RAW_SYSTEM_REMINDER_SEARCH_BYTES);
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    let haystack = &raw[..end];
    let mut search_start = 0;
    while let Some(relative_key_idx) = haystack[search_start..].find("\"text\"") {
        let key_idx = search_start + relative_key_idx;
        let previous = haystack[..key_idx]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace());
        let after_key = haystack[key_idx + "\"text\"".len()..].trim_start();
        if matches!(previous, Some('{') | Some(','))
            && let Some(after_colon) = after_key.strip_prefix(':')
        {
            return json_string_raw_starts_with_system_reminder(after_colon.trim_start());
        }

        search_start = key_idx + "\"text\"".len();
    }

    false
}

#[derive(Deserialize)]
struct SessionTokenUsageSummary {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

impl SessionTokenUsageSummary {
    fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_read_input_tokens.unwrap_or(0)
            + self.cache_creation_input_tokens.unwrap_or(0)
    }
}

#[derive(Deserialize)]
struct SessionJournalSummaryMeta {
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    short_name: Option<String>,
    #[serde(default)]
    provider_key: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    is_canary: bool,
    #[serde(default)]
    is_debug: bool,
    #[serde(default)]
    saved: Option<bool>,
    #[serde(default)]
    save_label: Option<String>,
    #[serde(default)]
    status: SessionStatus,
    #[serde(default)]
    last_active_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
struct SessionJournalSummaryEntry {
    meta: SessionJournalSummaryMeta,
    #[serde(default)]
    append_messages: SessionMessageSummaryData,
}

fn load_session_summary(path: &Path) -> Result<SessionSummary> {
    let bytes = std::fs::read(path)?;
    let mut summary: SessionSummary = serde_json::from_slice(&bytes)?;

    let journal_path = session::session_journal_path_from_snapshot(path);
    if journal_path.exists() {
        let file = File::open(&journal_path)?;
        let reader = BufReader::new(file);
        for (line_idx, line) in reader.lines().enumerate() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<SessionJournalSummaryEntry>(trimmed) {
                Ok(entry) => {
                    summary.parent_id = entry.meta.parent_id;
                    summary.title = entry.meta.title;
                    summary.updated_at = entry.meta.updated_at;
                    summary.last_active_at = entry.meta.last_active_at;
                    summary.working_dir = entry.meta.working_dir;
                    summary.short_name = entry.meta.short_name;
                    summary.provider_key = entry.meta.provider_key;
                    summary.model = entry.meta.model;
                    summary.is_canary = entry.meta.is_canary;
                    summary.is_debug = entry.meta.is_debug;
                    if let Some(saved) = entry.meta.saved {
                        summary.saved = saved;
                    }
                    summary.save_label = entry.meta.save_label;
                    summary.status = entry.meta.status;
                    summary.messages.merge(entry.append_messages);
                }
                Err(err) => {
                    crate::logging::warn(&format!(
                        "Session picker journal parse failed at {} line {}: {}",
                        journal_path.display(),
                        line_idx + 1,
                        err
                    ));
                    break;
                }
            }
        }
    }

    Ok(summary)
}

pub(super) fn build_messages_preview(session: &Session) -> Vec<PreviewMessage> {
    session::render_messages(session)
        .into_iter()
        .rev()
        .take(20)
        .rev()
        .map(|msg| PreviewMessage {
            role: msg.role,
            content: msg.content,
            tool_calls: msg.tool_calls,
            tool_data: msg.tool_data,
            timestamp: None,
        })
        .collect()
}

pub(super) fn crashed_sessions_from_all_sessions(
    sessions: &[SessionInfo],
) -> Option<CrashedSessionsInfo> {
    let recovered_parents: HashSet<&str> = sessions
        .iter()
        .filter(|s| s.id.starts_with("session_recovery_"))
        .filter_map(|s| s.parent_id.as_deref())
        .collect();

    let mut crashed: Vec<&SessionInfo> = sessions
        .iter()
        .filter(|s| matches!(s.status, SessionStatus::Crashed { .. }))
        .filter(|s| !recovered_parents.contains(s.id.as_str()))
        .collect();
    if crashed.is_empty() {
        return None;
    }

    let total_unrecovered_crashed = crashed.len();
    let crash_timestamp =
        |session: &SessionInfo| session.last_active_at.unwrap_or(session.last_message_time);
    let most_recent = crashed
        .iter()
        .map(|session| crash_timestamp(session))
        .max()?;
    let crash_window = chrono::Duration::seconds(60);
    crashed.retain(|s| {
        let delta = most_recent.signed_duration_since(crash_timestamp(s));
        delta >= chrono::Duration::zero() && delta <= crash_window
    });
    if crashed.is_empty() {
        return None;
    }

    crashed.sort_by(|a, b| b.last_message_time.cmp(&a.last_message_time));

    Some(CrashedSessionsInfo {
        session_ids: crashed.iter().map(|s| s.id.clone()).collect(),
        display_names: crashed.iter().map(|s| s.short_name.clone()).collect(),
        most_recent_crash: most_recent,
        omitted_crashed_count: total_unrecovered_crashed.saturating_sub(crashed.len()),
    })
}

/// Parse a single kcode session snapshot (+ journal) into a [`SessionInfo`],
/// returning `None` for empty/imported sessions or read/parse errors. Pulled out
/// of `load_sessions` so the summary pass can run across a scoped thread pool.
fn parse_kcode_session_info(
    sessions_dir: &Path,
    stem: &str,
    catchup_seen: &crate::catchup::CatchupSeenSnapshot,
) -> Option<SessionInfo> {
    let path = sessions_dir.join(format!("{stem}.json"));
    let session = load_session_summary(&path).ok()?;

    let visible_message_count = session.messages.visible_message_count;
    if visible_message_count == 0 {
        return None;
    }

    let short_name = session
        .short_name
        .clone()
        .or_else(|| extract_session_name(stem).map(|s| s.to_string()))
        .unwrap_or_else(|| stem.to_string());
    let icon = session_icon(&short_name);

    let user_message_count = session.messages.user_message_count;
    let assistant_message_count = session.messages.assistant_message_count;
    let estimated_tokens = session.messages.estimated_tokens;

    let status = session.status.clone();
    let needs_catchup = catchup_seen.needs_catchup(stem, session.updated_at, &status);

    let title = short_name.clone();
    let search_index = build_search_index_from_summary(
        stem,
        &short_name,
        &title,
        session.working_dir.as_deref(),
        session.save_label.as_deref(),
        &session.messages.search_text,
    );

    Some(SessionInfo {
        id: stem.to_string(),
        parent_id: session.parent_id,
        short_name,
        icon: icon.to_string(),
        title,
        message_count: visible_message_count,
        user_message_count,
        assistant_message_count,
        created_at: session.created_at,
        last_message_time: session.updated_at,
        last_active_at: session.last_active_at,
        working_dir: session.working_dir,
        model: session.model,
        provider_key: session.provider_key,
        is_canary: session.is_canary,
        is_debug: session.is_debug,
        saved: session.saved,
        save_label: session.save_label,
        status,
        needs_catchup,
        estimated_tokens,
        first_user_prompt: session.messages.first_user_prompt,
        messages_preview: Vec::new(),
        search_index,
        server_name: None,
        server_icon: None,
    })
}

pub fn load_sessions() -> Result<Vec<SessionInfo>> {
    let sessions_dir = storage::kcode_dir()?.join("sessions");
    let scan_limit = session_scan_limit();

    if let Ok(cache) = session_list_cache().lock()
        && let Some(entry) = cache.as_ref()
        && entry.sessions_dir == sessions_dir
        && entry.scan_limit == scan_limit
        && entry.loaded_at.elapsed() <= SESSION_LIST_CACHE_TTL
    {
        return Ok(entry.sessions.clone());
    }

    let candidates = if sessions_dir.exists() {
        // Keep startup responsive by avoiding `session_has_history` here. That helper parses
        // snapshots/journals, and `load_session_summary` below parses the same files again.
        // Instead, gather a recency-ordered candidate window cheaply from metadata and let the
        // single summary pass filter empty sessions while filling up to `scan_limit` entries.
        let mut candidates =
            collect_recent_session_candidates(&sessions_dir, session_candidate_window(scan_limit))?;
        if include_old_saved_sessions_on_initial_load() {
            let mut seen: HashSet<String> = candidates.iter().cloned().collect();
            for stem in collect_saved_session_candidates(&sessions_dir)? {
                if seen.insert(stem.clone()) {
                    candidates.push(stem);
                }
            }
        }
        candidates
    } else {
        Vec::new()
    };

    // Loading the catch-up "seen" state once (instead of per session) avoids
    // re-reading and re-parsing `catchup_seen.json` for every candidate.
    let catchup_seen = crate::catchup::CatchupSeenSnapshot::load();
    let sessions_dir_ref = &sessions_dir;
    let catchup_ref = &catchup_seen;

    let mut sessions: Vec<SessionInfo> = {
        // Phase 1: walk the recency-ordered candidates in parallel windows until
        // we have collected `scan_limit` non-empty sessions. `boundary` marks the
        // candidate index where the serial fill would start applying the saved
        // gate, so beyond it we only keep saved sessions (Phase 2). Parsing each
        // window in parallel keeps the per-file JSON cost off the critical path.
        //
        // Windows are sized to `scan_limit`: only the final window (the one that
        // crosses `scan_limit`) can over-parse, so wasted work is bounded to a
        // single window's worth of candidates while still parallelizing widely.
        let mut sessions: Vec<SessionInfo> = Vec::new();
        // Debug/canary sessions are hidden in the default picker view. Do not let a
        // burst of self-dev or swarm workers consume the entire recency budget and
        // crowd out ordinary sessions. Keep a separate bounded debug budget so the
        // test-session toggle still has useful recent entries.
        let mut visible_session_count = 0usize;
        let mut debug_session_count = 0usize;
        let mut boundary = candidates.len();
        let window = scan_limit.max(1);
        let mut start = 0;
        'fill: while start < candidates.len() {
            let end = (start + window).min(candidates.len());
            let batch = candidates[start..end].to_vec();
            let parsed = parallel_map(batch, move |stem| {
                parse_kcode_session_info(sessions_dir_ref, &stem, catchup_ref)
            });
            for (offset, parsed_session) in parsed.into_iter().enumerate() {
                if let Some(info) = parsed_session {
                    if info.is_debug {
                        if debug_session_count < scan_limit {
                            debug_session_count += 1;
                            sessions.push(info);
                        }
                    } else {
                        visible_session_count += 1;
                        sessions.push(info);
                    }
                    if visible_session_count >= scan_limit {
                        boundary = start + offset + 1;
                        break 'fill;
                    }
                }
            }
            start = end;
        }

        // Phase 2: beyond the fill boundary the serial loader only keeps saved
        // sessions. Compute the cheap saved tail-gate across the remaining
        // candidates in parallel, then fully parse just the gate-passers.
        if boundary < candidates.len() {
            let tail: Vec<String> = candidates[boundary..].to_vec();
            let gate_passers: Vec<String> = parallel_map(tail, move |stem| {
                let path = sessions_dir_ref.join(format!("{stem}.json"));
                session_snapshot_or_journal_has_saved_metadata(&path).then_some(stem)
            })
            .into_iter()
            .flatten()
            .collect();
            let saved_sessions = parallel_map(gate_passers, move |stem| {
                parse_kcode_session_info(sessions_dir_ref, &stem, catchup_ref)
            });
            sessions.extend(saved_sessions.into_iter().flatten());
        }

        sessions
    };

    sessions.sort_by(|a, b| b.last_message_time.cmp(&a.last_message_time));

    if let Ok(mut cache) = session_list_cache().lock() {
        *cache = Some(SessionListCacheEntry {
            loaded_at: Instant::now(),
            sessions_dir,
            scan_limit,
            sessions: sessions.clone(),
        });
    }

    Ok(sessions)
}

pub fn load_servers() -> Vec<ServerInfo> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| {
            handle.block_on(async { registry::list_servers().await.unwrap_or_default() })
        })
    } else {
        tokio::runtime::Runtime::new()
            .map(|rt| rt.block_on(async { registry::list_servers().await.unwrap_or_default() }))
            .unwrap_or_default()
    }
}

pub fn load_sessions_grouped() -> Result<(Vec<ServerGroup>, Vec<SessionInfo>)> {
    let sessions_dir = storage::kcode_dir()?.join("sessions");
    let scan_limit = session_scan_limit();
    let all_sessions = load_sessions()?;
    let servers = load_servers();

    let mut session_to_server: HashMap<String, &ServerInfo> = HashMap::new();
    for server in &servers {
        for session_name in &server.sessions {
            session_to_server.insert(session_name.clone(), server);
        }
    }

    let mut server_sessions: HashMap<String, Vec<SessionInfo>> = HashMap::new();
    let mut orphan_sessions: Vec<SessionInfo> = Vec::new();

    for mut session in all_sessions {
        if let Some(server) = session_to_server.get(&session.short_name) {
            session.server_name = Some(server.name.clone());
            session.server_icon = Some(server.icon.clone());
            server_sessions
                .entry(server.name.clone())
                .or_default()
                .push(session);
        } else {
            orphan_sessions.push(session);
        }
    }

    let mut groups: Vec<ServerGroup> = servers
        .iter()
        .map(|server| {
            let sessions = server_sessions.remove(&server.name).unwrap_or_default();
            ServerGroup {
                name: server.name.clone(),
                icon: server.icon.clone(),
                version: server.version.clone(),
                git_hash: server.git_hash.clone(),
                is_running: true,
                sessions,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        let a_latest = a.sessions.iter().map(|s| s.last_message_time).max();
        let b_latest = b.sessions.iter().map(|s| s.last_message_time).max();
        b_latest.cmp(&a_latest)
    });

    write_grouped_session_list_disk_cache(&sessions_dir, scan_limit, &groups, &orphan_sessions);

    Ok((groups, orphan_sessions))
}

#[cfg(test)]
#[path = "loading_tests.rs"]
mod tests;
