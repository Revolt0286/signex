//! Shared blocking-HTTP client construction for the distributor adapters.
//!
//! Every adapter builds its `reqwest::blocking::Client` through
//! [`client_builder`] so the connect and overall-request timeouts are
//! defined once. Without them a stalled distributor endpoint would hang
//! the calling thread forever.

use std::time::Duration;

/// Maximum time to establish the TCP/TLS connection.
pub const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum time for one whole request (connect + send + response body).
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const USER_AGENT: &str = "signex-library/0.9 (+https://signex.dev)";

/// A client builder preloaded with the user agent and both timeouts.
/// Callers add adapter-specific options (e.g. redirect policy), then pass
/// it to [`finish`].
pub fn client_builder() -> reqwest::blocking::ClientBuilder {
    builder_with_timeouts(HTTP_CONNECT_TIMEOUT, HTTP_REQUEST_TIMEOUT)
}

fn builder_with_timeouts(connect: Duration, request: Duration) -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(connect)
        .timeout(request)
}

/// Default adapter client: [`client_builder`] with no extra options.
pub fn build_client() -> reqwest::blocking::Client {
    finish(client_builder())
}

/// Build the client. Failure only occurs when the platform TLS backend
/// cannot initialise, which no caller can recover from.
pub fn finish(builder: reqwest::blocking::ClientBuilder) -> reqwest::blocking::Client {
    builder
        .build()
        .expect("reqwest::blocking::Client::build fails only if the TLS backend cannot initialise")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::Instant;

    #[test]
    fn built_client_times_out_against_a_server_that_never_answers() {
        // Accepts the connection, reads the request, never responds.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                std::thread::sleep(Duration::from_secs(5));
            }
        });
        // Same code path as `client_builder`, with a short overall timeout.
        let client = finish(builder_with_timeouts(
            Duration::from_secs(2),
            Duration::from_millis(300),
        ));
        let t0 = Instant::now();
        let err = client.get(format!("http://{addr}/")).send().unwrap_err();
        assert!(err.is_timeout(), "expected timeout, got {err}");
        assert!(t0.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn default_builder_carries_bounded_timeouts() {
        assert!(HTTP_CONNECT_TIMEOUT <= HTTP_REQUEST_TIMEOUT);
        assert!(HTTP_REQUEST_TIMEOUT <= Duration::from_secs(120));
    }
}
