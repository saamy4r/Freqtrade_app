//! Live updates from the server.
//!
//! The server refreshes bots in the background and announces what changed;
//! this listens and nudges the screens to re-read. Because the announcement
//! carries no payload, "re-read" is a warm cache hit — the same endpoint the
//! screen already uses, so there is no second data path to keep in step.
//!
//! Holding this stream open is also what tells the server someone is watching.
//! That matters more than it looks: the background sync runs only while a
//! client is connected, so a platform with no implementation here gets no
//! background sync at all — and therefore never notices a bot coming back.
//! The browser has `EventSource`; the packaged app is native Rust and does not,
//! so it reads the same stream over HTTP itself.

/// Keeps the subscription alive; dropping it closes the connection, which is
/// also what tells the server to stop syncing in the background.
#[cfg(target_arch = "wasm32")]
pub struct Subscription {
    _source: web_sys::EventSource,
    _on_message: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
}

/// Opens the event stream, calling `on_change` whenever the server reports new
/// data. Returns `None` if the browser refuses the connection.
#[cfg(target_arch = "wasm32")]
pub fn subscribe(mut on_change: impl FnMut() + 'static) -> Option<Subscription> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let url = format!("{}/events", crate::api::base_url());
    let source = web_sys::EventSource::new(&url).ok()?;

    let on_message = Closure::wrap(Box::new(move |_event: web_sys::MessageEvent| {
        // The payload is intentionally ignored: it names what changed, and the
        // screens re-read whatever they need themselves.
        on_change();
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

    Some(Subscription {
        _source: source,
        _on_message: on_message,
    })
}

/// Aborting the task closes the HTTP connection, which is what decrements the
/// server's watcher count.
#[cfg(all(not(target_arch = "wasm32"), feature = "embedded-server"))]
pub struct Subscription {
    task: tokio::task::JoinHandle<()>,
}

#[cfg(all(not(target_arch = "wasm32"), feature = "embedded-server"))]
impl Drop for Subscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Reads `GET /api/events` as Server-Sent Events.
///
/// The stream is loopback to our own embedded server, so it does not drop when
/// the device loses signal — which is the point: the sync keeps running and is
/// what notices the bot again. It still reconnects, because the server may not
/// have finished binding when the first screen renders.
///
/// Returns `None` when there is no Tokio runtime to spawn on, leaving the app
/// exactly as it behaved before: screens work, they just do not update until
/// asked. A missing reactor must not be a panic — the release profile aborts
/// on panic, so a panicking background task would take the whole app with it.
#[cfg(all(not(target_arch = "wasm32"), feature = "embedded-server"))]
pub fn subscribe(mut on_change: impl FnMut() + Send + 'static) -> Option<Subscription> {
    use futures_util::StreamExt;

    tokio::runtime::Handle::try_current().ok()?;

    let url = format!("{}/events", crate::api::base_url());
    // No request timeout: this connection is meant to stay open indefinitely.
    let http = reqwest::Client::builder().build().ok()?;

    let task = tokio::spawn(async move {
        loop {
            if let Ok(response) = http.get(&url).send().await {
                let mut stream = response.bytes_stream();
                let mut pending = Vec::new();
                while let Some(Ok(chunk)) = stream.next().await {
                    pending.extend_from_slice(&chunk);
                    // Frames arrive split across chunks, so only whole lines
                    // are interpreted and the remainder is carried over.
                    while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = pending.drain(..=end).collect();
                        // `data:` is an event; `:keep-alive` is the server
                        // holding the connection open and means nothing.
                        if line.starts_with(b"data:") {
                            on_change();
                        }
                    }
                }
            }
            // Dropped or refused. Wait before retrying so a server that is not
            // up yet is not hammered.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    Some(Subscription { task })
}

/// Builds with no server to talk to have nothing to subscribe to.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "embedded-server")))]
pub struct Subscription;

#[cfg(all(not(target_arch = "wasm32"), not(feature = "embedded-server")))]
pub fn subscribe(_on_change: impl FnMut() + Send + 'static) -> Option<Subscription> {
    None
}
