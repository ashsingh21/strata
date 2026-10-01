//! A log file and crash reports, kept on this computer - nothing is sent
//! anywhere. An installed copy is opened from Finder, where whatever the
//! app prints goes nowhere; these files are how anyone finds out what went
//! wrong.
//!
//! - `Logs/shor.log` in the user's data folder (see `paths`): everything
//!   the app prints (`eprintln!` all over the code), timestamped, still
//!   shown in a terminal too. Rotated to `shor.old.log` past 1 MB.
//! - Lines come from `tracing` (level, then a target: `audio` for the
//!   device and the callback's health, `action` for what the user did,
//!   `ui` for the rest). Warnings and up, plus those three targets at
//!   debug, are kept; `RUST_LOG=ui=trace,audio=trace` changes that.
//! - `Logs/crash-<time>.txt` if the app panics: the message, where, the
//!   backtrace, the version and the OS - and the next launch says so.

use std::io::Write;
use std::path::{Path, PathBuf};

const LOG_LIMIT: u64 = 1024 * 1024;

pub fn logs_dir() -> PathBuf {
    let dir = crate::paths::data_dir().join("Logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn about() -> String {
    format!("Shor {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)
}

/// Starts the log (call first thing in `main`) and the crash hook.
pub fn init() {
    let path = logs_dir().join("shor.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_LIMIT) {
        let _ = std::fs::rename(&path, logs_dir().join("shor.old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "\n[{}] --- {} started ---", now(), about());
        #[cfg(unix)]
        tee_stderr(file);
    }
    install_tracing();
    install_crash_hook();
    tracing::info!(
        "{} - {} logical cores, {} build",
        about(),
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        if cfg!(debug_assertions) { "debug" } else { "release" }
    );
}

/// What gets logged: this app's own targets at debug, everything else
/// (the UI toolkit, the audio library) only when it is a warning.
fn install_tracing() {
    use std::str::FromStr;
    use tracing::Level;
    use tracing_subscriber::filter::Targets;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Layer;

    let default = Targets::new()
        .with_default(Level::WARN)
        .with_targets([("ui", Level::DEBUG), ("engine", Level::DEBUG), ("shared", Level::INFO), ("audio", Level::DEBUG), ("action", Level::DEBUG)]);
    let filter = std::env::var("RUST_LOG").ok().and_then(|spec| Targets::from_str(&spec).ok()).unwrap_or(default);
    // No timestamp here: the stderr copy above adds one.
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .with_filter(filter);
    let _ = tracing_subscriber::registry().with(layer).try_init();
}

/// Everything written to stderr goes to the terminal as before *and* to
/// `file`, a timestamp on each line: stderr is pointed at a pipe, and a
/// thread copies the pipe to both.
#[cfg(unix)]
fn tee_stderr(mut file: std::fs::File) {
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;
    unsafe {
        let mut fds = [0i32; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        let terminal = libc::dup(libc::STDERR_FILENO);
        if terminal < 0 || libc::dup2(fds[1], libc::STDERR_FILENO) < 0 {
            return;
        }
        libc::close(fds[1]);
        let reader = std::fs::File::from_raw_fd(fds[0]);
        let mut terminal = std::fs::File::from_raw_fd(terminal);
        std::thread::Builder::new()
            .name("log".into())
            .spawn(move || {
                for line in BufReader::new(reader).lines() {
                    let Ok(line) = line else { break };
                    let _ = writeln!(terminal, "{line}");
                    let _ = writeln!(file, "[{}] {line}", now());
                }
            })
            .ok();
    }
}

/// On a panic, anywhere (the audio thread too): a crash report file, then
/// the usual message.
fn install_crash_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_string());
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let report = format!(
            "{}\n{}\n\nPanicked on thread '{thread}' at {place}:\n{message}\n\n{}\n",
            about(),
            now(),
            std::backtrace::Backtrace::force_capture()
        );
        let path = logs_dir().join(format!("crash-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        if std::fs::write(&path, report).is_ok() {
            eprintln!("Crash report written to {}", path.display());
        }
        previous(info);
    }));
}

/// Crash reports the user hasn't been told about yet, newest first; each
/// is remembered as seen once returned.
pub fn unseen_crashes() -> Vec<PathBuf> {
    let seen = crate::settings::load_crashes_seen();
    let mut found: Vec<PathBuf> = std::fs::read_dir(logs_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt")))
        .filter(|p| !seen.iter().any(|s| Path::new(s) == p.as_path()))
        .collect();
    found.sort();
    found.reverse();
    if !found.is_empty() {
        let mut all = seen;
        all.extend(found.iter().map(|p| p.display().to_string()));
        crate::settings::save_crashes_seen(&all);
    }
    found
}

/// Shows `path` in the system's file manager, selected where it can be.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}
