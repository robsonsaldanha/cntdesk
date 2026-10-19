//! Windows screen capture backend using DXGI Desktop Duplication.

use cntdesk_core::error::CoreError;
use crate::capture::{CaptureConfig, CapturedFrame, ScreenCaptureBackend};

/// DXGI-based screen capture for Windows.
pub struct DxgiCapture {
    _private: (),
}

impl DxgiCapture {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl ScreenCaptureBackend for DxgiCapture {
    fn start(&mut self, _config: &CaptureConfig) -> Result<(), CoreError> {
        todo!("DXGI capture not yet implemented")
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        todo!("DXGI capture not yet implemented")
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        todo!("DXGI capture not yet implemented")
    }
}