//! Software encoder backend producing structurally valid H.264 NAL units.
//!
//! NOTE: This is a mock implementation for VPS environments where the real
//! x264 crate cannot compile (missing libx264 native library). It produces
//! packets with valid H.264 structure (start codes + SPS/PPS + slice headers)
//! but does NOT perform actual video compression. On production builds with
//! libx264 available, replace with real x264 crate integration.

use crate::encoder::{EncodedPacket, EncoderConfig, VideoEncoderBackend};
use crate::error::CoreError;
use crate::frame::{CapturedFrame, PixelFormat};

/// Mock x264 encoder producing structurally valid H.264 NAL units.
pub struct X264Encoder {
    config: EncoderConfig,
    seq: u64,
    force_keyframe: bool,
    keyframe_interval_frames: u64,
}

impl X264Encoder {
    /// Create a new mock x264 encoder with the given configuration.
    pub fn new(config: &EncoderConfig) -> Result<Self, CoreError> {
        config.validate()?;
        let keyframe_interval_frames =
            (config.keyframe_interval_ms as u64 * config.fps as u64) / 1000;
        Ok(Self {
            config: config.clone(),
            seq: 0,
            force_keyframe: true, // First frame must be keyframe
            keyframe_interval_frames: keyframe_interval_frames.max(1),
        })
    }

    /// Generate a minimal valid SPS NAL unit for Baseline profile.
    fn generate_sps(&self) -> Vec<u8> {
        // NAL start code + NAL type 7 (SPS) + minimal valid SPS payload
        // Profile IDC 66 (Baseline), Level 4.0
        let mut sps = vec![
            0x00, 0x00, 0x00, 0x01, // Start code
            0x67,                   // NAL type 7 (SPS), nal_ref_idc=3
            0x42,                   // profile_idc = 66 (Baseline)
            0xC0,                   // constraint_set flags
            0x28,                   // level_idc = 40 (Level 4.0)
        ];
        // Encode width/height in simplified form (mock — not fully spec-compliant SPS)
        let w = self.config.width as u16;
        let h = self.config.height as u16;
        sps.extend_from_slice(&w.to_be_bytes());
        sps.extend_from_slice(&h.to_be_bytes());
        sps
    }

    /// Generate a minimal valid PPS NAL unit.
    fn generate_pps(&self) -> Vec<u8> {
        vec![
            0x00, 0x00, 0x00, 0x01, // Start code
            0x68,                   // NAL type 8 (PPS), nal_ref_idc=3
            0xCE, 0x38, 0x80,       // Minimal PPS payload
        ]
    }

    /// Generate a slice NAL unit (IDR or non-IDR depending on keyframe flag).
    fn generate_slice(&self, is_keyframe: bool, frame_data: &[u8]) -> Vec<u8> {
        let nal_type = if is_keyframe { 0x65 } else { 0x41 }; // IDR=5, non-IDR=1
        let mut slice = vec![
            0x00, 0x00, 0x00, 0x01, // Start code
            nal_type,               // NAL type with nal_ref_idc
        ];
        // Include a hash of the frame data as simulated compressed payload
        // This ensures different frames produce different output
        let hash = simple_hash(frame_data);
        slice.extend_from_slice(&hash.to_le_bytes());
        // Add padding to simulate realistic packet size based on bitrate
        let target_size = (self.config.bitrate_kbps as usize * 1000)
            / (self.config.fps as usize * 8);
        let padding_len = target_size.saturating_sub(slice.len()).min(4096);
        slice.extend(std::iter::repeat(0xAA).take(padding_len));
        slice
    }
}

/// Simple hash for deterministic but varied output per frame.
fn simple_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    // Sample every 1024th byte to avoid hashing entire frame
    for (i, &b) in data.iter().enumerate() {
        if i % 1024 == 0 {
            h = h.wrapping_mul(0x100000001b3).wrapping_add(b as u64);
        }
    }
    h
}

impl VideoEncoderBackend for X264Encoder {
    fn encode(&mut self, frame: &CapturedFrame) -> Result<EncodedPacket, CoreError> {
        if frame.format != PixelFormat::Bgra {
            return Err(CoreError::Protocol(
                "X264Encoder only supports BGRA pixel format".into(),
            ));
        }

        let is_keyframe = self.force_keyframe || (self.seq % self.keyframe_interval_frames == 0);
        self.force_keyframe = false;

        let mut data = Vec::new();

        // Keyframes include SPS + PPS before the slice
        if is_keyframe {
            data.extend(self.generate_sps());
            data.extend(self.generate_pps());
        }

        // Always include slice NAL
        data.extend(self.generate_slice(is_keyframe, &frame.data));

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

    fn update_bitrate(&mut self, kbps: u32) {
        self.config.bitrate_kbps = kbps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_encoder_nal_start_codes() {
        let config = EncoderConfig {
            width: 640,
            height: 480,
            fps: 30,
            bitrate_kbps: 1000,
            keyframe_interval_ms: 1000,
        };
        let mut enc = X264Encoder::new(&config).unwrap();
        let frame = CapturedFrame {
            data: vec![0u8; 640 * 480 * 4],
            width: 640,
            height: 480,
            format: PixelFormat::Bgra,
            timestamp_us: 0,
        };
        let pkt = enc.encode(&frame).unwrap();
        // Verify start codes present
        assert!(pkt.data.windows(4).any(|w| w == [0, 0, 0, 1]));
        // First packet is keyframe → should have SPS (NAL type 0x67)
        assert!(pkt.data.contains(&0x67));
        // And PPS (NAL type 0x68)
        assert!(pkt.data.contains(&0x68));
    }
}