use cntdesk_core::encoder::{EncoderConfig, EncodedPacket, VideoEncoderBackend, create_encoder};
use cntdesk_core::encoder_x264::X264Encoder;
use cntdesk_core::frame::{CapturedFrame, PixelFormat};

#[test]
fn test_encoder_config_validation() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    assert!(cfg.validate().is_ok());
    let bad = EncoderConfig { width: 0, ..cfg };
    assert!(bad.validate().is_err());
}

#[test]
fn test_encoder_config_zero_height_rejected() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 0,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn test_encoder_config_zero_fps_rejected() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 0,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn test_create_encoder_returns_valid_backend() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    let mut encoder = create_encoder(&cfg).unwrap();
    let frame = CapturedFrame {
        data: vec![0u8; 1920 * 1080 * 4],
        width: 1920,
        height: 1080,
        format: PixelFormat::Bgra,
        timestamp_us: 1000,
    };
    let packet = encoder.encode(&frame).unwrap();
    assert!(!packet.data.is_empty());
    assert!(packet.is_keyframe); // First frame should be keyframe
    assert_eq!(packet.timestamp_us, 1000);
    assert_eq!(packet.seq, 0);
}

#[test]
fn test_request_keyframe() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    let mut encoder = create_encoder(&cfg).unwrap();
    let frame = CapturedFrame {
        data: vec![0u8; 1920 * 1080 * 4],
        width: 1920,
        height: 1080,
        format: PixelFormat::Bgra,
        timestamp_us: 1000,
    };
    // First frame is keyframe
    let _ = encoder.encode(&frame).unwrap();
    // Second frame is not keyframe
    let p2 = encoder.encode(&frame).unwrap();
    assert!(!p2.is_keyframe);
    // Request keyframe
    encoder.request_keyframe();
    let p3 = encoder.encode(&frame).unwrap();
    assert!(p3.is_keyframe);
}

#[test]
fn test_update_bitrate_no_panic() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    let mut encoder = create_encoder(&cfg).unwrap();
    encoder.update_bitrate(8000); // Should not panic
}

#[test]
fn test_x264_encoder_produces_valid_packets() {
    let config = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 2000,
        keyframe_interval_ms: 1000,
    };
    let mut encoder = X264Encoder::new(&config).unwrap();
    let frame = CapturedFrame {
        data: vec![0u8; 1920 * 1080 * 4],
        width: 1920,
        height: 1080,
        format: PixelFormat::Bgra,
        timestamp_us: 0,
    };
    let packet = encoder.encode(&frame).unwrap();
    assert!(!packet.data.is_empty());
    assert!(packet.is_keyframe); // first frame must be keyframe
    assert_eq!(packet.seq, 0);
}

#[test]
fn test_x264_encoder_keyframe_request() {
    let config = EncoderConfig {
        width: 640,
        height: 480,
        fps: 30,
        bitrate_kbps: 1000,
        keyframe_interval_ms: 2000,
    };
    let mut encoder = X264Encoder::new(&config).unwrap();
    encoder.request_keyframe();
    let frame = CapturedFrame {
        data: vec![0u8; 640 * 480 * 4],
        width: 640,
        height: 480,
        format: PixelFormat::Bgra,
        timestamp_us: 1000,
    };
    let packet = encoder.encode(&frame).unwrap();
    assert!(packet.is_keyframe);
}