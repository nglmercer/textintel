//! Local Liquid LFM2.5 instruction generation over an explicit
//! OpenAI-compatible endpoint.
//!
//! [`LiquidInstructProvider`] talks to a user-run local server (llama-server
//! with an LFM2.5 GGUF, Ollama, vLLM) at `POST /v1/chat/completions`. The
//! HTTP client is standard-library only and restricted to `http://`
//! endpoints: TLS termination belongs to the deployment, not to this crate.
//! The server applies the checkpoint's chat template, so the provider sends
//! plain `system`/`user` messages and reads back the assistant content.
//!
//! The engine never constructs this provider by itself: generation always
//! needs an explicit endpoint, so analyzing a message can never send input
//! to a model implicitly. Serve e.g.:
//!
//! ```sh
//! llama-server -hf LiquidAI/LFM2.5-230M-GGUF:Q4_K_M -c 2048
//! ```

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::{
    GeneratedText, GenerationOptions, GenerativeProvider as GenerativeProviderTrait,
};

const PROVIDER: &str = "liquid_instruct";
/// Smallest Liquid LFM2.5 instruct checkpoint: fastest CPU generation.
pub const DEFAULT_MODEL: &str = "LiquidAI/LFM2.5-230M";
/// Conventional llama-server chat-completions URL.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080/v1/chat/completions";
/// Largest prompt accepted per request (bytes).
const MAX_PROMPT_BYTES: usize = 100_000;
/// Largest response body buffered per request (bytes).
const MAX_BODY_BYTES: usize = 1_000_000;

/// Parsed `http://host:port/path` endpoint.
#[derive(Debug, Clone)]
struct Endpoint {
    host: String,
    port: u16,
    path: String,
}

impl Endpoint {
    fn parse(endpoint: &str) -> Result<Self, ProviderError> {
        if endpoint.starts_with("https://") {
            return Err(ProviderError::new(
                PROVIDER,
                "https endpoints are not supported by the std-only client; terminate TLS \
                 in your deployment and point the provider at the local http:// endpoint",
            ));
        }
        let rest = endpoint.strip_prefix("http://").ok_or_else(|| {
            ProviderError::new(
                PROVIDER,
                "endpoint must use http:// (local server expected)",
            )
        })?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], rest[index..].to_string()),
            None => (rest, "/".to_string()),
        };
        if authority.is_empty() {
            return Err(ProviderError::new(PROVIDER, "endpoint is missing a host"));
        }
        let (host, port) = match authority.rfind(':') {
            Some(index) => {
                let port = authority[index + 1..].parse::<u16>().map_err(|_| {
                    ProviderError::new(
                        PROVIDER,
                        format!("endpoint has an invalid port in {authority:?}"),
                    )
                })?;
                (authority[..index].to_string(), port)
            }
            None => (authority.to_string(), 80),
        };
        if host.is_empty() {
            return Err(ProviderError::new(PROVIDER, "endpoint is missing a host"));
        }
        Ok(Self { host, port, path })
    }

    fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Local Liquid LFM2.5 generation provider (explicit endpoint only).
#[derive(Debug, Clone)]
pub struct LiquidInstructProvider {
    endpoint: Endpoint,
    endpoint_display: String,
    model: String,
    api_key: Option<String>,
    options: GenerationOptions,
    connect_timeout: Duration,
    io_timeout: Duration,
}

impl LiquidInstructProvider {
    /// Build a provider for `endpoint` (must be `http://`, e.g.
    /// [`DEFAULT_ENDPOINT`]) serving `model` (defaults to [`DEFAULT_MODEL`]).
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let endpoint_display = endpoint.into();
        let endpoint = Endpoint::parse(&endpoint_display)?;
        let model = model.into();
        if model.trim().is_empty() {
            return Err(ProviderError::new(PROVIDER, "model cannot be empty"));
        }
        Ok(Self {
            endpoint,
            endpoint_display,
            model,
            api_key: None,
            options: GenerationOptions::default(),
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(120),
        })
    }

    /// Provider for a local server with the default LFM2.5-230M model id.
    /// The server must already be running; nothing is downloaded or started.
    pub fn local_default(endpoint: impl Into<String>) -> Result<Self, ProviderError> {
        Self::new(endpoint, DEFAULT_MODEL)
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_options(mut self, options: GenerationOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.options = self.options.with_max_tokens(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.options = self.options.with_temperature(temperature);
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.options = self.options.with_system_prompt(prompt);
        self
    }

    pub fn with_timeouts(mut self, connect: Duration, io: Duration) -> Self {
        self.connect_timeout = connect;
        self.io_timeout = io;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint_display
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn request_body(&self, prompt: &str, options: &GenerationOptions) -> String {
        let mut messages = String::new();
        if let Some(system) = options
            .system_prompt
            .as_ref()
            .or(self.options.system_prompt.as_ref())
        {
            messages.push_str(&format!(
                "{{\"role\":\"system\",\"content\":{}}},",
                json_string(system)
            ));
        }
        messages.push_str(&format!(
            "{{\"role\":\"user\",\"content\":{}}}",
            json_string(prompt)
        ));
        let max_tokens = if options.max_tokens != GenerationOptions::default().max_tokens {
            options.max_tokens
        } else {
            self.options.max_tokens
        };
        let temperature = if (options.temperature - GenerationOptions::default().temperature).abs()
            > f32::EPSILON
        {
            options.temperature
        } else {
            self.options.temperature
        };
        format!(
            "{{\"model\":{},\"messages\":[{}],\"max_tokens\":{},\"temperature\":{},\"stream\":false}}",
            json_string(&self.model),
            messages,
            max_tokens,
            temperature,
        )
    }

    fn post(&self, body: &str) -> Result<serde_json::Value, ProviderError> {
        let address = self.endpoint.address();
        let socket = address
            .to_socket_addrs()
            .map_err(|error| http_error(format!("cannot resolve {}: {error}", self.endpoint.host)))?
            .next()
            .ok_or_else(|| http_error(format!("cannot resolve {}", self.endpoint.host)))?;
        let mut stream = TcpStream::connect_timeout(&socket, self.connect_timeout)
            .map_err(|error| http_error(format!("cannot connect to {address}: {error}")))?;
        stream
            .set_read_timeout(Some(self.io_timeout))
            .map_err(|error| http_error(format!("cannot set read timeout: {error}")))?;
        stream
            .set_write_timeout(Some(self.io_timeout))
            .map_err(|error| http_error(format!("cannot set write timeout: {error}")))?;
        let mut request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.endpoint.path,
            self.endpoint.address(),
            body.len()
        );
        if let Some(api_key) = &self.api_key {
            request.push_str(&format!("Authorization: Bearer {api_key}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream
            .write_all(request.as_bytes())
            .map_err(|error| http_error(format!("request write failed: {error}")))?;
        let mut raw = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    raw.extend_from_slice(&chunk[..read]);
                    if raw.len() > MAX_BODY_BYTES + 8192 {
                        return Err(http_error(format!(
                            "response exceeds {} bytes",
                            MAX_BODY_BYTES
                        )));
                    }
                }
                Err(error) => {
                    return Err(http_error(format!("response read failed: {error}")));
                }
            }
        }
        parse_response(&raw)
    }

    fn generate_with(
        &self,
        prompt: &str,
        options: &GenerationOptions,
    ) -> Result<GeneratedText, ProviderError> {
        if prompt.len() > MAX_PROMPT_BYTES {
            return Err(ProviderError::new(
                PROVIDER,
                format!(
                    "prompt is {} bytes, limit is {MAX_PROMPT_BYTES}",
                    prompt.len()
                ),
            ));
        }
        let body = self.request_body(prompt, options);
        let payload = self.post(&body)?;
        parse_completion(&payload, &self.model)
    }
}

impl GenerativeProviderTrait for LiquidInstructProvider {
    fn generate(
        &self,
        prompt: &str,
        options: &GenerationOptions,
    ) -> Result<GeneratedText, ProviderError> {
        self.generate_with(prompt, options)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new(PROVIDER)
            .remote()
            .with_version("openai-compatible-v1")
            .with_languages(["multilingual".to_string()])
            .with_quality(CapabilityLevel::Production)
    }
}

fn http_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(PROVIDER, message.into())
}

/// Minimal JSON string escaper (std only): the request body has no other
/// dynamic structure, so a full serializer is unnecessary.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn parse_response(raw: &[u8]) -> Result<serde_json::Value, ProviderError> {
    let text = std::str::from_utf8(raw).map_err(|_| http_error("response is not valid UTF-8"))?;
    let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| {
        http_error("response is not a valid HTTP/1.1 message (missing header/body split)")
    })?;
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| http_error(format!("cannot parse status line {status_line:?}")))?;
    if status != 200 {
        let snippet: String = body.chars().take(300).collect();
        return Err(http_error(format!(
            "server returned HTTP {status}: {snippet}"
        )));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(http_error(format!(
            "response body exceeds {MAX_BODY_BYTES} bytes"
        )));
    }
    serde_json::from_str(body)
        .map_err(|error| http_error(format!("response is not valid JSON: {error}")))
}

fn parse_completion(
    payload: &serde_json::Value,
    fallback_model: &str,
) -> Result<GeneratedText, ProviderError> {
    let choice = payload
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| http_error("response has no choices[0]"))?;
    let text = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| http_error("response choices[0].message.content is missing"))?;
    let model = payload
        .get("model")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback_model)
        .to_string();
    let usage = payload.get("usage");
    let tokens = |field: &str| {
        usage
            .and_then(|usage| usage.get(field))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    Ok(GeneratedText {
        text: text.to_string(),
        model,
        prompt_tokens: tokens("prompt_tokens"),
        completion_tokens: tokens("completion_tokens"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parsing_accepts_local_http_only() {
        let endpoint = Endpoint::parse("http://127.0.0.1:8080/v1/chat/completions").unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 8080);
        assert_eq!(endpoint.path, "/v1/chat/completions");
        let bare = Endpoint::parse("http://localhost:11434").unwrap();
        assert_eq!(bare.port, 11434);
        assert_eq!(bare.path, "/");
        assert!(Endpoint::parse("https://example.com/v1").is_err());
        assert!(Endpoint::parse("ftp://example.com/x").is_err());
        assert!(Endpoint::parse("http:///no-host").is_err());
        assert!(Endpoint::parse("http://host:notaport/x").is_err());
        assert!(LiquidInstructProvider::new(DEFAULT_ENDPOINT, "  ").is_err());
    }

    #[test]
    fn request_body_carries_messages_and_bounds() {
        let provider = LiquidInstructProvider::local_default(DEFAULT_ENDPOINT)
            .unwrap()
            .with_max_tokens(64)
            .with_temperature(0.2)
            .with_system_prompt("be brief");
        let body = provider.request_body("hello", &GenerationOptions::default());
        let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["model"], DEFAULT_MODEL);
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["max_tokens"], 64);
        assert_eq!(payload["messages"][0]["role"], "system");
        assert_eq!(payload["messages"][1]["content"], "hello");
        // Per-call options override the provider defaults.
        let body = provider.request_body(
            "hi",
            &GenerationOptions::default()
                .with_max_tokens(7)
                .with_temperature(1.5),
        );
        let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["max_tokens"], 7);
    }

    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn response_parsing_reports_status_and_shape() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        assert!(parse_response(ok).is_ok());
        let bad_status = b"HTTP/1.1 500 boom\r\nContent-Length: 11\r\n\r\nserver down";
        let error = parse_response(bad_status).unwrap_err();
        assert!(error.to_string().contains("HTTP 500"), "{error}");
        assert!(parse_response(b"not http").is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n\r\n{invalid").is_err());
    }

    #[test]
    fn completion_parsing_reads_openai_shape() {
        let payload = serde_json::json!({
            "model": "lfm2.5-230m-q4_0",
            "choices": [{"message": {"role": "assistant", "content": "hi there"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3},
        });
        let generated = parse_completion(&payload, DEFAULT_MODEL).unwrap();
        assert_eq!(generated.text, "hi there");
        assert_eq!(generated.model, "lfm2.5-230m-q4_0");
        assert_eq!(generated.prompt_tokens, Some(12));
        assert_eq!(generated.completion_tokens, Some(3));
        assert!(parse_completion(&serde_json::json!({}), DEFAULT_MODEL).is_err());
        assert!(
            parse_completion(
                &serde_json::json!({"choices": [{"message": {}}]}),
                DEFAULT_MODEL
            )
            .is_err()
        );
    }

    #[test]
    fn oversized_prompts_fail_before_any_io() {
        let provider = LiquidInstructProvider::local_default("http://127.0.0.1:1/").unwrap();
        let big = "x".repeat(MAX_PROMPT_BYTES + 1);
        let error = provider
            .generate(&big, &GenerationOptions::default())
            .unwrap_err();
        assert!(error.to_string().contains("limit"), "{error}");
    }
}
