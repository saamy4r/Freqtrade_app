//! Making the app's own failures visible.
//!
//! A native Android app has no stdout, and since Android 4.1 an app can only
//! read its *own* logs — so a logcat reader from the store will not show these
//! either, and `READ_LOGS` needs adb to grant. Without a cable, a crash is
//! simply a window that disappears.
//!
//! So the app records its own crash and shows it on the next launch. The panic
//! hook writes the message to a file beside the database; startup reads that
//! file, deletes it, and hands the text to the UI to display.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Set at startup if the previous run died. Rendered by the UI.
static LAST_CRASH: OnceLock<Option<String>> = OnceLock::new();

fn crash_file(data_dir: &Path) -> PathBuf {
    data_dir.join("last-crash.txt")
}

/// Installs logging and the panic recorder.
///
/// `data_dir` is where the crash note is written; it is the app's private
/// directory, the same place the database lives.
pub fn init(data_dir: &Path) {
    let path = crash_file(data_dir);

    // Read and clear any note the previous run left, before a new panic can
    // overwrite it.
    let previous = std::fs::read_to_string(&path)
        .ok()
        .filter(|s| !s.trim().is_empty());
    if previous.is_some() {
        let _ = std::fs::remove_file(&path);
    }
    let _ = LAST_CRASH.set(previous);

    init_platform_logging();

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_owned());
        let message = format!("{info}");
        let note = format!("{message}\n\nat {location}");

        // Best effort: if this fails there is nothing further to try.
        let _ = std::fs::write(&path, &note);
        report(&note);

        default_hook(info);
    }));
}

/// What the previous run died of, if it died.
pub fn last_crash() -> Option<&'static str> {
    LAST_CRASH.get().and_then(|c| c.as_deref())
}

#[cfg(target_os = "android")]
fn init_platform_logging() {
    use tracing_subscriber::prelude::*;

    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("freqtrade"),
    );

    // Everything below this crate logs through `tracing`; bridge it onto the
    // `log` facade android_logger consumes, or the server's output is lost.
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(LogcatWriter)
                .without_time(),
        )
        .try_init();
}

#[cfg(not(target_os = "android"))]
fn init_platform_logging() {}

#[cfg(target_os = "android")]
fn report(note: &str) {
    log::error!(target: "RustPanic", "{note}");
}

#[cfg(not(target_os = "android"))]
fn report(note: &str) {
    eprintln!("panic: {note}");
}

#[cfg(target_os = "android")]
struct LogcatWriter;

#[cfg(target_os = "android")]
impl std::io::Write for LogcatWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        log::info!("{}", String::from_utf8_lossy(buf).trim_end());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "android")]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogcatWriter {
    type Writer = LogcatWriter;
    fn make_writer(&'a self) -> Self::Writer {
        LogcatWriter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_note_round_trips_and_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = crash_file(dir.path());
        std::fs::write(&path, "boom\n\nat src/lib.rs:1:1").unwrap();

        let read = std::fs::read_to_string(&path)
            .ok()
            .filter(|s| !s.trim().is_empty());
        assert!(read.unwrap().contains("boom"));

        std::fs::remove_file(&path).unwrap();
        assert!(
            std::fs::read_to_string(&path).is_err(),
            "note must not survive"
        );
    }
}
