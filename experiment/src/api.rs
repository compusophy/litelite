//! Generation: the Anthropic Messages API, over raw HTTP (Rust has no SDK).
//!
//! Two facts shape everything here:
//!   * `stratlite::REFERENCE` is embedded as a CONST, compiled in, never
//!     copy-pasted. Prompt/verifier drift is impossible by construction — the
//!     card the model reads and the language the verifier enforces are one
//!     artifact.
//!   * The model is not reproducible. `temperature`/`top_p`/`top_k` are
//!     REMOVED on Opus 4.8 (they return 400) and no seed parameter exists on
//!     any Anthropic model. So diversity comes from PROMPT VARIATION, every
//!     varying byte is recorded, and every response is written to raw.jsonl
//!     verbatim BEFORE anything interprets it.
//!
//! No prompt caching: REFERENCE (~400 tok) plus framing lands well under Opus
//! 4.8's 4096-token minimum cacheable prefix, so `cache_control` would be a
//! silent no-op (`cache_creation_input_tokens: 0`, no error). Do not add it.

use serde_json::{Value, json};

/// Pinned for the paper. Never a date suffix — the alias IS the id.
pub const MODEL: &str = "claude-opus-4-8";
const API: &str = "https://api.anthropic.com/v1";
const VERSION: &str = "2023-06-01";

/// The generation card: the language's own const, framed with the task.
pub fn system_prompt() -> String {
    format!(
        "You write programs in stratlite, a tiny total language for trading \
         strategies. Here is the complete language reference.\n\n{}\n\nEmit ONE \
         stratlite program and nothing else. It must parse and run under the \
         reference above.",
        stratlite::REFERENCE
    )
}

/// Force the shape. Assistant prefill returns 400 on Opus 4.8; structured
/// outputs are its documented replacement.
fn schema() -> Value {
    json!({
        "type": "json_schema",
        "schema": {
            "type": "object",
            "properties": { "source": { "type": "string" } },
            "required": ["source"],
            "additionalProperties": false
        }
    })
}

/// One generation request. `variant` is a nonce that guarantees no two
/// prompts are byte-identical; `style` is the recorded diversity axis.
pub fn params(variant: u32, style: &str) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": 4096,
        "system": system_prompt(),
        "output_config": { "format": schema() },
        "messages": [{
            "role": "user",
            "content": format!("Variant {variant}. Write {style}.")
        }]
        // No `thinking` key: Opus 4.8 runs without thinking when it is absent.
        // No `effort`: the default is "high".
        // No temperature/top_p/top_k: they 400 on this model.
    })
}

fn key() -> Result<String, String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(k);
    }
    // The repo's gitignored /.env, the same file the release path uses.
    let env = std::fs::read_to_string("../.env")
        .map_err(|_| "no ANTHROPIC_API_KEY and no ../.env".to_string())?;
    for line in env.lines() {
        if let Some(v) = line.trim().strip_prefix("ANTHROPIC_API_KEY=") {
            return Ok(v.trim().to_string());
        }
    }
    Err("../.env has no ANTHROPIC_API_KEY".into())
}

fn post(path: &str, body: &Value) -> Result<Value, String> {
    ureq::post(&format!("{API}{path}"))
        .set("x-api-key", &key()?)
        .set("anthropic-version", VERSION)
        .set("content-type", "application/json")
        .send_json(body.clone())
        .map_err(http_err)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn get(path: &str) -> Result<String, String> {
    ureq::get(&format!("{API}{path}"))
        .set("x-api-key", &key()?)
        .set("anthropic-version", VERSION)
        .call()
        .map_err(http_err)?
        .into_string()
        .map_err(|e| e.to_string())
}

// 429 carries retry-after; 529 is overload. Both are retryable, and the
// caller decides — we surface the code rather than swallow it.
fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, r) => {
            format!("HTTP {c}: {}", r.into_string().unwrap_or_default())
        }
        e => e.to_string(),
    }
}

/// Pull the strategy source out of one Messages response, or say why not.
/// `stop_reason` is checked BEFORE `content` — a refusal has no content, and
/// indexing it would panic on exactly the case we most need to count.
pub fn extract(msg: &Value) -> Result<String, String> {
    match msg["stop_reason"].as_str() {
        Some("end_turn") => {}
        Some(other) => return Err(format!("stop_reason={other}")),
        None => return Err("no stop_reason".into()),
    }
    let text = msg["content"]
        .as_array()
        .and_then(|a| a.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or("no text block")?;
    let parsed: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    parsed["source"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "no `source` field".into())
}

/// One generation attempt: the style that asked for it, and either the
/// strategy source or the coded reason there isn't one (a refusal, a
/// truncation, a malformed body). Failures are DATA — the refusal rate is a
/// number §5 reports, not an error to swallow.
pub type Attempt = (String, Result<String, String>);

/// A handful of SYNCHRONOUS requests, before any batch is billed. Surfaces a
/// rejected schema, a wrong `stop_reason`, or a refusal rate in ~30 seconds
/// instead of after a 1,600-request batch has been paid for.
pub fn pilot(styles: &[String]) -> Result<Vec<Attempt>, String> {
    let mut out = Vec::new();
    for (i, style) in styles.iter().enumerate() {
        let msg = post("/messages", &params(i as u32, style))?;
        // Record before interpreting — always.
        println!("--- variant {i}: served by {}", msg["model"]);
        out.push((style.clone(), extract(&msg)));
    }
    Ok(out)
}

/// Submit the pool as one batch. 50% cost, and generation is not
/// latency-sensitive. Returns the batch id.
pub fn submit(reqs: &[(String, Value)]) -> Result<String, String> {
    let body = json!({
        "requests": reqs.iter()
            .map(|(id, p)| json!({ "custom_id": id, "params": p }))
            .collect::<Vec<_>>()
    });
    let r = post("/messages/batches", &body)?;
    r["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("no batch id in {r}"))
}

/// Is the batch done? (`processing_status == "ended"`)
pub fn ended(batch_id: &str) -> Result<bool, String> {
    let r: Value = serde_json::from_str(&get(&format!("/messages/batches/{batch_id}"))?)
        .map_err(|e| e.to_string())?;
    Ok(r["processing_status"] == "ended")
}

/// The raw JSONL results. Returned verbatim so the caller can persist them
/// before interpreting. Results arrive in ANY order — the caller keys by
/// `custom_id`, never by position.
pub fn results(batch_id: &str) -> Result<String, String> {
    get(&format!("/messages/batches/{batch_id}/results"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_is_the_const_not_a_copy() {
        // If this ever fails, prompt/verifier drift has become possible.
        assert!(system_prompt().contains(stratlite::REFERENCE));
        assert!(system_prompt().contains("lookback N;"));
    }

    #[test]
    fn request_omits_every_parameter_opus_4_8_rejects() {
        let p = params(7, "a trend-following strategy");
        assert_eq!(p["model"], MODEL);
        // These four are 400s on Opus 4.8 — their absence is load-bearing.
        for banned in ["temperature", "top_p", "top_k", "thinking"] {
            assert!(p.get(banned).is_none(), "{banned} must not be sent");
        }
        assert_eq!(p["output_config"]["format"]["type"], "json_schema");
        assert!(
            p["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Variant 7.")
        );
    }

    #[test]
    fn refusal_is_counted_not_a_panic() {
        // The case that would index an empty `content` array if we read it first.
        let refused = json!({ "stop_reason": "refusal", "content": [] });
        assert_eq!(extract(&refused), Err("stop_reason=refusal".into()));
        let truncated = json!({ "stop_reason": "max_tokens", "content": [] });
        assert!(extract(&truncated).unwrap_err().contains("max_tokens"));
    }

    #[test]
    fn a_good_response_yields_the_source() {
        let ok = json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "{\"source\":\"lookback 4;\\nsignal long;\"}" }]
        });
        assert_eq!(extract(&ok).unwrap(), "lookback 4;\nsignal long;");
    }
}
