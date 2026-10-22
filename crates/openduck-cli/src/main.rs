#![recursion_limit = "256"]

use anyhow::Result;
use openduck_cli::cli::cli;

// Name the sys crate so its malloc override is linked into this binary.
#[cfg(all(feature = "jemalloc", not(target_env = "msvc")))]
#[allow(unused_imports)]
use tikv_jemalloc_sys as _;

#[cfg(all(feature = "jemalloc", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// jemalloc reads this symbol when MALLOC_CONF is unset.
// Setting MALLOC_CONF replaces the string; it does not merge with it.
#[cfg(all(feature = "jemalloc-prof", not(target_env = "msvc")))]
#[allow(non_upper_case_globals)]
#[export_name = "malloc_conf"]
#[used]
static malloc_conf: &[u8] = openduck_cli::jemalloc_prof::MALLOC_CONF;

/// Enable ANSI/VT escape sequence processing on Windows Console Host.
///
/// Without this, spinners and progress bars from cliclack/indicatif render as
/// repeated new lines instead of updating in place, because Windows Console Host
/// does not process ANSI escapes by default.
#[cfg(windows)]
fn enable_windows_vt_processing() {
    // colors_supported() has the side effect of calling SetConsoleMode with
    // ENABLE_VIRTUAL_TERMINAL_PROCESSING on the underlying console handle.
    let _ = console::Term::stdout().features().colors_supported();
    let _ = console::Term::stderr().features().colors_supported();
}

async fn run() -> Result<()> {
    let result = cli().await;

    #[cfg(feature = "otel")]
    if openduck::otel::otlp::is_otlp_initialized() {
        openduck::otel::otlp::shutdown_otlp();
    }

    result
}

fn main() -> Result<()> {
    #[cfg(windows)]
    enable_windows_vt_processing();

    let handle = std::thread::Builder::new()
        .name("openduck-cli-main".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime");
            runtime.block_on(run())
        })
        .map_err(|e| anyhow::anyhow!("Failed to spawn openduck-cli main thread: {}", e))?;

    handle
        .join()
        .map_err(|_| anyhow::anyhow!("openduck-cli main thread panicked"))?
}
