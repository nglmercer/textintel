//! Liquid LFM2.5 generation over an explicit local endpoint.
//!
//! Core coverage (no feature gate): a loopback mock server speaking the
//! OpenAI-compatible `/v1/chat/completions` dialect proves request shaping,
//! response parsing, error mapping, and engine wiring. No external server or
//! model download is needed.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use textintel::core::providers::GenerativeProvider;
use textintel::{
    EngineConfig, GeneratedText, GenerationOptions, LIQUID_DEFAULT_MODEL, LiquidInstructProvider,
    TextIntelligence,
};

/// Serve one HTTP request on loopback, capture the raw request, and reply
/// with `status` plus `body`. Returns the endpoint URL and the captured
/// request bytes.
fn mock_server(status: u16, body: &'static str) -> (String, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("local addr")
    );
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        // Read until the client half-closes its write side (Connection: close
        // is only honored after our reply, so bound by Content-Length).
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    request.extend_from_slice(&chunk[..read]);
                    if let Some(body) = find_body(&request)
                        && body.len() >= content_length(&request)
                    {
                        break;
                    }
                    if request.len() > 1_000_000 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(request);
        let reason = if status == 200 { "OK" } else { "Error" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });
    (endpoint, receiver)
}

fn find_body(request: &[u8]) -> Option<&[u8]> {
    let text = std::str::from_utf8(request).ok()?;
    let index = text.find("\r\n\r\n")?;
    Some(&request[index + 4..])
}

fn content_length(request: &[u8]) -> usize {
    let text = std::str::from_utf8(request).unwrap_or("");
    text.lines()
        .filter_map(|line| line.strip_prefix("Content-Length:"))
        .filter_map(|value| value.trim().parse::<usize>().ok())
        .next()
        .unwrap_or(0)
}

const COMPLETION: &str = r#"{
    "model": "lfm2.5-230m-q4_0",
    "choices": [{"message": {"role": "assistant", "content": "hola, ¿en qué te ayudo?"}}],
    "usage": {"prompt_tokens": 9, "completion_tokens": 7}
}"#;

#[test]
fn liquid_posts_openai_chat_shape_and_parses_completion() {
    let (endpoint, captured) = mock_server(200, COMPLETION);
    let provider = LiquidInstructProvider::local_default(&endpoint).expect("provider");
    assert_eq!(provider.model(), LIQUID_DEFAULT_MODEL);
    let generated = provider
        .generate(
            "hola",
            &GenerationOptions::default()
                .with_max_tokens(64)
                .with_system_prompt("be brief"),
        )
        .expect("generate");
    assert_eq!(
        generated,
        GeneratedText {
            text: "hola, ¿en qué te ayudo?".to_string(),
            model: "lfm2.5-230m-q4_0".to_string(),
            prompt_tokens: Some(9),
            completion_tokens: Some(7),
        }
    );
    let request = captured
        .recv_timeout(Duration::from_secs(10))
        .expect("captured request");
    let text = String::from_utf8(request).expect("request text");
    assert!(
        text.starts_with("POST /v1/chat/completions HTTP/1.1"),
        "{text}"
    );
    let body = text.split_once("\r\n\r\n").expect("body split").1;
    let payload: serde_json::Value = serde_json::from_str(body).expect("json body");
    assert_eq!(payload["stream"], false);
    assert_eq!(payload["max_tokens"], 64);
    assert_eq!(payload["messages"][0]["role"], "system");
    assert_eq!(payload["messages"][1]["content"], "hola");
}

#[test]
fn liquid_maps_server_errors_and_rejects_bad_config() {
    let (endpoint, _) = mock_server(500, r#"{"error": "model busy"}"#);
    let provider = LiquidInstructProvider::local_default(&endpoint).expect("provider");
    let error = provider
        .generate("hi", &GenerationOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 500"), "{error}");

    let (endpoint, _) = mock_server(200, r#"{"choices": []}"#);
    let provider = LiquidInstructProvider::local_default(&endpoint).expect("provider");
    assert!(
        provider
            .generate("hi", &GenerationOptions::default())
            .is_err()
    );

    assert!(LiquidInstructProvider::new("https://example.com/v1", "m").is_err());
    assert!(LiquidInstructProvider::new("http://127.0.0.1:8080/v1", "").is_err());
    assert!(LiquidInstructProvider::new("not a url", "m").is_err());
}

#[test]
fn engine_generate_needs_an_explicit_provider() {
    let engine = TextIntelligence::new(EngineConfig::default());
    let error = engine
        .generate("hi", &GenerationOptions::default())
        .unwrap_err();
    assert!(
        error.to_string().contains("no generative provider"),
        "{error}"
    );
    assert!(!engine.provider_capabilities().contains_key("generative"));
    assert!(engine.diagnostics().generative.is_none());

    let (endpoint, _) = mock_server(200, COMPLETION);
    let engine = TextIntelligence::new(EngineConfig::default()).with_generative_provider(
        LiquidInstructProvider::local_default(&endpoint).expect("provider"),
    );
    let generated = engine
        .generate("hola", &GenerationOptions::default())
        .expect("engine generate");
    assert_eq!(generated.text, "hola, ¿en qué te ayudo?");
    let capabilities = engine.provider_capabilities();
    assert_eq!(
        capabilities
            .get("generative")
            .expect("generative capabilities")
            .provider,
        "liquid_instruct"
    );
    assert!(engine.diagnostics().generative.is_some());
}
