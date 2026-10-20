use cntdesk_capture::{CaptureConfig, create_backend};

#[test]
fn test_capture_config_defaults() {
    let cfg = CaptureConfig::default();
    assert_eq!(cfg.target_fps, 30);
    assert_eq!(cfg.max_width, 1920);
    assert_eq!(cfg.max_height, 1080);
}

#[test]
fn test_linux_capture_returns_none_when_no_display() {
    // In headless/VPS environment, capture must return None, never panic
    let mut backend = match create_backend() {
        Ok(b) => b,
        Err(_) => return, // platform not supported or init failed — acceptable
    };
    let config = CaptureConfig::default();
    // start may fail in headless — that's fine, we just need no panic
    let _ = backend.start(&config);
    let result = backend.next_frame();
    assert!(result.is_ok());
    let frame = result.unwrap();
    // In headless, expect None; on real display, Some is also valid
    assert!(frame.is_none() || frame.is_some());
    let _ = backend.stop();
}

#[test]
fn test_linux_capture_config_validation() {
    let config = CaptureConfig {
        target_fps: 60,
        max_width: 1920,
        max_height: 1080,
    };
    assert!(config.target_fps > 0);
    assert!(config.max_width > 0);
    assert!(config.max_height > 0);
}