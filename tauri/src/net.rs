//! The requests Haku makes itself, rather than a page: the search engine's
//! completions for the address bar, and an extension the user adds from the
//! Chrome Web Store.
//!
//! No cookie store and no shared state, so a request carries nothing that
//! ties it to the user's session with the engine.
//! [Address bar § Approved decisions](../../docs/specs/address-bar.md#approved-decisions), E and F.

use std::time::Duration;

/// The longest the engine is waited on for completions. Past it the list
/// shows without them, as the user has likely typed on.
pub const SUGGESTIONS_TIMEOUT: Duration = Duration::from_secs(1);

/// The most of an answer that is read. Completions are a few hundred bytes;
/// anything far larger is not an answer to use.
const BODY_LIMIT: u64 = 64 * 1024;

/// Fetches `url` as text, waiting at most `timeout` for all of it. Blocks.
///
/// Read as UTF-8, or as Latin-1 when it is not, as some engines answer in.
///
/// @returns The body, or nothing on any failure, including a status other than success.
pub fn fetch_text(url: &str, timeout: Duration) -> Option<String> {
    let bytes = fetch_bytes(url, timeout, BODY_LIMIT).ok()?;
    Some(String::from_utf8(bytes).unwrap_or_else(|error| error.into_bytes().into_iter().map(char::from).collect()))
}

/// Fetches `url`, following redirects, reading at most `limit` bytes and
/// waiting at most `timeout` for all of it. Blocks.
///
/// # Errors
/// Fails, with what went wrong, on any failure, including a status other
/// than success and a body larger than `limit`.
pub fn fetch_bytes(url: &str, timeout: Duration, limit: u64) -> Result<Vec<u8>, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent("Haku")
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent.get(url).call().map_err(|error| error.to_string())?;
    response
        .body_mut()
        .with_config()
        // ureq refuses a body as long as its limit, so it is given one more.
        .limit(limit.saturating_add(1))
        .read_to_vec()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::search::{search_url, PRESET_ENGINES};
    use crate::model::suggest::parse_open_search;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Serves one connection for each answer, in order, on a local port, and
    /// answers that port's address.
    fn serve(answers: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for answer in answers {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                stream.write_all(answer.as_bytes()).unwrap();
            }
        });
        address
    }

    fn answer(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}
Connection: close
Content-Length: {}
{headers}
{body}",
            body.len()
        )
    }

    #[test]
    fn bytes_follow_a_redirect() {
        let target = serve(vec![answer("200 OK", "", "extension")]);
        let start = serve(vec![answer(
            "302 Found",
            &format!(
                "Location: {target}/file
"
            ),
            "",
        )]);
        assert_eq!(fetch_bytes(&start, SUGGESTIONS_TIMEOUT * 5, 64).unwrap(), b"extension");
    }

    #[test]
    fn bytes_fail_on_a_status_other_than_success() {
        let address = serve(vec![answer("404 Not Found", "", "")]);
        assert!(fetch_bytes(&address, SUGGESTIONS_TIMEOUT * 5, 64).is_err());
    }

    #[test]
    fn bytes_fail_past_the_limit_and_not_at_it() {
        let address = serve(vec![
            answer("200 OK", "", "0123456789"),
            answer("200 OK", "", "0123456789"),
        ]);
        assert!(fetch_bytes(&address, SUGGESTIONS_TIMEOUT * 5, 9).is_err());
        assert_eq!(
            fetch_bytes(&address, SUGGESTIONS_TIMEOUT * 5, 10).unwrap(),
            b"0123456789"
        );
    }

    /// Asks every preset with a suggestion URL for completions, as a check by
    /// hand that each still answers without cookies. Needs the network, so it
    /// runs only when asked: `cargo test -- --ignored every_preset`.
    #[test]
    #[ignore = "needs the network"]
    fn every_preset_with_a_suggestion_url_answers() {
        for preset in PRESET_ENGINES {
            let Some(template) = preset.suggest_url else {
                continue;
            };
            let body = fetch_text(&search_url(template, "haku"), SUGGESTIONS_TIMEOUT * 3);
            let completions = body.as_deref().map(parse_open_search).unwrap_or_default();
            assert!(!completions.is_empty(), "{} answered {body:?}", preset.name);
        }
    }
}
