//! Streaming OpenAI-compatible `/chat/completions` client (blocking, run on
//! a worker thread).

use super::provider::Endpoint;
use super::ChatRequest;
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// Cap on how much of a streamed answer we accept.
const MAX_STREAM_BYTES: u64 = 4 * 1024 * 1024;
const ERROR_BODY_CHARS: usize = 400;
/// The server must start answering (status + headers) within this.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// Upper bound for the whole streamed body. ureq has no per-chunk idle
/// timeout, so this is what frees a worker stuck on a stalled stream.
const BODY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Default, PartialEq)]
pub struct StreamEnd {
    /// The model stopped at `max_tokens` (`finish_reason == "length"`).
    pub truncated: bool,
    /// `on_delta` asked to stop (receiver gone).
    pub cancelled: bool,
}

/// POST `request` and feed each content delta to `on_delta`; returning
/// false from it stops reading. `on_delta` also gets `""` for every stream
/// line without content (keep-alives, reasoning deltas) so a closed panel
/// cancels promptly even before the first answer token.
pub fn stream_chat(
    endpoint: &Endpoint,
    request: &ChatRequest,
    on_delta: impl FnMut(&str) -> bool,
) -> Result<StreamEnd, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        // Keep non-2xx bodies so the provider's error message can be shown.
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_send_request(Some(Duration::from_secs(30)))
        .timeout_send_body(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(BODY_TIMEOUT))
        .build()
        .into();

    let body = serde_json::to_vec(&request.to_json(&endpoint.model)).map_err(|e| e.to_string())?;
    let url = format!("{}/chat/completions", endpoint.base_url);
    let mut req = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header("X-Title", "glc");
    if let Some(key) = &endpoint.api_key {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let response = req
        .send(&body[..])
        .map_err(|e| mask(&format!("request failed: {e}"), endpoint))?;
    let status = response.status();
    let mut body = response.into_body();
    if !status.is_success() {
        let text = body.read_to_string().unwrap_or_default();
        let snippet: String = text.chars().take(ERROR_BODY_CHARS).collect();
        return Err(mask(
            &format!("HTTP {status}: {}", snippet.trim()),
            endpoint,
        ));
    }

    let reader = body.into_with_config().limit(MAX_STREAM_BYTES).reader();
    parse_sse(BufReader::new(reader), on_delta).map_err(|e| mask(&e, endpoint))
}

/// Parse an SSE chat-completions stream. Lenient about the quirks of the
/// various OpenAI-compatible servers: `data:` with or without a space,
/// `:` keep-alive comments, empty `choices` usage chunks, `null` content,
/// reasoning deltas (ignored) and in-band `{"error": …}` payloads.
///
/// A stream that hits EOF without `[DONE]` or a `finish_reason` was cut
/// off mid-answer and is an error, so a partial answer is never treated
/// (and cached) as complete.
pub fn parse_sse<R: BufRead>(
    reader: R,
    mut on_delta: impl FnMut(&str) -> bool,
) -> Result<StreamEnd, String> {
    let mut end = StreamEnd::default();
    let mut finished = false;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("stream read failed: {e}"))?;
        let line = line.trim_end_matches('\r');
        let mut had_content = false;
        let mut stop = false;
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim_start();
            if data == "[DONE]" {
                finished = true;
                break;
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(err) = value.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| err.to_string());
                    return Err(format!("provider error: {msg}"));
                }
                let choices = value.get("choices").and_then(|c| c.as_array());
                for choice in choices.into_iter().flatten() {
                    if let Some(text) = choice
                        .get("delta")
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        .filter(|t| !t.is_empty())
                    {
                        had_content = true;
                        stop |= !on_delta(text);
                    }
                    match choice.get("finish_reason").and_then(|f| f.as_str()) {
                        Some("length") => {
                            end.truncated = true;
                            finished = true;
                        }
                        Some(_) => finished = true,
                        None => {}
                    }
                }
            }
        }
        if !had_content {
            stop |= !on_delta("");
        }
        if stop {
            end.cancelled = true;
            return Ok(end);
        }
    }
    if !finished {
        return Err("stream ended before the model finished (connection closed early)".into());
    }
    Ok(end)
}

/// Some servers echo the Authorization header back in errors.
fn mask(text: &str, endpoint: &Endpoint) -> String {
    match &endpoint.api_key {
        Some(key) if !key.is_empty() => text.replace(key.as_str(), "***"),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &str) -> (Result<StreamEnd, String>, String) {
        let mut out = String::new();
        let res = parse_sse(input.as_bytes(), |d| {
            out.push_str(d);
            true
        });
        (res, out)
    }

    #[test]
    fn test_parse_sse_concatenates_deltas_until_done() {
        let input = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                     data:{\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\r\n\r\n\
                     data: [DONE]\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"ignored\"}}]}\n";
        let (res, out) = collect(input);
        assert_eq!(res, Ok(StreamEnd::default()));
        assert_eq!(out, "Hello");
    }

    #[test]
    fn test_parse_sse_skips_comments_usage_null_and_reasoning() {
        let input = ": OPENROUTER PROCESSING\n\n\
                     data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\",\"content\":null}}]}\n\n\
                     data: {\"choices\":[],\"usage\":{\"total_tokens\":3}}\n\n\
                     data: {not json\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n\
                     data: [DONE]\n";
        let (res, out) = collect(input);
        assert!(res.is_ok());
        assert_eq!(out, "ok");
    }

    #[test]
    fn test_parse_sse_flags_length_truncation() {
        let input =
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"length\"}]}\n";
        let (res, _) = collect(input);
        assert!(res.unwrap().truncated);
    }

    #[test]
    fn test_parse_sse_surfaces_inband_error() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n\
                     data: {\"error\":{\"message\":\"rate limited\",\"code\":429}}\n";
        let (res, out) = collect(input);
        assert_eq!(res, Err("provider error: rate limited".into()));
        assert_eq!(out, "a");
    }

    #[test]
    fn test_parse_sse_eof_without_finish_is_error() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"half an ans\"}}]}\n";
        let (res, out) = collect(input);
        assert!(res.unwrap_err().contains("ended before"));
        assert_eq!(out, "half an ans");
    }

    #[test]
    fn test_parse_sse_finish_reason_without_done_is_complete() {
        let input =
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"stop\"}]}\n";
        let (res, _) = collect(input);
        assert_eq!(res, Ok(StreamEnd::default()));
    }

    #[test]
    fn test_parse_sse_cancels_on_keepalive_before_first_token() {
        let input = ": keep-alive\n\
                     data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"never\"}}]}\n";
        let mut calls = Vec::new();
        let res = parse_sse(input.as_bytes(), |d| {
            calls.push(d.to_string());
            false
        });
        assert!(res.unwrap().cancelled);
        assert_eq!(calls, vec![String::new()]);
    }

    #[test]
    fn test_parse_sse_stops_when_callback_declines() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n";
        let mut seen = 0;
        let res = parse_sse(input.as_bytes(), |_| {
            seen += 1;
            false
        });
        assert!(res.unwrap().cancelled);
        assert_eq!(seen, 1);
    }

    #[test]
    fn test_mask_hides_key() {
        let ep = Endpoint {
            provider: "p".into(),
            base_url: "u".into(),
            model: "m".into(),
            api_key: Some("sk-abc".into()),
            key_source: super::super::provider::KeySource::Config,
        };
        assert_eq!(mask("bad key sk-abc", &ep), "bad key ***");
    }
}
