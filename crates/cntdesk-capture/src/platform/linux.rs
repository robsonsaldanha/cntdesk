//! Linux screen capture backend using PipeWire/XDG Portal.

use cntdesk_core::error::CoreError;
use crate::capture::{CaptureConfig, CapturedFrame, ScreenCaptureBackend};

/// PipeWire-based screen capture for Linux.
pub struct PipewireCapture {
    _private: (),
}

impl PipewireCapture {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl ScreenCaptureBackend for PipewireCapture {
    fn start(&mut self, _config: &CaptureConfig) -> Result<(), CoreError> {
        todo!("PipeWire capture not yet implemented")
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        todo!("PipeWire capture not yet implemented")
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        todo!("PipeWire capture not yet implemented")
    }
}