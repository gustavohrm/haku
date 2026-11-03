//! The requests Haku makes itself, rather than a page: today only the search
//! engine's completions for the address bar.
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
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent("Haku")
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent.get(url).call().ok()?;
    let bytes = response.body_mut().with_config().limit(BODY_LIMIT).read_to_vec().ok()?;
    Some(String::from_utf8(bytes).unwrap_or_else(|error| error.into_bytes().into_iter().map(char::from).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::search::{search_url, PRESET_ENGINES};
    use crate::model::suggest::parse_open_search;

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
