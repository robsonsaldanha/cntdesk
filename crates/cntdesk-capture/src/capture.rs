//! Screen capture abstraction layer.

use cntdesk_core::error::CoreError;
pub use cntdesk_core::frame::{CapturedFrame, PixelFormat};

/// Capture configuration.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub target_fps: u32,
    pub max_width: u32,
    pub max_height: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            target_fps: 30,
            max_width: 1920,
            max_height: 1080,
        }
    }
}

/// Platform-specific screen capture backend.
pub trait ScreenCaptureBackend: Send + Sync {
    /// Start capturing with the given configuration.
    fn start(&mut self, config: &CaptureConfig) -> Result<(), CoreError>;

    /// Get the next captured frame.
    /// Returns `Ok(None)` when the display is locked or sleeping.
    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError>;

    /// Stop capturing.
    fn stop(&mut self) -> Result<(), CoreError>;
}

/// Create a platform-appropriate capture backend.
pub fn create_backend() -> Result<Box<dyn ScreenCaptureBackend>, CoreError> {
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(crate::platform::windows::DxgiCapture::new()))
    }

    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(crate::platform::linux::PipewireCapture::new()))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(crate::platform::macos::ScreenCaptureKitBackend::new()))
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err(CoreError::Protocol(
            "unsupported platform for screen capture".into(),
        ))
    }
}