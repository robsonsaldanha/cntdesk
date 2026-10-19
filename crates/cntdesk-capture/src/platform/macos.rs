//! macOS screen capture backend using ScreenCaptureKit.

use cntdesk_core::error::CoreError;
use crate::capture::{CaptureConfig, CapturedFrame, ScreenCaptureBackend};

/// ScreenCaptureKit-based screen capture for macOS.
pub struct ScreenCaptureKitBackend {
    _private: (),
}

impl ScreenCaptureKitBackend {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl ScreenCaptureBackend for ScreenCaptureKitBackend {
    fn start(&mut self, _config: &CaptureConfig) -> Result<(), CoreError> {
        todo!("ScreenCaptureKit not yet implemented")
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        todo!("ScreenCaptureKit not yet implemented")
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        todo!("ScreenCaptureKit not yet implemented")
    }
}