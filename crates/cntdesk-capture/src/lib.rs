//! CNT Desk screen capture crate.

pub mod capture;
mod platform;

// Re-export frame types from core for backwards compatibility
pub use cntdesk_core::frame::{CapturedFrame, PixelFormat};
pub use capture::{CaptureConfig, ScreenCaptureBackend, create_backend};