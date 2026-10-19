//! Shared frame types used by capture and encoder modules.

/// Pixel format of captured frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra,
    Rgba,
}

/// A single captured screen frame.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub timestamp_us: u64,
}