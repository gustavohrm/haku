pub mod history;
pub mod pool;
pub mod tab;

pub use history::{History, Visit};
pub use pool::{Acquired, Slot, SlotId, WebviewPool, DEFAULT_CAPACITY};
pub use tab::{is_internal, Scroll, Tab, TabId, TabPresence};
