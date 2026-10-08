/// Identifies one request a page made to open a new window. The engine holds
/// the page's request open until it is answered, with a webview or a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowRequestId(pub u64);
