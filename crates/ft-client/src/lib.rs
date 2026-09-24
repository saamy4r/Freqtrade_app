//! HTTP client for the Freqtrade REST API.
//!
//! Unlike the Flutter implementation it replaces, this keeps the `refresh_token`
//! returned by `/token/login` and transparently refreshes on a 401, single-flight,
//! so a screen's parallel calls trigger one refresh rather than one per request.
