use cntdesk_capture::CaptureConfig;

#[test]
fn test_capture_config_defaults() {
    let cfg = CaptureConfig::default();
    assert_eq!(cfg.target_fps, 30);
    assert_eq!(cfg.max_width, 1920);
    assert_eq!(cfg.max_height, 1080);
}