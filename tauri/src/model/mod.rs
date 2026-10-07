pub mod dialog;
pub mod history;
pub mod optimization;
pub mod page_state;
pub mod pool;
pub mod pressure;
pub mod tab;

pub use dialog::{DialogAnswer, DialogId, DialogKind, PageDialog};
pub use history::{Commit, History, NavigationKind, Visit};
pub use optimization::{Policy, Preset};
pub use page_state::{Loss, LossSignal, PageRecord, PageState};
pub use pool::{Acquired, Slot, SlotId, WebviewPool, DEFAULT_CAPACITY};
pub use pressure::{MemoryStatus, Pressure};
pub use tab::{is_internal, Scroll, Tab, TabId, TabPresence};
