//! The auth requests' shared HTTP handling.

use anyhow::Context;
use serde::de::DeserializeOwned;
use ureq::http::Response;
use ureq::typestate::{WithBody, WithoutBody};
use ureq::{Body, RequestBuilder};

/// No request waits longer: a dead network must not hold the launcher.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

pub fn post(url: &str) -> RequestBuilder<WithBody> {
    ureq::post(url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
}

pub fn get(url: &str) -> RequestBuilder<WithoutBody> {
    ureq::get(url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
}

/// A sent request's response, if it arrived with a success status; else an error naming `what`.
pub fn check(
    what: &str,
    sent: Result<Response<Body>, ureq::Error>,
) -> anyhow::Result<Response<Body>> {
    let mut response = sent.with_context(|| format!("{what} (network)"))?;
    if !response.status().is_success() {
        return Err(error(what, &mut response));
    }
    Ok(response)
}

/// [`check`], then the response's JSON.
pub fn json<T: DeserializeOwned>(
    what: &str,
    sent: Result<Response<Body>, ureq::Error>,
) -> anyhow::Result<T> {
    check(what, sent)?
        .body_mut()
        .read_json()
        .with_context(|| format!("{what}: unreadable answer"))
}

/// An error status with its body.
pub fn error(what: &str, response: &mut Response<Body>) -> anyhow::Error {
    let status = response.status();
    let text = response
        .body_mut()
        .read_to_string()
        .unwrap_or_else(|_| "<no answer text>".to_string());
    anyhow::anyhow!("{what}: http status {status}: {}", short_body(&text))
}

/// A body fit for a message.
fn short_body(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("<html") || lower.contains("<head") || lower.contains("<!doctype") {
        let title = lower.find("<title>").and_then(|from| {
            let from = from + "<title>".len();
            let len = lower[from..].find("</title>")?;
            Some(text[from..from + len].trim().to_string())
        });
        return title.unwrap_or_else(|| "(the service's error page)".to_string());
    }
    let text = text.trim();
    match text.char_indices().nth(300) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text.to_string(),
    }
}
