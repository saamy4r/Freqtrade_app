//! The server that ships inside the app.
//!
//! On the web the UI talks to a standalone `ft-server`. In a packaged app the
//! same server runs in-process on loopback, so there is exactly one code path
//! for the UI to exercise either way.

#![cfg(feature = "embedded-server")]

use std::path::PathBuf;
use std::sync::Arc;

use ft_store::{FileKey, Store};

/// Starts the embedded server and returns the port it bound.
///
/// Blocks until the socket is listening: the UI cannot ask for anything before
/// it knows the port, so there is nothing to gain from returning early.
pub fn start(db_path: PathBuf) -> Result<u16, String> {
    let (tx, rx) = std::sync::mpsc::channel();

    // Its own thread with its own runtime. The UI toolkit owns the main thread
    // and runs its own event loop there.
    std::thread::Builder::new()
        .name("ft-server".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(e) => {
                    let _ = tx.send(Err(format!("could not start a runtime: {e}")));
                    return;
                }
            };
            runtime.block_on(async move {
                let store = match Store::open(&db_path, &FileKey::beside(&db_path)) {
                    Ok(store) => Arc::new(store),
                    Err(e) => {
                        let _ = tx.send(Err(format!("could not open the database: {e}")));
                        return;
                    }
                };
                match ft_server::spawn_embedded(store).await {
                    Ok(running) => {
                        let _ = tx.send(Ok(running.port));
                        // Keep the runtime alive for the life of the app.
                        let _ = running.handle.await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(format!("could not bind a port: {e}")));
                    }
                }
            });
        })
        .map_err(|e| format!("could not spawn the server thread: {e}"))?;

    rx.recv()
        .map_err(|_| "the server thread stopped before it reported a port".to_owned())?
}

/// Where the database lives.
#[cfg(target_os = "android")]
pub fn data_dir() -> Result<PathBuf, String> {
    // Android does not hand a native library a writable path; it has to be
    // asked for through the Java `Context`. This is the app's private
    // directory, which is also why the credential key file is safe to keep
    // beside the database until the Keystore lands.
    use jni::objects::{JObject, JString};

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("no Java VM: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("could not attach to the Java VM: {e}"))?;
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };

    let files_dir = env
        .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
        .and_then(|v| v.l())
        .map_err(|e| format!("getFilesDir failed: {e}"))?;
    let path = env
        .call_method(&files_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|e| format!("getAbsolutePath failed: {e}"))?;
    let path: String = env
        .get_string(&JString::from(path))
        .map_err(|e| format!("could not read the path: {e}"))?
        .into();

    Ok(PathBuf::from(path))
}

/// Desktop: the same XDG location the standalone server uses, so a developer
/// switching between them sees the same bots.
#[cfg(not(target_os = "android"))]
pub fn data_dir() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|base| base.join("freqtrade-visualizer"))
        .ok_or_else(|| "no home directory".to_owned())
}
