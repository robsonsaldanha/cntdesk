//! Video encoder abstraction with hardware/software fallback chain.

use crate::error::CoreError;
use crate::frame::{CapturedFrame, PixelFormat};

/// Encoder configuration parameters.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub keyframe_interval_ms: u32,
}

impl EncoderConfig {
    /// Validate encoder configuration.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.width == 0 || self.height == 0 {
            return Err(CoreError::Protocol(
                "Encoder dimensions must be non-zero".into(),
            ));
        }
        if self.fps == 0 {
            return Err(CoreError::Protocol("FPS must be non-zero".into()));
        }
        if self.bitrate_kbps == 0 {
            return Err(CoreError::Protocol("Bitrate must be non-zero".into()));
        }
        Ok(())
    }
}

/// Encoded video packet.
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub timestamp_us: u64,
    pub seq: u64,
}

/// Trait for video encoder backends.
pub trait VideoEncoderBackend: Send {
    /// Encode a captured frame into a packet.
    fn encode(&mut self, frame: &CapturedFrame) -> Result<EncodedPacket, CoreError>;

    /// Request the next frame to be a keyframe.
    fn request_keyframe(&mut self);

    /// Update target bitrate dynamically.
    fn update_bitrate(&mut self, kbps: u32);
}

/// Stub encoder for phase 0 — returns synthetic packets.
struct StubEncoder {
    config: EncoderConfig,
    seq: u64,
    force_keyframe: bool,
}

impl VideoEncoderBackend for StubEncoder {
    fn encode(&mut self, frame: &CapturedFrame) -> Result<EncodedPacket, CoreError> {
        let is_keyframe = self.force_keyframe || self.seq % 30 == 0;
        self.force_keyframe = false;

        // Synthetic packet: just enough to satisfy the interface
        let data = vec![0u8; 1024]; // placeholder NAL unit

        let packet = EncodedPacket {
            data,
            is_keyframe,
            timestamp_us: frame.timestamp_us,
            seq: self.seq,
        };
        self.seq += 1;
        Ok(packet)
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }

    fn update_bitrate(&mut self, _kbps: u32) {
        // No-op for stub encoder
    }
}

/// Create an encoder backend. Tries hardware encoders in priority order
/// (NVENC → AMF → VAAPI → VideoToolbox), falls back to x264 software.
pub fn create_encoder(config: &EncoderConfig) -> Result<Box<dyn VideoEncoderBackend>, CoreError> {
    config.validate()?;

    tracing::info!(
        "Creating encoder: {}x{} @ {}fps, {}kbps",
        config.width,
        config.height,
        config.fps,
        config.bitrate_kbps
    );

    // Use real x264 when available, mock otherwise
    crate::encoder_x264::create_x264_encoder(config)
}