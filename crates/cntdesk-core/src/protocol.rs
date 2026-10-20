//! Core protocol types for CNT Desk

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Input event type enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputType {
    MouseMove,
    MouseDown,
    MouseUp,
    KeyDown,
    KeyUp,
    Scroll,
}

/// Input event with timestamp and data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputEvent {
    pub event_type: InputType,
    pub timestamp_us: u64,
    pub data: Vec<u8>,
}

/// Protocol message enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    AuthRequest {
        id: String,
        password_hash: Vec<u8>,
    },
    AuthResponse {
        success: bool,
        session_token: Option<Vec<u8>>,
    },
    IceCandidate {
        candidate: String,
    },
    FrameMetadata {
        seq: u64,
        timestamp_us: u64,
        width: u16,
        height: u16,
        keyframe: bool,
    },
    FrameData {
        seq: u64,
        data: Vec<u8>,
        is_keyframe: bool,
    },
    InputEvent(InputEvent),
    Disconnect {
        reason: String,
    },
}

impl Message {
    /// Serialize message to bytes using bincode
    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Message serialization should not fail")
    }

    /// Deserialize message from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, CoreError> {
        bincode::deserialize(bytes).map_err(CoreError::from)
    }
}