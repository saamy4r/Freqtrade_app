//! Making the app's own failures visible on Android.
//!
//! A native Android app has no stdout. Anything printed there is discarded, so
//! a panic takes the process down leaving nothing behind but a disappearing
//! window. Everything here exists so that a crash says why.

/// Routes logging to logcat and installs a panic hook that reports through it.
///
/// Read the output with:
///
/// ```text
/// adb logcat -s freqtrade:V RustPanic:V
/// ```
#[cfg(target_os = "android")]
pub fn init() {
    use tracing_subscriber::prelude::*;

    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("freqtrade"),
    );

    // The crates below this one all log through `tracing`; bridge it onto the
    // `log` facade that android_logger consumes, or none of the server's
    // output would appear.
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(LogcatWriter)
                .without_time(),
        )
        .try_init();

    // Default hook writes to stderr, which is discarded. Without this a panic
    // is completely invisible.
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_owned());
        log::error!(target: "RustPanic", "panic at {location}: {info}");
    }));

    log::info!("logging initialised");
}

/// Writes `tracing` output into the Android log.
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

/// Everywhere else the default logging is already useful.
#[cfg(not(target_os = "android"))]
pub fn init() {}
