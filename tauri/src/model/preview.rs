//! What a tab showed as it was left, to cover its page while it reloads.
//!
//! Held in memory only and never written to disk: a capture shows another
//! tab's page, and a stale one is worth nothing after a restart.

use super::tab::TabId;

/// Captures held at once. Each is a JPEG of a viewport, typically a few
/// hundred kilobytes.
pub const PREVIEW_LIMIT: usize = 20;

/// The latest capture of each tab, the least recently shown dropped first.
#[derive(Debug, Default)]
pub struct Previews {
    /// Oldest first. A capture is taken as its tab stops being shown, so the
    /// order is also the order the tabs were last shown in.
    entries: Vec<(TabId, Vec<u8>)>,
}

impl Previews {
    /// Keeps a tab's capture, replacing any it had.
    pub fn insert(&mut self, tab: TabId, jpeg: Vec<u8>) {
        self.entries.retain(|(id, _)| *id != tab);
        self.entries.push((tab, jpeg));
        let excess = self.entries.len().saturating_sub(PREVIEW_LIMIT);
        self.entries.drain(..excess);
    }

    pub fn get(&self, tab: TabId) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(id, _)| *id == tab)
            .map(|(_, jpeg)| jpeg.as_slice())
    }

    /// Drops the captures of tabs that no longer exist.
    pub fn retain(&mut self, exists: impl Fn(TabId) -> bool) {
        self.entries.retain(|(id, _)| exists(*id));
    }
}

/// A JPEG as a `data:` URL, which an `<img>` in the interface can show
/// without any protocol a content webview could also reach.
pub fn jpeg_data_url(jpeg: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut url = String::with_capacity(23 + jpeg.len().div_ceil(3) * 4);
    url.push_str("data:image/jpeg;base64,");
    for chunk in jpeg.chunks(3) {
        let bytes = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let triple = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                url.push(ALPHABET[(triple >> (18 - 6 * index) & 0x3f) as usize] as char);
            } else {
                url.push('=');
            }
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_capture_replaces_the_tabs_last_one() {
        let mut previews = Previews::default();
        previews.insert(TabId(1), vec![1]);
        previews.insert(TabId(1), vec![2]);
        assert_eq!(previews.get(TabId(1)), Some(&[2][..]));
    }

    #[test]
    fn the_least_recently_shown_is_dropped_past_the_limit() {
        let mut previews = Previews::default();
        for id in 0..=PREVIEW_LIMIT as u64 {
            previews.insert(TabId(id), vec![0]);
        }
        assert!(previews.get(TabId(0)).is_none());
        assert!(previews.get(TabId(PREVIEW_LIMIT as u64)).is_some());
    }

    #[test]
    fn a_tab_shown_again_moves_to_the_back_of_the_queue() {
        let mut previews = Previews::default();
        for id in 0..PREVIEW_LIMIT as u64 {
            previews.insert(TabId(id), vec![0]);
        }
        previews.insert(TabId(0), vec![1]);
        previews.insert(TabId(99), vec![0]);
        assert!(previews.get(TabId(0)).is_some());
        assert!(previews.get(TabId(1)).is_none());
    }

    #[test]
    fn a_closed_tabs_capture_is_dropped() {
        let mut previews = Previews::default();
        previews.insert(TabId(1), vec![0]);
        previews.insert(TabId(2), vec![0]);
        previews.retain(|id| id == TabId(2));
        assert!(previews.get(TabId(1)).is_none());
        assert!(previews.get(TabId(2)).is_some());
    }

    #[test]
    fn a_data_url_encodes_base64_with_padding() {
        assert_eq!(jpeg_data_url(b""), "data:image/jpeg;base64,");
        assert_eq!(jpeg_data_url(b"f"), "data:image/jpeg;base64,Zg==");
        assert_eq!(jpeg_data_url(b"fo"), "data:image/jpeg;base64,Zm8=");
        assert_eq!(jpeg_data_url(b"foo"), "data:image/jpeg;base64,Zm9v");
        assert_eq!(jpeg_data_url(b"foobar"), "data:image/jpeg;base64,Zm9vYmFy");
        assert_eq!(jpeg_data_url(&[0xff, 0xd8, 0xff]), "data:image/jpeg;base64,/9j/");
    }
}
