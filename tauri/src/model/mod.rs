pub mod context_menu;
pub mod dialog;
pub mod extension;
pub mod history;
pub mod keymap;
pub mod optimization;
pub mod page_state;
pub mod pool;
pub mod pressure;
pub mod preview;
pub mod tab;
pub mod window;

pub use context_menu::{
    allowed_items, offers, plain_label, ContextMenuRequestId, ContextMenuTarget, ContextMenuTargetKind, NativeMenuItem,
    NativeMenuItemKind, PageMenu,
};
pub use dialog::{DialogAnswer, DialogId, DialogKind, PageDialog};
pub use extension::{
    icon_file, popup_origin, popup_size, popup_url, Extension, Extensions, ScreenRect, POPUP_MAX_SIZE, POPUP_MIN_SIZE,
};
pub use history::{Commit, History, NavigationKind, Visit};
pub use keymap::{shortcut_for, Chord, Key, Shortcut};
pub use optimization::{Policy, Preset};
pub use page_state::{Loss, LossSignal, PageRecord, PageState};
pub use pool::{Acquired, Profile, Slot, SlotId, WebviewPool, DEFAULT_CAPACITY};
pub use pressure::{MemoryStatus, Pressure};
pub use preview::{data_url, jpeg_data_url, Previews};
pub use tab::{host_of, is_internal, CrashCause, Scroll, Tab, TabId, TabPresence};
pub use window::{Placement, WindowId, WindowKind, WindowRequestId, WINDOW_LABEL_PREFIX};
