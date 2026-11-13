//! Bookmark files: the Netscape bookmark format every major browser imports
//! and exports.
//!
//! Reading builds a plain tree, which [`crate::storage::BookmarksDb::import`]
//! stores in one transaction; writing reads the stored tree. Neither touches
//! the database. [Bookmarks § Import and export](../../../docs/specs/bookmarks.md#import-and-export).

use std::fmt::Write as _;
use std::path::Path;

use crate::error::{HakuError, Result};
use crate::model::{is_bookmarkable, BookmarkKind, BookmarkNode, Bookmarks, ExportedBookmarks};

/// The largest file read, refused before reading. The reference file is
/// 74 kB for 82 links, icons included, so this holds tens of thousands.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The deepest folder kept. Whatever is below it goes into the folder at this
/// level, in order.
pub const MAX_DEPTH: usize = 64;

/// What every bookmark file starts with, Chrome's, Edge's and Firefox's alike.
const DOCTYPE: &str = "<!DOCTYPE NETSCAPE-Bookmark-file-1>";

/// The latest `ADD_DATE` read as a date, the last second of the year 9999.
/// Anything later is no date in seconds.
const LATEST_SECONDS: u64 = 253_402_300_799;

/// A link or folder read from a file, before it is stored.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Link {
        title: String,
        url: String,
        /// Milliseconds since the epoch, or nothing when the file has no
        /// valid date.
        added_at: Option<i64>,
    },
    Folder {
        title: String,
        added_at: Option<i64>,
        children: Vec<Entry>,
    },
}

/// A bookmark file's contents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BookmarkFile {
    /// What the folder marked `PERSONAL_TOOLBAR_FOLDER` held, for the end of
    /// **Bookmarks bar**.
    pub bar: Vec<Entry>,
    /// Everything else, in order.
    pub other: Vec<Entry>,
    /// Links of a scheme Haku does not import, and separators.
    pub skipped: u32,
}

/// Reads the bookmark file at `path`.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the file is larger than
/// [`MAX_BYTES`] or is not a bookmark file, and [`HakuError::Storage`] when it
/// cannot be read.
pub fn read(path: &Path) -> Result<BookmarkFile> {
    let bytes = match super::read_bounded(path, MAX_BYTES) {
        Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => return Err(too_large()),
        read => read?,
    };
    parse(&String::from_utf8_lossy(&bytes))
}

fn too_large() -> HakuError {
    HakuError::Unsupported(format!(
        "the file is larger than {} MB, the most a bookmark file may be",
        MAX_BYTES / 1024 / 1024
    ))
}

/// Reads a bookmark file's text.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the text is not a bookmark file.
pub fn parse(text: &str) -> Result<BookmarkFile> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if !text
        .get(..DOCTYPE.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(DOCTYPE))
    {
        return Err(HakuError::Unsupported("the file is not a bookmark file".into()));
    }
    let mut reader = Reader::default();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        reader.text(&rest[..start]);
        rest = &rest[start..];
        let (tag, after) = read_tag(rest);
        if let Some(tag) = tag {
            reader.tag(tag);
        }
        rest = after;
    }
    reader.text(rest);
    Ok(reader.finish())
}

/// A tag, with its name in upper case and its attributes' names too.
struct Tag {
    closing: bool,
    name: String,
    attributes: Vec<(String, String)>,
}

impl Tag {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Reads the tag `text` starts with, at its `<`, and returns it with what
/// follows it. A comment, a doctype or anything that is not a tag reads as
/// nothing.
fn read_tag(text: &str) -> (Option<Tag>, &str) {
    let skip_to = |end: &str| -> &str { text.find(end).map_or("", |at| &text[at + end.len()..]) };
    if text.starts_with("<!--") {
        return (None, skip_to("-->"));
    }
    if text.starts_with("<!") || text.starts_with("<?") {
        return (None, skip_to(">"));
    }
    let bytes = text.as_bytes();
    let mut at = 1;
    let closing = bytes.get(at) == Some(&b'/');
    if closing {
        at += 1;
    }
    let name_start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_alphanumeric) {
        at += 1;
    }
    if at == name_start {
        // A `<` that opens no tag is text.
        return (None, &text[1..]);
    }
    let name = text[name_start..at].to_ascii_uppercase();
    let mut attributes = Vec::new();
    loop {
        while bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/')
        {
            at += 1;
        }
        match bytes.get(at) {
            None => return (None, ""),
            Some(b'>') => {
                at += 1;
                break;
            }
            Some(_) => {}
        }
        let key_start = at;
        while bytes
            .get(at)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'=' | b'>' | b'/'))
        {
            at += 1;
        }
        let key = text[key_start..at].to_ascii_uppercase();
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let mut value = String::new();
        if bytes.get(at) == Some(&b'=') {
            at += 1;
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            let (raw, end) = match bytes.get(at) {
                Some(quote @ (b'"' | b'\'')) => {
                    let from = at + 1;
                    let to = text[from..]
                        .find(char::from(*quote))
                        .map_or(text.len(), |end| from + end);
                    (&text[from..to], (to + 1).min(text.len()))
                }
                _ => {
                    let from = at;
                    let mut to = at;
                    while bytes
                        .get(to)
                        .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
                    {
                        to += 1;
                    }
                    (&text[from..to], to)
                }
            };
            value = unescape(raw);
            at = end;
        }
        if key.is_empty() {
            if at == key_start {
                // Nothing a name could start with: step over it.
                at += 1;
            }
        } else {
            attributes.push((key, value));
        }
    }
    (
        Some(Tag {
            closing,
            name,
            attributes,
        }),
        &text[at.min(text.len())..],
    )
}

/// A folder being read.
struct Open {
    title: String,
    added_at: Option<i64>,
    children: Vec<Entry>,
    /// The `<DL>` depth its contents opened at.
    depth: usize,
    /// The folder marked `PERSONAL_TOOLBAR_FOLDER`, whose contents go to the bar.
    bar: bool,
}

/// A folder named by an `<H3>`, waiting for the `<DL>` that holds its contents.
enum Pending {
    Folder {
        title: String,
        added_at: Option<i64>,
        bar: bool,
    },
    /// Below [`MAX_DEPTH`]: its contents go into the folder it is in.
    Flattened,
}

/// The text of an `<A>` or `<H3>` being read.
struct Capture {
    tag: Tag,
    text: String,
}

#[derive(Default)]
struct Reader {
    file: BookmarkFile,
    /// The folders open, outermost first; the file's top level is outside them.
    open: Vec<Open>,
    top: Vec<Entry>,
    depth: usize,
    pending: Option<Pending>,
    capture: Option<Capture>,
    bar_seen: bool,
}

impl Reader {
    fn text(&mut self, text: &str) {
        if let Some(capture) = &mut self.capture {
            capture.text.push_str(text);
        }
    }

    fn tag(&mut self, tag: Tag) {
        let ends_capture = match (&self.capture, tag.closing, tag.name.as_str()) {
            (None, ..) => false,
            (Some(capture), true, name) => name == capture.tag.name || name == "DL",
            (Some(_), false, name) => matches!(name, "A" | "H3" | "DT" | "DL" | "HR"),
        };
        if ends_capture {
            self.end_capture();
        }
        match (tag.closing, tag.name.as_str()) {
            (false, "A" | "H3") => {
                self.capture = Some(Capture {
                    tag,
                    text: String::new(),
                })
            }
            (false, "HR") => {
                self.settle_pending();
                self.file.skipped += 1;
            }
            (false, "DL") => {
                self.depth += 1;
                if let Some(Pending::Folder { title, added_at, bar }) = self.pending.take() {
                    self.open.push(Open {
                        title,
                        added_at,
                        children: Vec::new(),
                        depth: self.depth,
                        bar,
                    });
                }
            }
            (true, "DL") => {
                self.settle_pending();
                if self.open.last().is_some_and(|open| open.depth == self.depth) {
                    self.close();
                }
                self.depth = self.depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn end_capture(&mut self) {
        let Some(Capture { tag, text }) = self.capture.take() else {
            return;
        };
        self.settle_pending();
        let title = unescape(&text);
        let added_at = tag.attribute("ADD_DATE").and_then(date);
        if tag.name == "A" {
            let url = tag.attribute("HREF").unwrap_or_default().trim();
            if is_bookmarkable(url) {
                self.children().push(Entry::Link {
                    title,
                    url: url.to_owned(),
                    added_at,
                });
            } else {
                self.file.skipped += 1;
            }
        } else if self.open.len() >= MAX_DEPTH {
            self.pending = Some(Pending::Flattened);
        } else {
            let bar = !self.bar_seen
                && self.open.is_empty()
                && tag
                    .attribute("PERSONAL_TOOLBAR_FOLDER")
                    .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            self.bar_seen |= bar;
            self.pending = Some(Pending::Folder { title, added_at, bar });
        }
    }

    /// A folder named but never given its contents is an empty folder.
    fn settle_pending(&mut self) {
        if let Some(Pending::Folder { title, added_at, bar }) = self.pending.take() {
            if !bar {
                self.children().push(Entry::Folder {
                    title,
                    added_at,
                    children: Vec::new(),
                });
            }
        }
    }

    fn children(&mut self) -> &mut Vec<Entry> {
        match self.open.last_mut() {
            Some(open) => &mut open.children,
            None => &mut self.top,
        }
    }

    fn close(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        if open.bar {
            self.file.bar.extend(open.children);
        } else {
            self.children().push(Entry::Folder {
                title: open.title,
                added_at: open.added_at,
                children: open.children,
            });
        }
    }

    fn finish(mut self) -> BookmarkFile {
        self.end_capture();
        self.settle_pending();
        while !self.open.is_empty() {
            self.close();
        }
        self.file.other = self.top;
        self.file
    }
}

/// An `ADD_DATE`, in seconds since the epoch, as milliseconds; nothing when it
/// is missing, not a number, zero, or past the year 9999.
fn date(value: &str) -> Option<i64> {
    let seconds: u64 = value.trim().parse().ok()?;
    (1..=LATEST_SECONDS).contains(&seconds).then(|| (seconds * 1000) as i64)
}

/// Decodes character references: the named ones bookmark files use, and
/// numeric ones. Anything else is kept as written.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        decoded.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded_reference = rest[1..]
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| Some((reference(&rest[1..=end])?, end + 2)));
        match decoded_reference {
            Some((character, length)) => {
                decoded.push(character);
                rest = &rest[length..];
            }
            None => {
                decoded.push('&');
                rest = &rest[1..];
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

/// The character a reference's name, between `&` and `;`, stands for.
fn reference(name: &str) -> Option<char> {
    let number = |digits: &str, radix| u32::from_str_radix(digits, radix).ok().and_then(char::from_u32);
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => match name.strip_prefix('#')? {
            hex if hex.starts_with(['x', 'X']) => number(&hex[1..], 16),
            decimal => number(decimal, 10),
        },
    }
}

/// Writes the tree as Chrome writes a bookmark file, and counts the links and
/// folders written, the roots aside: **Bookmarks bar** as a folder marked
/// `PERSONAL_TOOLBAR_FOLDER`, then what **Other bookmarks** holds, at the top
/// level, where every browser's import reads it as not on the bar.
pub fn write(tree: &Bookmarks) -> (String, ExportedBookmarks) {
    let mut out = String::from(concat!(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\r\n",
        "<!-- This is an automatically generated file.\r\n",
        "     It will be read and overwritten.\r\n",
        "     DO NOT EDIT! -->\r\n",
        "<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\r\n",
        "<TITLE>Bookmarks</TITLE>\r\n",
        "<H1>Bookmarks</H1>\r\n",
        "<DL><p>\r\n",
    ));
    let mut written = ExportedBookmarks::default();
    write_folder(&mut out, &tree.bar, 1, true, &mut written);
    write_children(&mut out, &tree.other, 1, &mut written);
    out.push_str("</DL><p>\r\n");
    (out, written)
}

fn write_folder(out: &mut String, folder: &BookmarkNode, level: usize, bar: bool, written: &mut ExportedBookmarks) {
    let indent = "    ".repeat(level);
    let _ = write!(
        out,
        "{indent}<DT><H3 ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\"{}>{}</H3>\r\n{indent}<DL><p>\r\n",
        folder.added_at / 1000,
        folder.modified_at / 1000,
        if bar { " PERSONAL_TOOLBAR_FOLDER=\"true\"" } else { "" },
        escape(&folder.title),
    );
    write_children(out, folder, level + 1, written);
    let _ = write!(out, "{indent}</DL><p>\r\n");
}

fn write_children(out: &mut String, folder: &BookmarkNode, level: usize, written: &mut ExportedBookmarks) {
    let BookmarkKind::Folder { children } = &folder.kind else {
        return;
    };
    for child in children {
        match &child.kind {
            BookmarkKind::Link { url } => {
                let _ = write!(
                    out,
                    "{}<DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\r\n",
                    "    ".repeat(level),
                    escape(url),
                    child.added_at / 1000,
                    escape(&child.title),
                );
                written.links += 1;
            }
            BookmarkKind::Folder { .. } => {
                written.folders += 1;
                write_folder(out, child, level, false, written);
            }
        }
    }
}

/// Escapes text for an element's content or a quoted attribute.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BookmarkId;

    fn titles(entries: &[Entry]) -> Vec<&str> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Link { title, .. } | Entry::Folder { title, .. } => title.as_str(),
            })
            .collect()
    }

    fn children(entry: &Entry) -> &[Entry] {
        match entry {
            Entry::Folder { children, .. } => children,
            Entry::Link { .. } => panic!("not a folder"),
        }
    }

    /// Links and folders in `entries`, at every depth.
    fn count(entries: &[Entry]) -> (usize, usize) {
        entries.iter().fold((0, 0), |(links, folders), entry| match entry {
            Entry::Link { .. } => (links + 1, folders),
            Entry::Folder { children, .. } => {
                let (inner_links, inner_folders) = count(children);
                (links + inner_links, folders + 1 + inner_folders)
            }
        })
    }

    /// A file shaped as the reference export is, written as Chrome writes one:
    /// the bar holding 6 folders, which hold 82 links and 27 more folders, each
    /// link with an icon. Generated, so the user's own file stays out of the
    /// repository.
    fn reference_shaped() -> String {
        let mut out = String::from(concat!(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\r\n<!-- This is an automatically generated file.\r\n",
            "     It will be read and overwritten.\r\n     DO NOT EDIT! -->\r\n",
            "<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\r\n",
            "<TITLE>Bookmarks</TITLE>\r\n<H1>Bookmarks</H1>\r\n<DL><p>\r\n",
            "    <DT><H3 ADD_DATE=\"1790874769\" LAST_MODIFIED=\"1790874800\" PERSONAL_TOOLBAR_FOLDER=\"true\">Bookmarks bar</H3>\r\n",
            "    <DL><p>\r\n",
        ));
        let mut link = 0;
        let mut add_links = |out: &mut String, indent: &str, count: usize| {
            for _ in 0..count {
                link += 1;
                let _ = write!(
                    out,
                    "{indent}<DT><A HREF=\"https://site{link}.test/page?a=1&amp;b=2\" ADD_DATE=\"1790874{link:03}\" ICON=\"data:image/png;base64,iVBORw0KGgo=\">Site {link} &amp; more</A>\r\n"
                );
            }
        };
        for folder in 0..6 {
            let _ = write!(
                out,
                "        <DT><H3 ADD_DATE=\"1790874770\" LAST_MODIFIED=\"1790874771\">Folder {folder}</H3>\r\n        <DL><p>\r\n"
            );
            add_links(&mut out, "            ", 7);
            // 27 subfolders in all: 5 in the first three folders, 4 in the rest.
            for sub in 0..if folder < 3 { 5 } else { 4 } {
                let _ = write!(
                    out,
                    "            <DT><H3 ADD_DATE=\"1790874772\" LAST_MODIFIED=\"1790874773\">Sub {folder}.{sub}</H3>\r\n            <DL><p>\r\n"
                );
                add_links(&mut out, "                ", 1);
                out.push_str("            </DL><p>\r\n");
            }
            out.push_str("        </DL><p>\r\n");
        }
        add_links(&mut out, "        ", 82 - 6 * 7 - 27);
        out.push_str("    </DL><p>\r\n</DL><p>\r\n");
        out
    }

    #[test]
    fn the_reference_shape_arrives_in_the_bar() {
        let text = reference_shaped();
        assert_eq!(text.matches("<A ").count(), 82);
        assert_eq!(text.matches("<H3 ").count(), 34);

        let file = parse(&text).unwrap();

        assert_eq!(count(&file.bar), (82, 33));
        assert!(file.other.is_empty());
        assert_eq!(file.skipped, 0);
        assert_eq!(
            titles(&file.bar)[..7],
            [
                "Folder 0",
                "Folder 1",
                "Folder 2",
                "Folder 3",
                "Folder 4",
                "Folder 5",
                "Site 70 & more"
            ]
        );
        let Entry::Link { title, url, added_at } = &children(&file.bar[0])[0] else {
            panic!("expected a link");
        };
        assert_eq!(title, "Site 1 & more");
        assert_eq!(url, "https://site1.test/page?a=1&b=2");
        assert_eq!(*added_at, Some(1_790_874_001_000));
    }

    /// Reads the user's own export, kept out of the repository, when
    /// `HAKU_BOOKMARKS_FILE` names it: `pnpm test:rust -- bookmarks_html --ignored`.
    #[test]
    #[ignore = "needs a local bookmark file"]
    fn a_local_reference_file() {
        let path = std::env::var("HAKU_BOOKMARKS_FILE").expect("HAKU_BOOKMARKS_FILE names the file");
        let file = read(Path::new(&path)).unwrap();
        assert_eq!(count(&file.bar), (82, 33));
        assert!(file.other.is_empty());
        assert_eq!(file.skipped, 0);
    }

    const FIREFOX: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<!-- This is an automatically generated file.
     It will be read and overwritten.
     DO NOT EDIT! -->
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'self'; script-src 'none'; img-src data: *; object-src 'none'"></meta>
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks Menu</H1>

<DL><p>
    <DT><H3 ADD_DATE="1700000000" LAST_MODIFIED="1700000100">Mozilla Firefox</H3>
    <DL><p>
        <DT><A HREF="https://support.mozilla.org/products/firefox" ADD_DATE="1700000001" LAST_MODIFIED="1700000001" ICON_URI="fake-favicon-uri:https://support.mozilla.org/products/firefox">Get Help</A>
    </DL><p>
    <HR>    <DT><A HREF="place:parent=menu________&amp;queryType=1&amp;sort=12&amp;maxResults=10" ADD_DATE="1700000002">Recent Tags</A>
    <DT><H3 ADD_DATE="1700000003" LAST_MODIFIED="1700000004" PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Toolbar</H3>
    <DL><p>
        <DT><A HREF="https://www.mozilla.org/about/" ADD_DATE="1700000005" LAST_MODIFIED="1700000005" SHORTCUTURL="moz" TAGS="a,b">About Us</A>
<DD>A description, not a title
        <DT><A HREF="javascript:void(document.body.innerHTML='x')" ADD_DATE="1700000006">Bookmarklet</A>
    </DL><p>
    <DT><H3 ADD_DATE="1700000007" LAST_MODIFIED="1700000008" UNFILED_BOOKMARKS_FOLDER="true">Other Bookmarks</H3>
    <DL><p>
        <DT><A HREF="data:text/html,hi" ADD_DATE="1700000009">Data</A>
        <DT><A HREF="file:///C:/notes.html">Notes</A>
    </DL><p>
</DL>
"#;

    #[test]
    fn a_firefox_export() {
        let file = parse(FIREFOX).unwrap();
        assert_eq!(titles(&file.bar), ["About Us"]);
        assert_eq!(titles(&file.other), ["Mozilla Firefox", "Other Bookmarks"]);
        assert_eq!(titles(children(&file.other[0])), ["Get Help"]);
        assert_eq!(titles(children(&file.other[1])), ["Notes"]);
        let Entry::Link { added_at, .. } = &children(&file.other[1])[0] else {
            panic!("expected a link");
        };
        assert_eq!(*added_at, None);
        // The separator, the place: query, the bookmarklet and the data: URL.
        assert_eq!(file.skipped, 4);
    }

    const EDGE: &str = "<!DOCTYPE NETSCAPE-Bookmark-file-1>\r\n<!-- This is an automatically generated file.\r\n     It will be read and overwritten.\r\n     DO NOT EDIT! -->\r\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\r\n<TITLE>Bookmarks</TITLE>\r\n<H1>Bookmarks</H1>\r\n<DL><p>\r\n    <DT><H3 ADD_DATE=\"1700000000\" LAST_MODIFIED=\"1700000001\" PERSONAL_TOOLBAR_FOLDER=\"true\">Favorites bar</H3>\r\n    <DL><p>\r\n        <DT><A HREF=\"https://www.bing.com/\" ADD_DATE=\"1700000002\" ICON=\"data:image/png;base64,AAAA\">Bing</A>\r\n        <DT><H3 ADD_DATE=\"1700000003\" LAST_MODIFIED=\"0\">Empty</H3>\r\n        <DL><p>\r\n        </DL><p>\r\n    </DL><p>\r\n    <DT><A HREF=\"https://learn.microsoft.com/\" ADD_DATE=\"1700000004\">Learn</A>\r\n    <DT><H3 ADD_DATE=\"1700000005\" LAST_MODIFIED=\"1700000006\">Mobile favorites</H3>\r\n    <DL><p>\r\n        <DT><A HREF=\"https://m.example.test/\" ADD_DATE=\"1700000007\">Phone</A>\r\n    </DL><p>\r\n</DL><p>\r\n";

    #[test]
    fn an_edge_export() {
        let file = parse(EDGE).unwrap();
        assert_eq!(titles(&file.bar), ["Bing", "Empty"]);
        assert!(children(&file.bar[1]).is_empty());
        assert_eq!(titles(&file.other), ["Learn", "Mobile favorites"]);
        assert_eq!(titles(children(&file.other[1])), ["Phone"]);
        assert_eq!(file.skipped, 0);
    }

    #[test]
    fn only_web_and_file_links_are_kept() {
        let file = parse(concat!(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>",
            "<DT><A HREF=\"javascript:alert(1)\">a</A>",
            "<DT><A HREF=\"data:text/html,x\">b</A>",
            "<DT><A HREF=\"chrome://settings\">c</A>",
            "<DT><A>no address</A>",
            "<DT><A HREF=\"HTTPS://EXAMPLE.TEST/\">kept</A>",
            "<HR>",
            "</DL>",
        ))
        .unwrap();
        assert_eq!(titles(&file.other), ["kept"]);
        assert_eq!(file.skipped, 5);
    }

    #[test]
    fn a_file_that_is_not_a_bookmark_file_is_refused() {
        for text in [
            "",
            "<html><body><a href=\"https://a.test/\">a</a></body></html>",
            "{\"roots\": {}}",
        ] {
            assert!(matches!(parse(text), Err(HakuError::Unsupported(_))), "{text}");
        }
        // A byte order mark and leading space are allowed, in any case.
        assert!(parse("\u{feff}\r\n<!doctype netscape-bookmark-file-1>").is_ok());
    }

    #[test]
    fn a_file_over_the_limit_is_refused_before_it_is_read() {
        let path = std::env::temp_dir().join(format!("haku-bookmarks-large-{}.html", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        // Sparse where the file system allows it: its length is all that is read.
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);
        let read = read(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(read, Err(HakuError::Unsupported(message)) if message.contains("32 MB")));
    }

    #[test]
    fn missing_and_invalid_dates_read_as_none() {
        assert_eq!(date("1790874769"), Some(1_790_874_769_000));
        for value in ["", "0", "-5", "abc", "1.5", "253402300800", "99999999999999999999"] {
            assert_eq!(date(value), None, "{value}");
        }
    }

    #[test]
    fn folders_below_the_64th_level_are_flattened_into_it() {
        let mut text = String::from("<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>");
        for level in 1..=70 {
            let _ = write!(
                text,
                "<DT><H3>Level {level}</H3><DL><p><DT><A HREF=\"https://a.test/{level}\">Link {level}</A>"
            );
        }
        text.push_str(&"</DL><p>".repeat(70));
        text.push_str("<DT><A HREF=\"https://a.test/after\">After</A></DL>");

        let file = parse(&text).unwrap();

        assert_eq!(titles(&file.other), ["Level 1", "After"]);
        let mut folder = &file.other[0];
        for level in 2..=MAX_DEPTH {
            folder = children(folder)
                .iter()
                .find(|entry| matches!(entry, Entry::Folder { .. }))
                .unwrap();
            assert_eq!(titles(std::slice::from_ref(folder)), [format!("Level {level}")]);
        }
        assert_eq!(
            titles(children(folder)),
            ["Link 64", "Link 65", "Link 66", "Link 67", "Link 68", "Link 69", "Link 70"]
        );
    }

    #[test]
    fn deeply_nested_lists_do_not_exhaust_the_stack() {
        let depth = 200_000;
        let text = format!(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>{}<DT><H3>Deep</H3><DL><DT><A HREF=\"https://a.test/\">a</A></DL>{}",
            "<DL>".repeat(depth),
            "</DL>".repeat(depth)
        );
        let file = parse(&text).unwrap();
        assert_eq!(count(&file.other), (1, 1));
    }

    #[test]
    fn references_are_decoded_and_broken_ones_kept() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#x27; &apos;"),
            "a & b <c> \"d\" 'e' '"
        );
        assert_eq!(unescape("&#233;t&#xE9; &nbsp;"), "été \u{a0}");
        assert_eq!(
            unescape("AT&T &unknown; & &#xZZ; &#99999999;"),
            "AT&T &unknown; & &#xZZ; &#99999999;"
        );
    }

    #[test]
    fn attributes_are_read_in_any_case_and_quoting() {
        let file = parse(concat!(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1><dl><dt>",
            "<a add_date=1700000000 href='https://a.test/?q=\"x\">y'>Single</a>",
            "<dt><A\nHREF=\"https://b.test/\"\nADD_DATE=\"1700000001\">Split <B>bold</B> lines</A>",
            "</dl>",
        ))
        .unwrap();
        assert_eq!(
            file.other,
            [
                Entry::Link {
                    title: "Single".into(),
                    url: "https://a.test/?q=\"x\">y".into(),
                    added_at: Some(1_700_000_000_000),
                },
                Entry::Link {
                    title: "Split bold lines".into(),
                    url: "https://b.test/".into(),
                    added_at: Some(1_700_000_001_000),
                },
            ]
        );
    }

    fn node(id: i64, title: &str, added_at: i64, kind: BookmarkKind) -> BookmarkNode {
        BookmarkNode {
            id: BookmarkId(id),
            title: title.into(),
            added_at,
            modified_at: added_at + 5_000,
            kind,
        }
    }

    fn link(id: i64, title: &str, url: &str) -> BookmarkNode {
        node(
            id,
            title,
            1_700_000_000_000 + id * 1000,
            BookmarkKind::Link { url: url.into() },
        )
    }

    fn folder(id: i64, title: &str, children: Vec<BookmarkNode>) -> BookmarkNode {
        node(id, title, 1_700_000_000_000, BookmarkKind::Folder { children })
    }

    fn sample() -> Bookmarks {
        Bookmarks {
            bar: folder(
                1,
                "Bookmarks bar",
                vec![
                    link(3, "Tom & Jerry <\"quoted\"> 'it's'", "https://a.test/?x=1&y=\"2\"<3>"),
                    folder(4, "Empty", vec![]),
                    folder(
                        5,
                        "Nested",
                        vec![folder(6, "Inner", vec![link(7, "Deep", "file:///C:/notes.html")])],
                    ),
                ],
            ),
            other: folder(2, "Other bookmarks", vec![link(8, "Other", "https://other.test/")]),
        }
    }

    #[test]
    fn titles_and_addresses_are_escaped() {
        let (written, counted) = write(&sample());
        assert_eq!(counted, ExportedBookmarks { links: 3, folders: 3 });
        assert!(written.contains(
            "<A HREF=\"https://a.test/?x=1&amp;y=&quot;2&quot;&lt;3&gt;\" ADD_DATE=\"1700000003\">Tom &amp; Jerry &lt;&quot;quoted&quot;&gt; &#39;it&#39;s&#39;</A>"
        ));
        assert!(written.contains(
            "<DT><H3 ADD_DATE=\"1700000000\" LAST_MODIFIED=\"1700000005\" PERSONAL_TOOLBAR_FOLDER=\"true\">Bookmarks bar</H3>"
        ));
        assert!(!written.contains("ICON"));
    }

    /// What a stored tree reads back as.
    fn entries(nodes: &[BookmarkNode]) -> Vec<Entry> {
        nodes
            .iter()
            .map(|node| match &node.kind {
                BookmarkKind::Link { url } => Entry::Link {
                    title: node.title.clone(),
                    url: url.clone(),
                    added_at: Some(node.added_at),
                },
                BookmarkKind::Folder { children } => Entry::Folder {
                    title: node.title.clone(),
                    added_at: Some(node.added_at),
                    children: entries(children),
                },
            })
            .collect()
    }

    fn held(node: &BookmarkNode) -> &[BookmarkNode] {
        match &node.kind {
            BookmarkKind::Folder { children } => children,
            BookmarkKind::Link { .. } => &[],
        }
    }

    #[test]
    fn a_written_tree_reads_back_the_same() {
        let tree = sample();
        let file = parse(&write(&tree).0).unwrap();
        assert_eq!(file.bar, entries(held(&tree.bar)));
        assert_eq!(file.other, entries(held(&tree.other)));
        assert_eq!(file.skipped, 0);
    }
}
