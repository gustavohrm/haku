pub mod dialog;
pub mod history;
pub mod pool;
pub mod tab;

pub use dialog::{DialogAnswer, DialogId, DialogKind, PageDialog};
pub use history::{Commit, History, NavigationKind, Visit};
pub use pool::{Acquired, Slot, SlotId, WebviewPool, DEFAULT_CAPACITY};
pub use tab::{is_internal, Scroll, Tab, TabId, TabPresence};
