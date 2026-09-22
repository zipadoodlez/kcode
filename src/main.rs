use anyhow::Result;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn configure_system_allocator() {
    unsafe extern "C" {
        fn mallopt(param: i32, value: i32) -> i32;
    }

    const M_ARENA_MAX: i32 = -8;
    const M_MMAP_THRESHOLD: i32 = -3;

    let arena_max = parse_alloc_tuning_env("JCODE_GLIBC_ARENA_MAX", 4);
    let _ = unsafe { mallopt(M_ARENA_MAX, arena_max) };

    // Pin the mmap threshold so large transient allocations (history JSON,
    // provider payloads) are served by mmap and returned to the OS
    // immediately on free, instead of landing in sbrk arenas where freed
    // blocks below the top chunk become permanent RSS retention.
    //
    // Tradeoff: setting M_MMAP_THRESHOLD via mallopt disables glibc's
    // dynamic threshold growth (normally the threshold rises toward 32 MiB
    // as large blocks are freed, keeping hot large buffers in the arena for
    // cheap reuse). Pinning trades some throughput on repeated large
    // alloc/free cycles (mmap/munmap syscalls + page faults each time) for
    // predictable, immediate memory return. For a long-running interactive
    // agent, lower steady-state RSS wins.
    let mmap_threshold = parse_alloc_tuning_env("JCODE_GLIBC_MMAP_THRESHOLD", 256 * 1024);
    let _ = unsafe { mallopt(M_MMAP_THRESHOLD, mmap_threshold) };
}

/// Parse a positive i32 allocator tuning knob from an env var, falling back
/// to `default` when unset, unparsable, or non-positive.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn parse_alloc_tuning_env(var: &str, default: i32) -> i32 {
    parse_alloc_tuning(std::env::var(var).ok().as_deref(), default)
}

/// Pure parsing core of [`parse_alloc_tuning_env`], separated for unit tests.
#[cfg(any(test, all(target_os = "linux", target_env = "gnu")))]
fn parse_alloc_tuning(value: Option<&str>, default: i32) -> i32 {
    value
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn configure_system_allocator() {}

fn main() -> Result<()> {
    run_main()
}

fn run_main() -> Result<()> {
    configure_system_allocator();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async { jcode::run().await })
}

#[cfg(test)]
mod tests {
    use super::parse_alloc_tuning;

    #[test]
    fn alloc_tuning_uses_default_when_unset() {
        assert_eq!(parse_alloc_tuning(None, 262_144), 262_144);
    }

    #[test]
    fn alloc_tuning_parses_positive_value_with_whitespace() {
        assert_eq!(parse_alloc_tuning(Some(" 131072 "), 262_144), 131_072);
    }

    #[test]
    fn alloc_tuning_rejects_garbage_zero_and_negative() {
        assert_eq!(parse_alloc_tuning(Some("not-a-number"), 4), 4);
        assert_eq!(parse_alloc_tuning(Some("0"), 4), 4);
        assert_eq!(parse_alloc_tuning(Some("-1"), 4), 4);
        // i32 overflow falls back to default rather than wrapping.
        assert_eq!(parse_alloc_tuning(Some("4294967296"), 4), 4);
    }
}
