#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};

    fn read_complete_request(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set test read timeout");
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let Ok(read) = stream.read(&mut chunk) else {
                break;
            };
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let content_length = String::from_utf8_lossy(&request[..header_end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }

    fn one_shot_server_with_hold(
        response: Vec<u8>,
        delay: Duration,
        hold_open: Duration,
    ) -> (String, JoinHandle<()>) {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind test provider");
        let address = listener.local_addr().expect("test provider address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            read_complete_request(&mut stream);
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            let _ = stream.write_all(&response);
            if !hold_open.is_zero() {
                thread::sleep(hold_open);
            }
        });
        (format!("http://{address}/v1/chat/completions"), handle)
    }

    fn one_shot_server(response: Vec<u8>, delay: Duration) -> (String, JoinHandle<()>) {
        one_shot_server_with_hold(response, delay, Duration::ZERO)
    }

    fn one_shot_disconnect_server() -> (String, JoinHandle<()>) {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind test provider");
        let address = listener.local_addr().expect("test provider address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            read_complete_request(&mut stream);
            let _ = stream.shutdown(Shutdown::Both);
        });
        (format!("http://{address}/v1/chat/completions"), handle)
    }

    fn fixed_response(status: &str, body: &[u8]) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect()
    }

    fn chunked_response(status: &str, body: &[u8]) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n{:x}\r\n",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .chain(b"\r\n0\r\n\r\n".iter().copied())
        .collect()
    }

    fn config() -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::OpenaiCompatible,
            endpoint: "http://127.0.0.1:11434/v1/chat/completions".to_string(),
            model: "openai/gpt-oss-safeguard-20b".to_string(),
            runtime_model: "gpt-oss-safeguard:20b".to_string(),
            reasoning_effort: ReasoningEffort::Medium,
            connect_timeout: Duration::from_secs(1),
            timeout: Duration::from_secs(10),
            max_response_bytes: 65_536,
            max_output_tokens: 1_024,
            context_window_tokens: 9_216,
            runtime_label: Some("ollama".to_string()),
            model_digest: Some("sha256:fixture".to_string()),
            mock_response: None,
            resource_guard: None,
            resource_preflight: None,
        }
    }

    #[test]
    fn request_keeps_policy_roles_schema_and_runtime_alias_explicit() {
        let request = openai_request_payload(&json!({"schema_version": 1}), &config())
            .expect("provider request");
        assert_eq!(request["model"], "gpt-oss-safeguard:20b");
        assert_eq!(request["reasoning_effort"], "medium");
        assert_eq!(request["max_tokens"], 1_024);
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "developer");
        assert_eq!(request["messages"][2]["role"], "user");
        assert_eq!(request["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            request["response_format"]["json_schema"]["schema"]["$id"],
            "https://github.com/coreycoto/git-slop/blob/v0.16.0/schemas/advice-response-1.json"
        );
    }

    #[test]
    fn ollama_request_preserves_zones_structured_output_and_reasoning_controls() {
        let request = ollama_request_payload(&json!({"schema_version": 1}), &config())
            .expect("Ollama request");
        assert_eq!(request["model"], "gpt-oss-safeguard:20b");
        assert_eq!(request["think"], "medium");
        assert_eq!(request["stream"], false);
        assert_eq!(request["options"]["num_predict"], 1_024);
        assert_eq!(request["options"]["num_ctx"], 9_216);
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "developer");
        assert_eq!(request["messages"][2]["role"], "user");
        assert_eq!(
            request["format"]["$id"],
            "https://github.com/coreycoto/git-slop/blob/v0.16.0/schemas/advice-response-1.json"
        );
    }

    #[test]
    fn ollama_adapter_normalizes_structured_content_and_runtime_timings() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-oss-safeguard:20b",
            "message": {"role": "assistant", "content": "{\"schema_version\":1}"},
            "done": true,
            "done_reason": "stop",
            "total_duration": 500,
            "load_duration": 100,
            "prompt_eval_count": 20,
            "prompt_eval_duration": 200,
            "eval_count": 10,
            "eval_duration": 150
        }))
        .expect("Ollama fixture");
        let (endpoint, handle) = one_shot_server(fixed_response("200 OK", &body), Duration::ZERO);
        let mut native = config();
        native.kind = ProviderKind::Ollama;
        native.endpoint = endpoint;
        let result = invoke(&json!({"schema_version": 1}), &native).expect("Ollama response");
        assert_eq!(result.response["schema_version"], 1);
        assert_eq!(result.metadata["provider"], "ollama");
        assert_eq!(result.metadata["usage"]["prompt_tokens"], 20);
        assert!(
            result.metadata.get("endpoint").is_none(),
            "provider provenance must not retain endpoint paths"
        );
        assert_eq!(
            result.metadata["runtime_timings"]["prompt_eval_duration"],
            200
        );
        handle.join().expect("Ollama server thread");
    }

    #[test]
    fn content_length_finishes_without_waiting_for_connection_close() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-oss-safeguard:20b",
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": {"schema_version": 1}}
            }]
        }))
        .expect("OpenAI-compatible fixture");
        let (endpoint, handle) = one_shot_server_with_hold(
            fixed_response("200 OK", &body),
            Duration::ZERO,
            Duration::from_millis(150),
        );
        let mut provider = config();
        provider.endpoint = endpoint;
        provider.timeout = Duration::from_millis(50);
        let result = invoke(&json!({"schema_version": 1}), &provider)
            .expect("Content-Length should complete before close");
        assert_eq!(result.response["schema_version"], 1);
        handle.join().expect("held-open server thread");
    }

    #[test]
    fn chunked_response_finishes_without_waiting_for_connection_close() {
        let body = serde_json::to_vec(&json!({
            "model": "gpt-oss-safeguard:20b",
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": {"schema_version": 1}}
            }]
        }))
        .expect("OpenAI-compatible fixture");
        let (endpoint, handle) = one_shot_server_with_hold(
            chunked_response("200 OK", &body),
            Duration::ZERO,
            Duration::from_millis(150),
        );
        let mut provider = config();
        provider.endpoint = endpoint;
        provider.timeout = Duration::from_millis(50);
        let result = invoke(&json!({"schema_version": 1}), &provider)
            .expect("chunked response should complete before close");
        assert_eq!(result.response["schema_version"], 1);
        handle.join().expect("held-open server thread");
    }

    #[test]
    fn provider_rejects_model_identity_and_completion_drift() {
        for (body, expected) in [
            (
                json!({
                    "model": "different-model",
                    "choices": [{
                        "finish_reason": "stop",
                        "message": {"content": {"schema_version": 1}}
                    }]
                }),
                "provider_model_mismatch",
            ),
            (
                json!({
                    "model": "gpt-oss-safeguard:20b",
                    "choices": [{
                        "finish_reason": "length",
                        "message": {"content": {"schema_version": 1}}
                    }]
                }),
                "provider_incomplete_response",
            ),
        ] {
            let body = serde_json::to_vec(&body).expect("provider fixture");
            let (endpoint, handle) =
                one_shot_server(fixed_response("200 OK", &body), Duration::ZERO);
            let mut provider = config();
            provider.endpoint = endpoint;
            let error = invoke(&json!({"schema_version": 1}), &provider)
                .expect_err("provider provenance drift must fail");
            assert!(format!("{error:#}").contains(expected));
            handle.join().expect("provider drift server thread");
        }
    }

    #[test]
    fn provider_failures_are_normalized_for_malformed_and_oversized_responses() {
        let (endpoint, handle) =
            one_shot_server(fixed_response("200 OK", b"{not-json"), Duration::ZERO);
        let mut malformed = config();
        malformed.endpoint = endpoint;
        let error = invoke(&json!({"schema_version": 1}), &malformed)
            .expect_err("malformed provider envelope must fail");
        assert!(format!("{error:#}").contains("provider_response_invalid"));
        handle.join().expect("malformed server thread");

        let (endpoint, handle) = one_shot_server(
            fixed_response("200 OK", br#"{"choices":[{"message":{"content":{}}}]}"#),
            Duration::ZERO,
        );
        let mut oversized = config();
        oversized.endpoint = endpoint;
        oversized.max_response_bytes = 8;
        let error = invoke(&json!({"schema_version": 1}), &oversized)
            .expect_err("oversized provider envelope must fail");
        assert!(format!("{error:#}").contains("provider_response_too_large"));
        handle.join().expect("oversized server thread");
    }

    #[test]
    fn provider_failures_distinguish_unavailable_models_endpoints_and_timeouts() {
        let (endpoint, handle) = one_shot_server(
            fixed_response("404 Not Found", br#"{"error":"model is not available"}"#),
            Duration::ZERO,
        );
        let mut missing_model = config();
        missing_model.endpoint = endpoint;
        let error = invoke(&json!({"schema_version": 1}), &missing_model)
            .expect_err("missing model response must fail");
        let diagnostic = format!("{error:#}");
        assert!(
            diagnostic.contains("provider_http_error"),
            "unexpected provider diagnostic: {diagnostic}"
        );
        assert!(
            !diagnostic.contains("model is not available"),
            "provider response bodies must not enter diagnostics: {diagnostic}"
        );
        handle.join().expect("missing-model server thread");

        let (endpoint, handle) = one_shot_disconnect_server();
        let mut unavailable = config();
        unavailable.endpoint = endpoint;
        let error = invoke(&json!({"schema_version": 1}), &unavailable)
            .expect_err("disconnected endpoint must fail");
        let diagnostic = format!("{error:#}");
        assert!(
            diagnostic.contains("provider_unavailable"),
            "unexpected provider diagnostic: {diagnostic}"
        );
        handle.join().expect("disconnected server thread");

        let (endpoint, handle) = one_shot_server(
            fixed_response("200 OK", br#"{"choices":[]}"#),
            Duration::from_millis(150),
        );
        let mut timeout = config();
        timeout.endpoint = endpoint;
        timeout.timeout = Duration::from_millis(25);
        let error = invoke(&json!({"schema_version": 1}), &timeout)
            .expect_err("stalled provider must time out");
        assert!(format!("{error:#}").contains("provider_timeout"));
        handle.join().expect("timeout server thread");
    }

    #[test]
    fn provider_provenance_keeps_only_bounded_scalar_metadata() {
        let envelope = json!({
            "system_fingerprint": "unsafe fingerprint with spaces",
            "total_duration": "private timing label",
            "load_duration": 42,
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 3,
                "total_tokens": 15,
                "private_prompt": "must not be retained"
            }
        });
        assert_eq!(safe_system_fingerprint(&envelope), Value::Null);
        assert_eq!(
            openai_usage(&envelope),
            json!({
                "prompt_tokens": 12,
                "completion_tokens": 3,
                "total_tokens": 15
            })
        );
        assert_eq!(
            safe_system_fingerprint(&json!({"system_fingerprint": "fp_123-safe"})),
            "fp_123-safe"
        );
        assert_eq!(
            numeric_metadata_field(&envelope, "total_duration"),
            Value::Null
        );
        assert_eq!(numeric_metadata_field(&envelope, "load_duration"), 42);
    }
}
