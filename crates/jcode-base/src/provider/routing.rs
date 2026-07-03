pub(crate) fn should_eager_detect_copilot_tier() -> bool {
    std::env::var("JCODE_NON_INTERACTIVE").is_err()
}

pub(crate) fn is_transient_transport_error(error_str: &str) -> bool {
    let lower = error_str.to_ascii_lowercase();
    lower.contains("connection reset")
        || lower.contains("connection closed")
        || lower.contains("connection refused")
        || lower.contains("connection aborted")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("operation timed out")
        || lower.contains("error decoding")
        || lower.contains("error reading")
        || lower.contains("unexpected eof")
        // rustls reports an abrupt TCP close mid-stream as "peer closed
        // connection without sending TLS close_notify" (its docs URL spells
        // it "unexpected-eof", which the space-separated marker above does
        // not match).
        || lower.contains("close_notify")
        || lower.contains("peer closed connection")
        || lower.contains("tls handshake eof")
        // reqwest/hyper wrap connect-phase failures as "client error
        // (Connect)" and connection-level faults as "connection error: ...".
        || lower.contains("client error (connect)")
        || lower.contains("connection error")
        // hyper h1: connection closed while a response was still incomplete.
        || lower.contains("incomplete message")
        || lower.contains("request or response body error")
        || lower.contains("badrecordmac")
        || lower.contains("bad_record_mac")
        || lower.contains("fatal alert: badrecordmac")
        || lower.contains("fatal alert: bad_record_mac")
        || lower.contains("received fatal alert: badrecordmac")
        || lower.contains("received fatal alert: bad_record_mac")
        || lower.contains("decryption failed or bad record mac")
        || lower.contains("temporary failure in name resolution")
        || lower.contains("failed to lookup address information")
        || lower.contains("dns error")
        || lower.contains("name or service not known")
        || lower.contains("no route to host")
        || lower.contains("network is unreachable")
        || lower.contains("host is unreachable")
        // HTTP/2 transport faults on reused/multiplexed connections. These are
        // transient: a fresh connection on retry typically succeeds. Seen as
        // "http2 error: stream error received: unspecific protocol error detected"
        // or RST_STREAM / GOAWAY frames from the server or an intermediary.
        || lower.contains("http2 error")
        || lower.contains("stream error")
        || lower.contains("protocol error")
        || lower.contains("refused_stream")
        || lower.contains("refused stream")
        || lower.contains("enhance_your_calm")
        || lower.contains("goaway")
        || lower.contains("go away")
        || lower.contains("sendrequest")
}

pub(crate) fn anthropic_oauth_route_availability(model: &str) -> (bool, String) {
    if model.ends_with("[1m]") && !crate::usage::has_extra_usage() {
        (false, "requires extra usage".to_string())
    } else if model.contains("opus") && !crate::auth::claude::is_max_subscription() {
        (false, "requires Max subscription".to_string())
    } else {
        (true, String::new())
    }
}

pub(crate) fn anthropic_api_key_route_availability(model: &str) -> (bool, String) {
    if model.ends_with("[1m]") && !crate::usage::has_extra_usage() {
        (false, "requires extra usage".to_string())
    } else {
        (true, String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::is_transient_transport_error;

    #[test]
    fn http2_stream_protocol_error_is_transient() {
        // Exact shape reqwest/h2 surfaces for a reset on a reused HTTP/2 connection.
        let msg = "error sending request for url (https://api.anthropic.com/v1/messages): \
                   client error (SendRequest): http2 error: stream error received: \
                   unspecific protocol error detected";
        assert!(is_transient_transport_error(msg));
    }

    #[test]
    fn http2_goaway_and_refused_stream_are_transient() {
        assert!(is_transient_transport_error("http2 error: GOAWAY received"));
        assert!(is_transient_transport_error("stream error: REFUSED_STREAM"));
    }

    #[test]
    fn auth_errors_are_not_transient() {
        assert!(!is_transient_transport_error("401 unauthorized"));
        assert!(!is_transient_transport_error("invalid x-api-key"));
    }

    /// Real transport-error shapes harvested from ~/.jcode/logs.
    #[test]
    fn real_world_transport_errors_are_transient() {
        let real_errors = [
            "client error (Connect): dns error: failed to lookup address information: \
             Name or service not known",
            "client error (SendRequest): http2 error: keep-alive timed out: operation timed out",
            "client error (SendRequest): connection error: peer closed connection without \
             sending TLS close_notify: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof",
            "client error (Connect): operation timed out",
            "client error (SendRequest): connection error: timed out",
            "client error (Connect): tls handshake eof",
            "error decoding response body: request or response body error: operation timed out",
        ];
        for error in real_errors {
            assert!(
                is_transient_transport_error(error),
                "should be transient: {error}"
            );
        }
    }
}
