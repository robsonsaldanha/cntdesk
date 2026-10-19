//! CNT Desk screen capture crate.

pub mod capture;
mod platform;

pub use capture::{CapturedFrame, CaptureConfig, PixelFormat, ScreenCaptureBackend};