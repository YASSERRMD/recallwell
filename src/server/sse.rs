//! Small helpers for Server-Sent Events.

use std::convert::Infallible;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;

pub fn sse_response<S>(stream: S) -> Sse<S>
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub fn event_json(name: &str, value: &serde_json::Value) -> Result<Event, Infallible> {
    Ok(Event::default().event(name).data(value.to_string()))
}
