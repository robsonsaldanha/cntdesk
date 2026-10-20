use cntdesk_core::protocol::{InputEvent, InputType, Message};

#[test]
fn test_message_roundtrip() {
    let msg = Message::AuthRequest {
        id: "123-456-789".into(),
        password_hash: vec![0u8; 32],
    };
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_input_event_roundtrip() {
    let msg = Message::InputEvent(InputEvent {
        event_type: InputType::MouseMove,
        timestamp_us: 123456789,
        data: vec![10, 20, 30],
    });
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_frame_metadata_roundtrip() {
    let msg = Message::FrameMetadata {
        seq: 42,
        timestamp_us: 999999,
        width: 1920,
        height: 1080,
        keyframe: true,
    };
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_deserialize_invalid_bytes_returns_error() {
    let result = Message::deserialize(&[0xFF, 0xFE]);
    assert!(result.is_err());
}

#[test]
fn test_frame_data_roundtrip() {
    let msg = Message::FrameData {
        seq: 42,
        data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        is_keyframe: true,
    };
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_frame_data_large_payload() {
    // Simula frame codificado ~256KB
    let msg = Message::FrameData {
        seq: 1,
        data: vec![0xAB; 262144],
        is_keyframe: false,
    };
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}