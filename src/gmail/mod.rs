//! Gmail API wrappers — OAuth2 device-loopback auth, draft creation,
//! draft-send. Used by the TUI bot (drafts.create) and the scheduler binary
//! (drafts.send).

pub mod auth;
pub mod draft;
pub mod sender;

pub use auth::{ensure_token, run_setup_flow, TokenCache};
pub use draft::{create_draft, DraftRequest};
pub use sender::{send_draft, SendError};
