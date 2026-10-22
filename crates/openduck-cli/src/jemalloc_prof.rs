//! Jemalloc sampling profiles for binaries built with `jemalloc-prof`.

#[cfg(unix)]
use std::ffi::CString;

/// Default options. `MALLOC_CONF` replaces this whole string when it is set.
pub const MALLOC_CONF: &[u8] =
    b"background_thread:true,prof:true,prof_active:true,lg_prof_sample:19,prof_prefix:/tmp/od-heap\0";

#[cfg(unix)]
const DEFAULT_PREFIX: &str = "/tmp/od-heap";

pub fn install() {
    #[cfg(unix)]
    spawn_dump_listener();
    #[cfg(not(unix))]
    tracing::info!(conf = conf_text(), "jemalloc profiling is compiled in");
}

#[cfg(unix)]
fn spawn_dump_listener() {
    tokio::spawn(async {
        let mut signals =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined2()) {
                Ok(signals) => signals,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to listen for SIGUSR2");
                    return;
                }
            };
        tracing::info!(
            conf = conf_text(),
            "jemalloc profiling is active; SIGUSR2 writes a heap profile"
        );
        loop {
            if signals.recv().await.is_none() {
                break;
            }
            dump_profile();
        }
    });
}

#[cfg(unix)]
fn dump_profile() {
    let path = profile_path(&profile_prefix(), std::process::id(), unix_secs());
    let Ok(path_c) = CString::new(path.clone()) else {
        tracing::warn!(path = %path, "jemalloc profile path contains an interior nul");
        return;
    };
    // `prof.dump` writes a `const char *`. `path_c` stays alive for the call.
    let result = unsafe { tikv_jemalloc_ctl::raw::write(b"prof.dump\0", path_c.as_ptr()) };
    let (allocated, resident) = allocator_usage();
    match result {
        Ok(()) => tracing::info!(
            path = %path,
            allocated,
            resident,
            "wrote jemalloc heap profile"
        ),
        Err(err) => tracing::warn!(
            error = %err,
            path = %path,
            "jemalloc prof.dump failed"
        ),
    }
}

#[cfg(unix)]
fn allocator_usage() -> (Option<usize>, Option<usize>) {
    if tikv_jemalloc_ctl::epoch::advance().is_err() {
        return (None, None);
    }
    (
        tikv_jemalloc_ctl::stats::allocated::read().ok(),
        tikv_jemalloc_ctl::stats::resident::read().ok(),
    )
}

#[cfg(unix)]
fn profile_prefix() -> String {
    std::env::var("OPENDUCK_JEMALLOC_PROF_PREFIX").unwrap_or_else(|_| DEFAULT_PREFIX.to_string())
}

fn profile_path(prefix: &str, pid: u32, secs: u64) -> String {
    format!("{prefix}.{pid}.{secs}.heap")
}

#[cfg(unix)]
fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn conf_text() -> &'static str {
    let end = MALLOC_CONF
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(MALLOC_CONF.len());
    std::str::from_utf8(&MALLOC_CONF[..end]).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::{profile_path, MALLOC_CONF};

    #[test]
    fn builtin_malloc_conf_enables_sampling() {
        let text = std::str::from_utf8(MALLOC_CONF).unwrap();
        assert!(text.ends_with('\0'));
        assert!(text.contains("prof:true"));
        assert!(text.contains("prof_active:true"));
        assert!(text.contains("lg_prof_sample:19"));
        assert!(text.contains("background_thread:true"));
    }

    #[test]
    fn profile_names_include_pid_and_time() {
        assert_eq!(
            profile_path("/tmp/od-heap", 12, 99),
            "/tmp/od-heap.12.99.heap"
        );
    }
}
