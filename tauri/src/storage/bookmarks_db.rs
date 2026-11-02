//! The bookmark tree.
//!
//! A file of its own rather than a table beside history: bookmarks are the
//! user's own work, kept until the user deletes them, and must survive history
//! being cleared or set aside. [Bookmarks § Storage](../../../docs/specs/bookmarks.md#storage).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::{HakuError, Result};
use crate::model::{is_bookmarkable, BookmarkId, BookmarkKind, BookmarkNode, Bookmarks};
use crate::storage::sqlite;

/// Schema revisions, applied in order by [`sqlite::open`].
///
/// A row with no `url` is a folder. The two roots are the rows with a `root`,
/// and have no parent. `position` orders a folder's rows from 0, with no gaps.
const MIGRATIONS: &[&str] = &["CREATE TABLE bookmarks (
        id          INTEGER PRIMARY KEY,
        parent      INTEGER,
        position    INTEGER NOT NULL,
        root        TEXT    UNIQUE,
        url         TEXT,
        title       TEXT    NOT NULL,
        added_at    INTEGER NOT NULL,
        modified_at INTEGER NOT NULL
    );
    CREATE INDEX idx_bookmarks_parent ON bookmarks (parent, position);
    INSERT INTO bookmarks (parent, position, root, url, title, added_at, modified_at) VALUES
        (NULL, 0, 'bar',   NULL, 'Bookmarks bar',   CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000),
        (NULL, 1, 'other', NULL, 'Other bookmarks', CAST(strftime('%s', 'now') AS INTEGER) * 1000, CAST(strftime('%s', 'now') AS INTEGER) * 1000);"];

pub struct BookmarksDb {
    connection: Connection,
}

/// One stored row, before the tree is built.
struct Row {
    id: i64,
    parent: Option<i64>,
    root: Option<String>,
    url: Option<String>,
    title: String,
    added_at: i64,
    modified_at: i64,
}

impl BookmarksDb {
    /// Opens the database at `path`, setting a damaged file aside first, as
    /// [`sqlite::open_or_set_aside`] does.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when the file cannot be opened, moved
    /// aside, or a migration fails.
    pub fn open_or_set_aside(path: &Path) -> Result<Self> {
        Ok(Self {
            connection: sqlite::open_or_set_aside(path, MIGRATIONS)?,
        })
    }

    /// Opens the database at `path` as [`Self::open_or_set_aside`] does, or
    /// keeps bookmarks in memory for this run when the file cannot be opened,
    /// leaving the file untouched for the next run. The failure is returned
    /// alongside, for the user to be told once a window can show it.
    ///
    /// # Panics
    /// Panics if SQLite cannot create an in-memory database with the fixed
    /// schema, which leaves nothing to fall back to.
    pub fn open_or_in_memory(path: &Path) -> (Self, Option<HakuError>) {
        match Self::open_or_set_aside(path) {
            Ok(database) => (database, None),
            Err(error) => (
                Self::in_memory().expect("an in-memory bookmarks database could not be created"),
                Some(HakuError::Storage(format!(
                    "bookmarks could not be opened from {}, so they are not shown, and changes made now will not be kept: {error}",
                    PathBuf::from(path).display()
                ))),
            ),
        }
    }

    /// In-memory database, for tests and for a file that cannot be opened.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when migration fails.
    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            connection: sqlite::in_memory(MIGRATIONS)?,
        })
    }

    /// The whole tree.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when the query fails or a root is
    /// missing.
    pub fn tree(&self) -> Result<Bookmarks> {
        let mut statement = self.connection.prepare(
            "SELECT id, parent, root, url, title, added_at, modified_at FROM bookmarks ORDER BY parent, position",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(Row {
                    id: row.get(0)?,
                    parent: row.get(1)?,
                    root: row.get(2)?,
                    url: row.get(3)?,
                    title: row.get(4)?,
                    added_at: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut roots = HashMap::new();
        let mut children: HashMap<i64, Vec<Row>> = HashMap::new();
        for row in rows {
            match (&row.root, row.parent) {
                (Some(root), _) => {
                    roots.insert(root.clone(), row);
                }
                (None, Some(parent)) => children.entry(parent).or_default().push(row),
                // A row with neither is unreachable; it is never written.
                (None, None) => {}
            }
        }
        let mut root = |name: &str| {
            roots
                .remove(name)
                .map(|row| build(row, &mut children))
                .ok_or_else(|| HakuError::Storage(format!("the {name} bookmarks root is missing")))
        };
        Ok(Bookmarks {
            bar: root("bar")?,
            other: root("other")?,
        })
    }

    /// Adds a link to `parent` at `index`, or at its end when `index` is
    /// nothing or past it.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] when `parent` is not a folder or
    /// `url` may not be bookmarked, and [`HakuError::Storage`] when the write
    /// fails.
    pub fn add_link(
        &mut self,
        parent: BookmarkId,
        index: Option<u32>,
        title: &str,
        url: &str,
        now: i64,
    ) -> Result<BookmarkId> {
        check_url(url)?;
        self.insert(parent, index, title, Some(url), now)
    }

    /// Adds an empty folder to `parent`, placed as [`Self::add_link`] places a
    /// link.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] when `parent` is not a folder, and
    /// [`HakuError::Storage`] when the write fails.
    pub fn add_folder(&mut self, parent: BookmarkId, index: Option<u32>, title: &str, now: i64) -> Result<BookmarkId> {
        self.insert(parent, index, title, None, now)
    }

    fn insert(
        &mut self,
        parent: BookmarkId,
        index: Option<u32>,
        title: &str,
        url: Option<&str>,
        now: i64,
    ) -> Result<BookmarkId> {
        let transaction = self.connection.transaction()?;
        check_folder(&transaction, parent)?;
        let position = open_place(&transaction, parent, index)?;
        transaction.execute(
            "INSERT INTO bookmarks (parent, position, url, title, added_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![parent.0, position, url, title, now],
        )?;
        let id = BookmarkId(transaction.last_insert_rowid());
        touch(&transaction, parent, now)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Renames a link or folder, and gives a link `url` when there is one.
    /// `url` is ignored for a folder.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] for a root, an unknown id, or a URL
    /// that may not be bookmarked, and [`HakuError::Storage`] when the write
    /// fails.
    pub fn update(&mut self, id: BookmarkId, title: &str, url: Option<&str>, now: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let entry = movable(&transaction, id)?;
        let url = match (entry.is_folder, url) {
            (false, Some(url)) => {
                check_url(url)?;
                Some(url)
            }
            _ => None,
        };
        transaction.execute(
            "UPDATE bookmarks SET title = ?2, url = COALESCE(?3, url), modified_at = ?4 WHERE id = ?1",
            params![id.0, title, url, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Moves a link or folder into `parent`, to be at `index` among what it
    /// then holds, or at its end when `index` is nothing or past it.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] for a root, an unknown id, a parent
    /// that is not a folder, or a folder moved into itself or one it holds,
    /// and [`HakuError::Storage`] when the write fails.
    pub fn move_to(&mut self, id: BookmarkId, parent: BookmarkId, index: Option<u32>, now: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let entry = movable(&transaction, id)?;
        check_folder(&transaction, parent)?;
        if entry.is_folder && holds(&transaction, id, parent)? {
            return Err(HakuError::Unsupported(
                "a folder cannot be moved into itself or a folder it holds".into(),
            ));
        }
        close_place(&transaction, entry.parent, entry.position)?;
        let position = open_place(&transaction, parent, index)?;
        transaction.execute(
            "UPDATE bookmarks SET parent = ?2, position = ?3 WHERE id = ?1",
            params![id.0, parent.0, position],
        )?;
        touch(&transaction, entry.parent, now)?;
        touch(&transaction, parent, now)?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes a link, or a folder with everything it holds.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] for a root or an unknown id, and
    /// [`HakuError::Storage`] when the write fails.
    pub fn remove(&mut self, id: BookmarkId, now: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let entry = movable(&transaction, id)?;
        transaction.execute(
            "WITH RECURSIVE held(id) AS (
                 SELECT ?1 UNION ALL SELECT bookmarks.id FROM bookmarks JOIN held ON bookmarks.parent = held.id
             )
             DELETE FROM bookmarks WHERE id IN held",
            [id.0],
        )?;
        close_place(&transaction, entry.parent, entry.position)?;
        touch(&transaction, entry.parent, now)?;
        transaction.commit()?;
        Ok(())
    }
}

fn build(row: Row, children: &mut HashMap<i64, Vec<Row>>) -> BookmarkNode {
    let kind = match row.url {
        Some(url) => BookmarkKind::Link { url },
        None => BookmarkKind::Folder {
            children: children
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|child| build(child, children))
                .collect(),
        },
    };
    BookmarkNode {
        id: BookmarkId(row.id),
        title: row.title,
        added_at: row.added_at,
        modified_at: row.modified_at,
        kind,
    }
}

fn check_url(url: &str) -> Result<()> {
    if is_bookmarkable(url) {
        Ok(())
    } else {
        Err(HakuError::Unsupported(format!(
            "only http, https and file addresses can be bookmarked: {url}"
        )))
    }
}

fn check_folder(transaction: &Transaction<'_>, id: BookmarkId) -> Result<()> {
    let url: Option<Option<String>> = transaction
        .query_row("SELECT url FROM bookmarks WHERE id = ?1", [id.0], |row| row.get(0))
        .optional()?;
    match url {
        Some(None) => Ok(()),
        Some(Some(_)) => Err(HakuError::Unsupported(format!("bookmark {} is not a folder", id.0))),
        None => Err(HakuError::Unsupported(format!("no bookmark {}", id.0))),
    }
}

/// Where a bookmark that is not a root sits.
struct Entry {
    parent: BookmarkId,
    position: i64,
    is_folder: bool,
}

/// The bookmark `id`, refused when it is a root or does not exist.
fn movable(transaction: &Transaction<'_>, id: BookmarkId) -> Result<Entry> {
    let row: Option<(Option<i64>, i64, bool)> = transaction
        .query_row(
            "SELECT parent, position, url IS NULL FROM bookmarks WHERE id = ?1",
            [id.0],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match row {
        Some((Some(parent), position, is_folder)) => Ok(Entry {
            parent: BookmarkId(parent),
            position,
            is_folder,
        }),
        Some((None, ..)) => Err(HakuError::Unsupported(
            "the bookmarks bar and other bookmarks cannot be changed".into(),
        )),
        None => Err(HakuError::Unsupported(format!("no bookmark {}", id.0))),
    }
}

/// Whether `folder` is `ancestor` or is held in it, at any depth.
fn holds(transaction: &Transaction<'_>, ancestor: BookmarkId, folder: BookmarkId) -> Result<bool> {
    Ok(transaction.query_row(
        "WITH RECURSIVE above(id) AS (
             SELECT ?2 UNION SELECT bookmarks.parent FROM bookmarks JOIN above ON bookmarks.id = above.id
             WHERE bookmarks.parent IS NOT NULL
         )
         SELECT EXISTS (SELECT 1 FROM above WHERE id = ?1)",
        params![ancestor.0, folder.0],
        |row| row.get(0),
    )?)
}

/// Makes room in `parent` at `index`, clamped to its end, and returns the
/// position made.
fn open_place(transaction: &Transaction<'_>, parent: BookmarkId, index: Option<u32>) -> Result<i64> {
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM bookmarks WHERE parent = ?1", [parent.0], |row| {
        row.get(0)
    })?;
    let position = index.map_or(count, |index| i64::from(index).min(count));
    transaction.execute(
        "UPDATE bookmarks SET position = position + 1 WHERE parent = ?1 AND position >= ?2",
        params![parent.0, position],
    )?;
    Ok(position)
}

/// Closes the gap a bookmark leaving `position` in `parent` leaves.
fn close_place(transaction: &Transaction<'_>, parent: BookmarkId, position: i64) -> Result<()> {
    transaction.execute(
        "UPDATE bookmarks SET position = position - 1 WHERE parent = ?1 AND position > ?2",
        params![parent.0, position],
    )?;
    Ok(())
}

/// Marks a folder as changed.
fn touch(transaction: &Transaction<'_>, folder: BookmarkId, now: i64) -> Result<()> {
    transaction.execute(
        "UPDATE bookmarks SET modified_at = ?2 WHERE id = ?1",
        params![folder.0, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::storage::corrupt_path;

    fn database() -> BookmarksDb {
        BookmarksDb::in_memory().unwrap()
    }

    fn children(node: &BookmarkNode) -> &[BookmarkNode] {
        match &node.kind {
            BookmarkKind::Folder { children } => children,
            BookmarkKind::Link { .. } => panic!("{} is not a folder", node.title),
        }
    }

    fn titles(node: &BookmarkNode) -> Vec<&str> {
        children(node).iter().map(|child| child.title.as_str()).collect()
    }

    /// Every folder's positions, read back from the table, run 0, 1, 2…
    fn assert_dense(database: &BookmarksDb) {
        let gaps: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM (
                     SELECT position, ROW_NUMBER() OVER (PARTITION BY parent ORDER BY position) - 1 AS expected
                     FROM bookmarks WHERE parent IS NOT NULL
                 ) WHERE position != expected",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(gaps, 0, "positions have gaps or repeats");
    }

    fn roots(database: &BookmarksDb) -> (BookmarkId, BookmarkId) {
        let tree = database.tree().unwrap();
        (tree.bar.id, tree.other.id)
    }

    #[test]
    fn a_new_database_has_the_two_empty_roots() {
        let tree = database().tree().unwrap();

        assert_eq!(tree.bar.title, "Bookmarks bar");
        assert_eq!(tree.other.title, "Other bookmarks");
        assert!(children(&tree.bar).is_empty());
        assert!(children(&tree.other).is_empty());
    }

    #[test]
    fn links_and_folders_are_added_in_order_and_at_an_index() {
        let mut database = database();
        let (bar, _) = roots(&database);

        database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        database.add_link(bar, None, "C", "https://c.test/", 2).unwrap();
        let folder = database.add_folder(bar, Some(1), "B", 3).unwrap();
        database
            .add_link(folder, Some(9), "Inside", "https://in.test/", 4)
            .unwrap();

        let tree = database.tree().unwrap();
        assert_eq!(titles(&tree.bar), ["A", "B", "C"]);
        assert_eq!(titles(&children(&tree.bar)[1]), ["Inside"]);
        assert_dense(&database);
    }

    #[test]
    fn a_link_keeps_its_url_and_times() {
        let mut database = database();
        let (_, other) = roots(&database);

        let id = database.add_link(other, None, "A", "https://a.test/", 5).unwrap();

        let tree = database.tree().unwrap();
        let link = &children(&tree.other)[0];
        assert_eq!(link.id, id);
        assert_eq!(
            link.kind,
            BookmarkKind::Link {
                url: "https://a.test/".into()
            }
        );
        assert_eq!((link.added_at, link.modified_at), (5, 5));
        assert_eq!(tree.other.modified_at, 5, "the folder changed too");
    }

    #[test]
    fn the_same_url_may_be_bookmarked_twice() {
        let mut database = database();
        let (bar, _) = roots(&database);

        database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        database.add_link(bar, None, "A", "https://a.test/", 2).unwrap();

        assert_eq!(children(&database.tree().unwrap().bar).len(), 2);
    }

    #[test]
    fn a_url_that_may_not_be_bookmarked_is_refused() {
        let mut database = database();
        let (bar, _) = roots(&database);

        let added = database.add_link(bar, None, "Script", "javascript:alert(1)", 1);

        assert!(matches!(added, Err(HakuError::Unsupported(_))));
        assert!(children(&database.tree().unwrap().bar).is_empty());
    }

    #[test]
    fn nothing_is_added_to_a_link_or_an_unknown_folder() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let link = database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();

        assert!(matches!(
            database.add_folder(link, None, "F", 2),
            Err(HakuError::Unsupported(_))
        ));
        assert!(matches!(
            database.add_folder(BookmarkId(999), None, "F", 2),
            Err(HakuError::Unsupported(_))
        ));
    }

    #[test]
    fn updating_renames_and_readdresses_a_link() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let id = database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();

        database.update(id, "B", Some("https://b.test/"), 2).unwrap();
        database.update(id, "C", None, 3).unwrap();

        let tree = database.tree().unwrap();
        let link = &children(&tree.bar)[0];
        assert_eq!(link.title, "C");
        assert_eq!(
            link.kind,
            BookmarkKind::Link {
                url: "https://b.test/".into()
            }
        );
        assert_eq!(link.modified_at, 3);
    }

    #[test]
    fn updating_a_folder_ignores_a_url() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let id = database.add_folder(bar, None, "F", 1).unwrap();

        database.update(id, "G", Some("https://a.test/"), 2).unwrap();

        let tree = database.tree().unwrap();
        let folder = &children(&tree.bar)[0];
        assert_eq!(folder.title, "G");
        assert!(matches!(folder.kind, BookmarkKind::Folder { .. }));
    }

    #[test]
    fn the_roots_cannot_be_renamed_moved_or_removed() {
        let mut database = database();
        let (bar, other) = roots(&database);

        assert!(matches!(
            database.update(bar, "X", None, 1),
            Err(HakuError::Unsupported(_))
        ));
        assert!(matches!(
            database.move_to(bar, other, None, 1),
            Err(HakuError::Unsupported(_))
        ));
        assert!(matches!(database.remove(other, 1), Err(HakuError::Unsupported(_))));
        assert_eq!(database.tree().unwrap().bar.title, "Bookmarks bar");
    }

    #[test]
    fn moving_within_a_folder_reorders_it() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let a = database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        database.add_link(bar, None, "B", "https://b.test/", 1).unwrap();
        database.add_link(bar, None, "C", "https://c.test/", 1).unwrap();

        database.move_to(a, bar, Some(2), 2).unwrap();
        assert_eq!(titles(&database.tree().unwrap().bar), ["B", "C", "A"]);

        database.move_to(a, bar, Some(0), 3).unwrap();
        assert_eq!(titles(&database.tree().unwrap().bar), ["A", "B", "C"]);
        assert_dense(&database);
    }

    #[test]
    fn moving_between_folders_closes_the_gap_it_leaves() {
        let mut database = database();
        let (bar, other) = roots(&database);
        database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        let b = database.add_link(bar, None, "B", "https://b.test/", 1).unwrap();
        database.add_link(bar, None, "C", "https://c.test/", 1).unwrap();
        database.add_link(other, None, "X", "https://x.test/", 1).unwrap();

        database.move_to(b, other, Some(0), 2).unwrap();

        let tree = database.tree().unwrap();
        assert_eq!(titles(&tree.bar), ["A", "C"]);
        assert_eq!(titles(&tree.other), ["B", "X"]);
        assert_eq!((tree.bar.modified_at, tree.other.modified_at), (2, 2));
        assert_dense(&database);
    }

    #[test]
    fn a_folder_cannot_move_into_itself_or_a_folder_it_holds() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let outer = database.add_folder(bar, None, "Outer", 1).unwrap();
        let inner = database.add_folder(outer, None, "Inner", 1).unwrap();
        let deepest = database.add_folder(inner, None, "Deepest", 1).unwrap();

        for target in [outer, inner, deepest] {
            assert!(matches!(
                database.move_to(outer, target, None, 2),
                Err(HakuError::Unsupported(_))
            ));
        }
        // The other way is fine.
        database.move_to(deepest, bar, None, 2).unwrap();
        assert_eq!(titles(&database.tree().unwrap().bar), ["Outer", "Deepest"]);
    }

    #[test]
    fn a_link_cannot_be_moved_into_a_link() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let a = database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        let b = database.add_link(bar, None, "B", "https://b.test/", 1).unwrap();

        assert!(matches!(
            database.move_to(a, b, None, 2),
            Err(HakuError::Unsupported(_))
        ));
        assert_dense(&database);
    }

    #[test]
    fn removing_a_folder_removes_everything_it_holds() {
        let mut database = database();
        let (bar, _) = roots(&database);
        let folder = database.add_folder(bar, None, "F", 1).unwrap();
        let inner = database.add_folder(folder, None, "G", 1).unwrap();
        database.add_link(inner, None, "A", "https://a.test/", 1).unwrap();
        database.add_link(bar, None, "After", "https://after.test/", 1).unwrap();

        database.remove(folder, 2).unwrap();

        assert_eq!(titles(&database.tree().unwrap().bar), ["After"]);
        let rows: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 3, "the two roots and the one link left");
        assert_dense(&database);
    }

    #[test]
    fn removing_an_unknown_bookmark_is_refused() {
        assert!(matches!(
            database().remove(BookmarkId(999), 1),
            Err(HakuError::Unsupported(_))
        ));
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("haku-bookmarks-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("bookmarks.sqlite")
    }

    #[test]
    fn bookmarks_are_kept_in_the_file() {
        let path = temp_path("kept");
        {
            let mut database = BookmarksDb::open_or_set_aside(&path).unwrap();
            let (bar, _) = roots(&database);
            database.add_link(bar, None, "A", "https://a.test/", 1).unwrap();
        }

        let database = BookmarksDb::open_or_set_aside(&path).unwrap();

        assert_eq!(titles(&database.tree().unwrap().bar), ["A"]);
    }

    #[test]
    fn a_damaged_file_is_set_aside_and_kept() {
        let path = temp_path("damaged");
        fs::write(&path, "this is not a database, just bytes\n").unwrap();

        let database = BookmarksDb::open_or_set_aside(&path).unwrap();

        assert!(children(&database.tree().unwrap().bar).is_empty());
        assert_eq!(
            fs::read_to_string(corrupt_path(&path)).unwrap(),
            "this is not a database, just bytes\n"
        );
    }

    #[test]
    fn a_file_that_cannot_be_opened_falls_back_to_memory_and_says_so() {
        let path = temp_path("unopenable");
        // A folder where the file should be cannot be opened as one.
        fs::create_dir_all(&path).unwrap();

        let (database, failure) = BookmarksDb::open_or_in_memory(&path);

        assert!(matches!(failure, Some(HakuError::Storage(_))));
        assert!(database.tree().is_ok());
        assert!(path.is_dir(), "the path is left as it was");
    }
}
