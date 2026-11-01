//! Finding text in a page. [Find, print and save](../../../docs/specs/find-print-save.md).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};

/// The longest query the find bar may send, in characters.
pub const FIND_QUERY_MAX: usize = 1_000;

/// Where a find moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FindDirection {
    /// Starts a new search, at the first match.
    First,
    Next,
    Previous,
}

/// Refuses a query longer than [`FIND_QUERY_MAX`].
///
/// # Errors
/// Returns [`HakuError::Unsupported`], as for any invalid argument, for a
/// longer query.
pub fn check_query(query: &str) -> Result<()> {
    if query.chars().count() > FIND_QUERY_MAX {
        return Err(HakuError::Unsupported(format!(
            "invalid find query: longer than {FIND_QUERY_MAX} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_up_to_the_limit_is_accepted_and_a_longer_one_refused() {
        assert!(check_query(&"é".repeat(FIND_QUERY_MAX)).is_ok());
        assert!(matches!(
            check_query(&"a".repeat(FIND_QUERY_MAX + 1)),
            Err(HakuError::Unsupported(_))
        ));
    }
}
