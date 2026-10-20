//! Linux screen capture backend using PipeWire/XDG Portal.
//!
//! NOTE: This is a stub implementation for phase 1. The real PipeWire backend
//! requires pipewire-rs and a running PipeWire daemon, which are unavailable
//! in headless VPS environments. On systems with PipeWire available, this
//! should be replaced with a real implementation using pipewire-rs + XDG
//! Desktop Portal screencast API. See task-7-report.md for details.

use cntdesk_core::error::CoreError;
use crate::capture::{CaptureConfig, CapturedFrame, ScreenCaptureBackend};

/// PipeWire-based screen capture for Linux.
/// Phase 1: stub that returns None gracefully in headless environments.
pub struct PipewireCapture {
    started: bool,
}

impl PipewireCapture {
    pub fn new() -> Self {
        Self { started: false }
    }
}

impl ScreenCaptureBackend for PipewireCapture {
    fn start(&mut self, _config: &CaptureConfig) -> Result<(), CoreError> {
        // Stub: mark as started but no actual PipeWire connection.
        // Real implementation would connect to XDG Portal screencast session here.
        self.started = true;
        tracing::warn!(
            "PipeWire capture backend is a stub — returning no frames. \
             Real PipeWire support requires pipewire-rs and a running PipeWire daemon."
        );
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        if !self.started {
            return Err(CoreError::Protocol(
                "PipeWire capture not started".into(),
            ));
        }
        // Stub: no display available in headless/VPS environment.
        // Real implementation would dequeue a DMA-BUF frame from PipeWire stream.
        Ok(None)
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        self.started = false;
        Ok(())
    }
}