//! Windows screen capture backend using DXGI Desktop Duplication.
//!
//! Phase 1: Returns synthetic BGRA frames for end-to-end testing.
//! Real DXGI Desktop Duplication API integration deferred to phase 2
//! (requires windows-rs + d3d11 + dxgi FFI bindings).

use std::time::{SystemTime, UNIX_EPOCH};

use cntdesk_core::error::CoreError;
use cntdesk_core::frame::{CapturedFrame, PixelFormat};
use crate::capture::{CaptureConfig, ScreenCaptureBackend};

/// DXGI-based screen capture for Windows.
/// Phase 1: synthetic frame generator for installer validation.
pub struct DxgiCapture {
    width: u32,
    height: u32,
    running: bool,
    frame_seq: u64,
}

impl DxgiCapture {
    pub fn new() -> Self {
        Self {
            width: 1920,
            height: 1080,
            running: false,
            frame_seq: 0,
        }
    }

    /// Generate a synthetic BGRA frame with a moving gradient pattern.
    /// This allows visual verification that the streaming pipeline works
    /// end-to-end without requiring real DXGI FFI bindings.
    fn generate_synthetic_frame(&mut self) -> CapturedFrame {
        let timestamp_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        // Create a visible gradient pattern that changes per frame
        // so the viewer can confirm frames are flowing and updating.
        let mut data = vec![0u8; (self.width * self.height * 4) as usize];
        let offset = (self.frame_seq % 256) as u8;
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = ((y * self.width + x) * 4) as usize;
                // BGRA: blue varies with X, green with Y, red with frame seq
                data[idx] = ((x as u8).wrapping_add(offset)) as u8;     // B
                data[idx + 1] = (y as u8).wrapping_add(offset) as u8;   // G
                data[idx + 2] = offset;                                  // R
                data[idx + 3] = 255;                                     // A
            }
        }

        self.frame_seq += 1;

        CapturedFrame {
            data,
            width: self.width,
            height: self.height,
            format: PixelFormat::Bgra,
            timestamp_us,
        }
    }
}

impl Default for DxgiCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCaptureBackend for DxgiCapture {
    fn start(&mut self, config: &crate::capture::CaptureConfig) -> Result<(), CoreError> {
        if config.max_width > 0 && config.max_height > 0 {
            self.width = config.max_width;
            self.height = config.max_height;
        }
        self.running = true;
        self.frame_seq = 0;
        tracing::info!(
            width = self.width,
            height = self.height,
            "DXGI capture started (phase 1 synthetic mode)"
        );
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        if !self.running {
            return Ok(None);
        }
        // Throttle to ~30 FPS to avoid burning CPU in synthetic mode
        std::thread::sleep(std::time::Duration::from_millis(33));
        Ok(Some(self.generate_synthetic_frame()))
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        self.running = false;
        tracing::info!("DXGI capture stopped");
        Ok(())
    }
}