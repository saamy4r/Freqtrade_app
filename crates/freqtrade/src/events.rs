//! Live updates from the server.
//!
//! The server refreshes bots in the background and announces what changed;
//! this listens and nudges the screens to re-read. Because the announcement
//! carries no payload, "re-read" is a warm cache hit — the same endpoint the
//! screen already uses, so there is no second data path to keep in step.

/// Keeps the subscription alive; dropping it closes the connection, which is
/// also what tells the server to stop syncing in the background.
pub struct Subscription {
    #[cfg(target_arch = "wasm32")]
    _source: web_sys::EventSource,
    #[cfg(target_arch = "wasm32")]
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

/// Desktop builds have no EventSource; screens still work, they just do not
/// update until asked.
#[cfg(not(target_arch = "wasm32"))]
pub fn subscribe(_on_change: impl FnMut() + 'static) -> Option<Subscription> {
    None
}
