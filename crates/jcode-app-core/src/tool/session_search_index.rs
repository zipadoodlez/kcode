//! Incremental hashed-token index used by `session_search` to pre-filter
//! candidate files before any expensive parsing.
//!
//! One index is kept per session store (jcode, codex, claude, ...). Each entry
//! records the file's identity (key, mtime, size) plus the set of FNV-1a
//! hashes of its search tokens. On rebuild, entries whose identity is
//! unchanged are reused as-is, so only new or modified files are re-read and
//! re-tokenized. Hash collisions can only produce false-positive candidates,
//! which downstream scoring re-verifies against the real file contents.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Tokens longer than this are dropped from the index (they are almost always
/// hashes or base64 blobs). Query terms longer than this match every entry so
/// recall is preserved.
pub const MAX_INDEX_TOKEN_LEN: usize = 32;
/// Entries with more unique tokens than this are marked overflow and always
/// treated as candidates.
pub const MAX_TOKENS_PER_FILE: usize = 200_000;
const MAGIC: &[u8; 4] = b"JSIX";
const VERSION: u32 = 1;
const INDEX_THREADS: usize = 8;

static INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<TokenHashIndex>>>> = OnceLock::new();

/// Identity of one indexable file (or file group) inside a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexFileSpec {
    /// Stable key, usually the file path or session id.
    pub key: String,
    pub mtime_ms: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    key: String,
    mtime_ms: u64,
    size: u64,
    overflow: bool,
    /// Sorted, deduplicated token hashes.
    tokens: Arc<[u32]>,
}

#[derive(Debug, Default)]
pub struct TokenHashIndex {
    entries: Vec<IndexEntry>,
}

/// FNV-1a 32-bit hash over the token bytes.
pub fn hash_token(token: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in token.as_bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Tokenize searchable text into sorted unique hashes. Returns `(hashes,
/// overflowed)`.
pub fn hash_tokens_for_text(text: &str) -> (Vec<u32>, bool) {
    let tokens = jcode_session_types::tokenize_session_search_query(&text.to_lowercase());
    let mut hashes: Vec<u32> = tokens
        .iter()
        .filter(|token| token.len() <= MAX_INDEX_TOKEN_LEN)
        .map(|token| hash_token(token))
        .collect();
    hashes.sort_unstable();
    hashes.dedup();
    let overflow = hashes.len() > MAX_TOKENS_PER_FILE;
    if overflow {
        hashes.truncate(MAX_TOKENS_PER_FILE);
    }
    (hashes, overflow)
}

impl TokenHashIndex {
    /// Return spec indices that plausibly match `terms` under the
    /// `min_term_matches` threshold. Overflowed and unreadable entries are
    /// always candidates so recall never regresses.
    pub fn candidate_slots(&self, terms: &[String], min_term_matches: usize) -> Vec<usize> {
        let term_hashes: Vec<Option<u32>> = terms
            .iter()
            .map(|term| (term.len() <= MAX_INDEX_TOKEN_LEN).then(|| hash_token(term)))
            .collect();

        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if entry.overflow {
                    return true;
                }
                let matched = term_hashes
                    .iter()
                    .filter(|hash| match hash {
                        // Query term too long to be indexed: assume present.
                        None => true,
                        Some(hash) => entry.tokens.binary_search(hash).is_ok(),
                    })
                    .count();
                matched >= min_term_matches.max(1)
            })
            .map(|(slot, _)| slot)
            .collect()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn matches_specs(&self, specs: &[IndexFileSpec]) -> bool {
        self.entries.len() == specs.len()
            && self.entries.iter().zip(specs).all(|(entry, spec)| {
                entry.key == spec.key && entry.mtime_ms == spec.mtime_ms && entry.size == spec.size
            })
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::with_capacity(64 + self.entries.len() * 48);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            let key_bytes = entry.key.as_bytes();
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&entry.mtime_ms.to_le_bytes());
            buf.extend_from_slice(&entry.size.to_le_bytes());
            buf.push(u8::from(entry.overflow));
            buf.extend_from_slice(&(entry.tokens.len() as u32).to_le_bytes());
        }
        for entry in &self.entries {
            for hash in entry.tokens.iter() {
                buf.extend_from_slice(&hash.to_le_bytes());
            }
        }
        let tmp = path.with_extension("bin.tmp");
        std::fs::write(&tmp, &buf)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read(path)?;
        let mut cursor = Cursor::new(&raw);
        if cursor.take(4)? != MAGIC {
            bail!("bad magic");
        }
        if cursor.read_u32()? != VERSION {
            bail!("version mismatch");
        }
        let entry_count = cursor.read_u32()? as usize;
        let mut metas = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let key_len = cursor.read_u16()? as usize;
            let key = std::str::from_utf8(cursor.take(key_len)?)
                .context("invalid key")?
                .to_string();
            let mtime_ms = cursor.read_u64()?;
            let size = cursor.read_u64()?;
            let overflow = cursor.take(1)?[0] != 0;
            let token_count = cursor.read_u32()? as usize;
            metas.push((key, mtime_ms, size, overflow, token_count));
        }
        let mut entries = Vec::with_capacity(entry_count);
        for (key, mtime_ms, size, overflow, token_count) in metas {
            let bytes = cursor.take(token_count * 4)?;
            let tokens: Vec<u32> = bytes
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            entries.push(IndexEntry {
                key,
                mtime_ms,
                size,
                overflow,
                tokens: tokens.into(),
            });
        }
        Ok(Self { entries })
    }
}

struct Cursor<'a> {
    raw: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(raw: &'a [u8]) -> Self {
        Self { raw, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.raw.len())
            .context("truncated index file")?;
        let slice = &self.raw[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }
}

/// Load, incrementally update, and persist the index at `index_path` so it
/// exactly covers `specs` (in order). `read_text` returns the searchable text
/// for one spec slot (content plus any metadata like the path); it must be
/// callable from multiple threads. Returning `None` marks the entry overflow
/// (always-candidate) so unreadable files are never silently dropped.
pub fn build_or_update(
    index_path: &Path,
    specs: &[IndexFileSpec],
    read_text: &(dyn Fn(usize) -> Option<String> + Sync),
) -> Result<Arc<TokenHashIndex>> {
    let cache = INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock()
        && let Some(index) = guard.get(index_path)
        && index.matches_specs(specs)
    {
        return Ok(Arc::clone(index));
    }

    let previous = TokenHashIndex::load(index_path).unwrap_or_default();
    let mut previous_by_key: HashMap<&str, &IndexEntry> = HashMap::new();
    for entry in &previous.entries {
        previous_by_key.insert(entry.key.as_str(), entry);
    }

    // Decide which slots can reuse existing token sets.
    let mut reused: Vec<Option<IndexEntry>> = Vec::with_capacity(specs.len());
    let mut stale_slots = Vec::new();
    for (slot, spec) in specs.iter().enumerate() {
        match previous_by_key.get(spec.key.as_str()) {
            Some(entry) if entry.mtime_ms == spec.mtime_ms && entry.size == spec.size => {
                reused.push(Some((*entry).clone()));
            }
            _ => {
                reused.push(None);
                stale_slots.push(slot);
            }
        }
    }

    let rebuilt = tokenize_slots_parallel(specs, &stale_slots, read_text);
    let mut entries = Vec::with_capacity(specs.len());
    let mut rebuilt_iter = rebuilt.into_iter();
    for reusable in reused {
        match reusable {
            Some(entry) => entries.push(entry),
            None => entries.push(rebuilt_iter.next().expect("rebuilt entry per stale slot")),
        }
    }

    let index = TokenHashIndex { entries };
    if !stale_slots.is_empty() || previous.entries.len() != specs.len() {
        if let Err(err) = index.save(index_path) {
            crate::logging::warn(&format!(
                "session_search index save failed for {}: {err}",
                index_path.display()
            ));
        }
    }
    let index = Arc::new(index);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(index_path.to_path_buf(), Arc::clone(&index));
    }
    Ok(index)
}

fn tokenize_slots_parallel(
    specs: &[IndexFileSpec],
    stale_slots: &[usize],
    read_text: &(dyn Fn(usize) -> Option<String> + Sync),
) -> Vec<IndexEntry> {
    if stale_slots.is_empty() {
        return Vec::new();
    }
    let thread_count = INDEX_THREADS.min(stale_slots.len());
    let chunk_size = stale_slots.len().div_ceil(thread_count);

    let chunks: Vec<Vec<IndexEntry>> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for chunk in stale_slots.chunks(chunk_size) {
            handles.push(scope.spawn(move || {
                chunk
                    .iter()
                    .map(|&slot| {
                        let spec = &specs[slot];
                        let (tokens, overflow) = match read_text(slot) {
                            Some(text) => hash_tokens_for_text(&text),
                            // Unreadable now: keep it as always-candidate so
                            // downstream verification decides.
                            None => (Vec::new(), true),
                        };
                        IndexEntry {
                            key: spec.key.clone(),
                            mtime_ms: spec.mtime_ms,
                            size: spec.size,
                            overflow,
                            tokens: tokens.into(),
                        }
                    })
                    .collect()
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect()
    });

    // Rebuild order must match stale_slots order: chunks preserve it.
    chunks.into_iter().flatten().collect()
}

/// Stat helper returning `(mtime_ms, size)`, or zeros when unavailable.
pub fn stat_ms_size(path: &Path) -> (u64, u64) {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            (mtime_ms, meta.len())
        }
        Err(_) => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: &str, mtime_ms: u64, size: u64) -> IndexFileSpec {
        IndexFileSpec {
            key: key.to_string(),
            mtime_ms,
            size,
        }
    }

    #[test]
    fn candidates_respect_min_term_matches_and_wildcards() {
        let texts = ["alpha beta gamma", "alpha delta", "unrelated words"];
        let specs: Vec<IndexFileSpec> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| spec(&format!("file-{i}"), 1, text.len() as u64))
            .collect();
        let temp = tempfile::TempDir::new().expect("temp dir");
        let index_path = temp.path().join("index.bin");
        let index = build_or_update(&index_path, &specs, &|slot| Some(texts[slot].to_string()))
            .expect("build index");

        let terms = vec!["alpha".to_string(), "beta".to_string()];
        assert_eq!(index.candidate_slots(&terms, 2), vec![0]);
        assert_eq!(index.candidate_slots(&terms, 1), vec![0, 1]);

        // Terms longer than the index cap match everything.
        let long_term = vec!["x".repeat(MAX_INDEX_TOKEN_LEN + 1)];
        assert_eq!(index.candidate_slots(&long_term, 1), vec![0, 1, 2]);
    }

    #[test]
    fn incremental_update_reuses_unchanged_entries_and_persists() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let index_path = temp.path().join("index.bin");

        let specs = vec![spec("a", 1, 10), spec("b", 1, 10)];
        let texts = std::sync::Mutex::new(vec!["needle one", "other two"]);
        let reads = std::sync::atomic::AtomicUsize::new(0);
        let read = |slot: usize| {
            reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(texts.lock().unwrap()[slot].to_string())
        };

        let index = build_or_update(&index_path, &specs, &read).expect("build");
        assert_eq!(index.len(), 2);
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(
            index.candidate_slots(&vec!["needle".to_string()], 1),
            vec![0]
        );

        // Change only file b; a must not be re-read.
        texts.lock().unwrap()[1] = "needle three";
        let specs = vec![spec("a", 1, 10), spec("b", 2, 12)];
        let index = build_or_update(&index_path, &specs, &read).expect("update");
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(
            index.candidate_slots(&vec!["needle".to_string()], 1),
            vec![0, 1]
        );

        // Round-trip through disk (bypass in-memory cache with a fresh load).
        let loaded = TokenHashIndex::load(&index_path).expect("load");
        assert!(loaded.matches_specs(&specs));
        assert_eq!(
            loaded.candidate_slots(&vec!["needle".to_string()], 1),
            vec![0, 1]
        );
    }

    #[test]
    fn unreadable_files_stay_candidates() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let index_path = temp.path().join("index.bin");
        let specs = vec![spec("gone", 5, 5)];
        let index = build_or_update(&index_path, &specs, &|_| None).expect("build");
        assert_eq!(
            index.candidate_slots(&vec!["anything".to_string()], 1),
            vec![0]
        );
    }
}
