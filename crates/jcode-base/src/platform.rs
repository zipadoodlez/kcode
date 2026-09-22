use std::path::Path;

#[cfg(target_os = "macos")]
mod macos_power {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_void};

    type CFStringRef = *const c_void;
    type IOPMAssertionID = u32;
    type IOReturn = i32;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;
    const K_IO_RETURN_SUCCESS: IOReturn = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            assertion_level: u32,
            assertion_name: CFStringRef,
            assertion_id: *mut IOPMAssertionID,
        ) -> IOReturn;
        fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
    }

    fn cf_string(value: &str) -> Option<CFStringRef> {
        let c_string = CString::new(value).ok()?;
        let cf = unsafe {
            CFStringCreateWithCString(
                std::ptr::null(),
                c_string.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        (!cf.is_null()).then_some(cf)
    }

    pub struct PowerAssertion {
        id: Option<IOPMAssertionID>,
    }

    impl PowerAssertion {
        pub fn prevent_user_idle_system_sleep(reason: &str) -> Self {
            let Some(assertion_type) = cf_string("PreventUserIdleSystemSleep") else {
                return Self { id: None };
            };
            let Some(assertion_name) = cf_string(reason) else {
                unsafe { CFRelease(assertion_type) };
                return Self { id: None };
            };

            let mut id = 0;
            let result = unsafe {
                IOPMAssertionCreateWithName(
                    assertion_type,
                    K_IOPM_ASSERTION_LEVEL_ON,
                    assertion_name,
                    &mut id,
                )
            };
            unsafe {
                CFRelease(assertion_type);
                CFRelease(assertion_name);
            }

            if result == K_IO_RETURN_SUCCESS {
                crate::logging::info(&format!(
                    "Created macOS sleep-prevention assertion while streaming (id={id})"
                ));
                Self { id: Some(id) }
            } else {
                crate::logging::warn(&format!(
                    "Failed to create macOS sleep-prevention assertion while streaming: IOReturn={result}"
                ));
                Self { id: None }
            }
        }

        #[cfg(test)]
        pub fn is_active(&self) -> bool {
            self.id.is_some()
        }
    }

    impl Drop for PowerAssertion {
        fn drop(&mut self) {
            if let Some(id) = self.id.take() {
                let result = unsafe { IOPMAssertionRelease(id) };
                if result != K_IO_RETURN_SUCCESS {
                    crate::logging::warn(&format!(
                        "Failed to release macOS sleep-prevention assertion id={id}: IOReturn={result}"
                    ));
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_power {
    pub struct PowerAssertion;

    impl PowerAssertion {
        pub fn prevent_user_idle_system_sleep(_reason: &str) -> Self {
            Self
        }

        #[cfg(test)]
        pub fn is_active(&self) -> bool {
            false
        }
    }
}

pub use macos_power::PowerAssertion;

#[cfg(any(unix, test))]
fn desired_nofile_soft_limit(current: u64, hard: u64, minimum: u64) -> Option<u64> {
    let desired = current.max(minimum).min(hard);
    (desired > current).then_some(desired)
}

/// Create a symlink at `dst` pointing to `src` (Unix `symlink(2)`).
pub fn symlink_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    { std::os::unix::fs::symlink(src, dst) }
}

pub use jcode_core::fs::{set_directory_permissions_owner_only, set_permissions_owner_only};

/// Set file permissions to owner read/write/execute (0o755).
pub fn set_permissions_executable(path: &Path) -> std::io::Result<()> {
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path, perms)
    }
}

/// Best-effort increase of the current process soft `RLIMIT_NOFILE` on Unix.
///
/// This helps jcode survive short-lived reload/connect spikes even when it was
/// launched from a shell with a conservative `ulimit -n` like 1024.
pub fn raise_nofile_limit_best_effort(minimum_soft_limit: u64) {
    {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
            crate::logging::warn(&format!(
                "Failed to read RLIMIT_NOFILE: {}",
                std::io::Error::last_os_error()
            ));
            return;
        }

        // `rlim_cur`/`rlim_max` are `u64` on Linux/macOS but `i64` on some
        // platforms (e.g. FreeBSD), so cast explicitly to keep builds portable.
        // The cast is a no-op (and clippy-flagged) where the field is already
        // `u64`, hence the allow.
        #[allow(clippy::unnecessary_cast)]
        let current: u64 = limit.rlim_cur as u64;
        #[allow(clippy::unnecessary_cast)]
        let hard: u64 = limit.rlim_max as u64;
        let Some(desired) = desired_nofile_soft_limit(current, hard, minimum_soft_limit) else {
            return;
        };

        let updated = libc::rlimit {
            rlim_cur: desired as libc::rlim_t,
            rlim_max: limit.rlim_max,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &updated) } == 0 {
            crate::logging::info(&format!(
                "Raised RLIMIT_NOFILE soft limit from {} to {} (hard={})",
                current, desired, hard
            ));
        } else {
            crate::logging::warn(&format!(
                "Failed to raise RLIMIT_NOFILE from {} toward {} (hard={}): {}",
                current,
                desired,
                hard,
                std::io::Error::last_os_error()
            ));
        }
    }
}

/// Check if a process is running by PID.
///
/// Uses `kill(pid, 0)` to check without sending a signal.
pub fn is_process_running(pid: u32) -> bool {
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        !matches!(err.raw_os_error(), Some(code) if code == libc::ESRCH)
    }
}

/// Send a signal to an entire detached process group/session led by `pid`.
///
/// Detached tasks are spawned with `setsid()`, so the leader PID is also the
/// process-group/session ID. Signaling `-pid` reaches the full tree.
pub fn signal_detached_process_group(pid: u32, signal: i32) -> std::io::Result<()> {
    {
        let rc = unsafe { libc::kill(-(pid as i32), signal) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

/// Best-effort non-blocking reap for a child process owned by the current process.
///
/// Returns:
/// - `Ok(Some(exit_code))` if the child exited and was reaped now
/// - `Ok(None)` if it is still running or is not our child
pub fn try_reap_child_process(pid: u32) -> std::io::Result<Option<i32>> {
    {
        let mut status = 0;
        let rc = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
        if rc == 0 {
            return Ok(None);
        }
        if rc == -1 {
            let err = std::io::Error::last_os_error();
            if matches!(err.raw_os_error(), Some(code) if code == libc::ECHILD) {
                return Ok(None);
            }
            return Err(err);
        }

        if libc::WIFEXITED(status) {
            Ok(Some(libc::WEXITSTATUS(status)))
        } else if libc::WIFSIGNALED(status) {
            Ok(Some(128 + libc::WTERMSIG(status)))
        } else {
            Ok(Some(-1))
        }
    }
}

/// Atomically swap a symlink by creating a temp symlink and renaming it over
/// the target.
pub fn atomic_symlink_swap(src: &Path, dst: &Path, temp: &Path) -> std::io::Result<()> {
    {
        let _ = std::fs::remove_file(temp);
        std::os::unix::fs::symlink(src, temp)?;
        std::fs::rename(temp, dst)?;
    }
    Ok(())
}

/// Spawn a process detached from the current client session.
///
/// This is used for launching new terminal windows (for `/resume`, `/split`,
/// crash restore, etc.) so the new client survives if the invoking jcode
/// process exits or its terminal closes.
pub fn spawn_detached(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    {
        use std::os::unix::process::CommandExt;

        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    cmd.spawn()
}

/// Reap a detached child without blocking the caller.
pub fn reap_detached(child: std::process::Child) {
    {
        let mut child = child;
        let _ = std::thread::Builder::new()
            .name("jcode-detached-child".to_string())
            .spawn(move || {
                let _ = child.wait();
            });
    }
}

/// Replace the current process with a new command via `exec()`.
///
/// Returns an error only if the operation fails. On success this function
/// never returns.
pub fn replace_process(cmd: &mut std::process::Command) -> std::io::Error {
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        crate::logging::error(&format!(
            "replace_process failed: {} ({})",
            err,
            crate::util::process_fd_diagnostic_snapshot()
        ));
        err
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod platform_tests;
