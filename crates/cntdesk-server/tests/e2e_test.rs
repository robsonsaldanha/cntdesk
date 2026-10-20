mod fixtures;

use fixtures::{TestServer, TestClient, TestHarness};
use regex::Regex;
use cntdesk_core::protocol::{Message, InputEvent, InputType};

// ── Phase 0 regression tests (preserved) ──

#[tokio::test]
async fn test_full_connection_flow() {
    let server = TestServer::start();
    let mut host = TestClient::new();
    let mut viewer = TestClient::new();

    let host_id = host.register(&server).unwrap();
    assert!(
        Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&host_id),
        "ID format must be XXX-XXX-XXX, got: {}",
        host_id
    );

    host.set_permanent_password(&server, "testpass123").unwrap();
    let _viewer_id = viewer.register(&server).unwrap();

    let valid = host.verify_password(&server, "testpass123").unwrap();
    assert!(valid, "Correct password must verify");
}

#[tokio::test]
async fn test_wrong_password_rejected() {
    let server = TestServer::start();
    let mut host = TestClient::new();

    let host_id = host.register(&server).unwrap();
    host.set_permanent_password(&server, "correctpassword").unwrap();

    let result = host.verify_password(&server, "wrongpassword");
    match result {
        Ok(false) => {}
        Err(_) => {}
        Ok(true) => panic!("Wrong password must not verify for device {}", host_id),
    }
}

#[tokio::test]
async fn test_duplicate_id_atomicity() {
    use std::sync::Arc;
    use tokio::task;

    let server = Arc::new(TestServer::start());
    let mut handles = vec![];

    for i in 0..10 {
        let srv = Arc::clone(&server);
        let handle = task::spawn(async move {
            let client = TestClient::new();
            let id = srv.auth.register_new_device(&client.keypair.public_key_bytes())
                .expect(&format!("Registration {} failed", i));
            id
        });
        handles.push(handle);
    }

    let mut ids = vec![];
    for handle in handles {
        let id = handle.await.unwrap();
        ids.push(id);
    }

    assert_eq!(ids.len(), 10, "All 10 registrations must succeed");

    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 10, "All IDs must be unique, got {} duplicates", 10 - unique.len());

    let re = Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap();
    for id in &ids {
        assert!(re.is_match(id), "ID '{}' does not match XXX-XXX-XXX format", id);
    }
}

// ── Phase 1 streaming e2e tests ──

#[tokio::test]
async fn test_e2e_video_streaming_via_relay() {
    let harness = TestHarness::setup("streamtest123").await;

    // Host sends 10 FrameData messages through relay
    for seq in 0..10u64 {
        let msg = Message::FrameData {
            seq,
            data: vec![0xAB; 1024],
            is_keyframe: seq == 0,
        };
        harness.host_send_to_viewer(msg).await.expect("host send failed");
    }

    // Viewer drains pending messages and verifies all 10 arrived in order
    let received = harness.drain_pending().await;
    assert_eq!(received.len(), 10, "Viewer must receive all 10 frames");

    for (i, msg) in received.iter().enumerate() {
        match msg {
            Message::FrameData { seq, data, is_keyframe } => {
                assert_eq!(*seq, i as u64, "Frame seq mismatch at index {}", i);
                assert_eq!(data.len(), 1024, "Frame data size mismatch at seq {}", seq);
                if i == 0 {
                    assert!(*is_keyframe, "First frame must be keyframe");
                }
            }
            other => panic!("Expected FrameData at index {}, got {:?}", i, other),
        }
    }
}

#[tokio::test]
async fn test_e2e_input_forwarding_via_relay() {
    let harness = TestHarness::setup("inputtest123").await;

    let input_event = InputEvent {
        event_type: InputType::MouseMove,
        timestamp_us: 12345,
        data: vec![100, 0, 200, 0], // x=100, y=200
    };

    let msg = Message::InputEvent(input_event.clone());
    harness.viewer_send_to_host(msg).await.expect("viewer send failed");

    let received = harness.drain_pending().await;
    assert_eq!(received.len(), 1, "Host must receive exactly 1 input event");

    match &received[0] {
        Message::InputEvent(evt) => {
            assert_eq!(evt.event_type, InputType::MouseMove);
            assert_eq!(evt.timestamp_us, 12345);
            assert_eq!(evt.data, vec![100, 0, 200, 0]);
        }
        other => panic!("Expected InputEvent, got {:?}", other),
    }
}

#[tokio::test]
async fn test_e2e_graceful_disconnect() {
    let harness = TestHarness::setup("disctest123").await;

    // Verify session exists before disconnect
    let bw_before = harness.server.signaling.get_bandwidth(&harness.session_id).await;
    assert!(bw_before.is_some(), "Session must exist before disconnect");

    // Viewer sends Disconnect message
    let msg = Message::Disconnect {
        reason: "user_requested".to_string(),
    };
    harness.viewer_send_to_host(msg).await.expect("disconnect send failed");

    // Drain to process the disconnect through signaling
    let received = harness.drain_pending().await;
    assert_eq!(received.len(), 1, "Host must receive disconnect message");

    match &received[0] {
        Message::Disconnect { reason } => {
            assert_eq!(reason, "user_requested");
        }
        other => panic!("Expected Disconnect, got {:?}", other),
    }

    // Close the session in signaling (simulates what forward_loop does on Disconnect)
    harness.server.signaling.close_session(&harness.session_id).await;

    // Verify session is gone
    let bw_after = harness.server.signaling.get_bandwidth(&harness.session_id).await;
    assert!(bw_after.is_none(), "Session must be cleaned up after disconnect");
}

#[tokio::test]
async fn test_e2e_wrong_password_still_rejected() {
    let harness = TestHarness::setup("correctpass123").await;

    // Create a new client that tries to authenticate with wrong password
    let mut bad_viewer = TestClient::new();
    bad_viewer.register(&harness.server).expect("bad viewer registration failed");

    let result = bad_viewer.authenticate(&harness.server, "wrongpassword");
    assert!(result.is_err(), "Wrong password must be rejected during authentication");
}