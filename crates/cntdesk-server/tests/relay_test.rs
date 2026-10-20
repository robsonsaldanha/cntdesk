use cntdesk_core::crypto::KeyPair;
use cntdesk_core::protocol::Message;
use cntdesk_server::auth::AuthService;
use cntdesk_server::db::Database;
use cntdesk_server::signaling::SignalingService;

#[tokio::test]
async fn test_signaling_routes_ice_candidates() {
    let svc = SignalingService::new_test();

    // Create a session first
    svc.create_session("peer-a", "peer-b").await.unwrap();

    let session_id = "peer-a-peer-b";

    // Route an ICE candidate message
    svc.route_message(
        session_id,
        Message::IceCandidate {
            candidate: "test".into(),
        },
    )
    .await
    .unwrap();

    let routed = svc.get_pending_messages(session_id).await;
    assert_eq!(routed.len(), 1);
    assert_eq!(
        routed[0],
        Message::IceCandidate {
            candidate: "test".into()
        }
    );
}

#[tokio::test]
async fn test_signaling_get_pending_drains_queue() {
    let svc = SignalingService::new_test();
    svc.create_session("a", "b").await.unwrap();

    let sid = "a-b";
    svc.route_message(sid, Message::Disconnect { reason: "x".into() })
        .await
        .unwrap();
    svc.route_message(
        sid,
        Message::IceCandidate {
            candidate: "y".into(),
        },
    )
    .await
    .unwrap();

    let msgs = svc.get_pending_messages(sid).await;
    assert_eq!(msgs.len(), 2);

    // Second call should be empty (drained)
    let msgs2 = svc.get_pending_messages(sid).await;
    assert_eq!(msgs2.len(), 0);
}

#[tokio::test]
async fn test_signaling_route_to_nonexistent_session_fails() {
    let svc = SignalingService::new_test();
    let result = svc
        .route_message(
            "nonexistent",
            Message::IceCandidate {
                candidate: "test".into(),
            },
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_signaling_close_session() {
    let svc = SignalingService::new_test();
    svc.create_session("x", "y").await.unwrap();
    svc.close_session("x-y").await;

    let result = svc
        .route_message(
            "x-y",
            Message::IceCandidate {
                candidate: "z".into(),
            },
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_signaling_bandwidth_tracking() {
    let svc = SignalingService::new_test();
    svc.create_session("p1", "p2").await.unwrap();

    let sid = "p1-p2";
    svc.route_message(
        sid,
        Message::IceCandidate {
            candidate: "candidate-data-here".into(),
        },
    )
    .await
    .unwrap();

    let bw = svc.get_bandwidth(sid).await;
    assert!(bw.is_some());
    assert!(bw.unwrap() > 0);
}

// --- Task 3: Bidirectional forwarding & session creation tests ---

#[tokio::test]
async fn test_relay_creates_session_on_dual_auth() {
    // Setup: server with in-memory DB, two devices registered with passwords
    let db = Database::open_in_memory("test").unwrap();
    let auth = AuthService::new(db);
    let signaling = SignalingService::new_test();

    let kp_a = KeyPair::generate();
    let kp_b = KeyPair::generate();
    let id_a = auth.register_new_device(&kp_a.public_key_bytes()).unwrap();
    let id_b = auth.register_new_device(&kp_b.public_key_bytes()).unwrap();
    auth.set_permanent_password(&id_a, b"password_a_1234").unwrap();
    auth.set_permanent_password(&id_b, b"password_b_1234").unwrap();

    // Simulate peer A authenticating — should register as waiting
    // Simulate peer B authenticating — should create session with A
    // We test via signaling state since handle_connection requires a real QUIC connection.
    // Direct test of session pairing logic through signaling + waiting peers map.

    // Register peer A as waiting
    signaling.register_waiting_peer(&id_a).await;
    assert!(signaling.has_waiting_peer(&id_a).await);

    // Peer B arrives — create session between B and waiting peer A
    let paired_id = signaling.pair_with_waiting(&id_b).await;
    assert!(paired_id.is_some());
    let session_id = paired_id.unwrap();

    // Verify session exists in signaling
    let pending = signaling.get_pending_messages(&session_id).await;
    assert_eq!(pending.len(), 0); // fresh session, no messages yet

    // Peer A should no longer be waiting
    assert!(!signaling.has_waiting_peer(&id_a).await);
}

#[tokio::test]
async fn test_relay_forwards_frame_data_between_peers() {
    let signaling = SignalingService::new_test();

    // Create a session directly
    let session_id = signaling.create_session("host-001", "viewer-001").await.unwrap();

    // Host sends FrameData
    let frame_msg = Message::FrameData {
        seq: 42,
        data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        is_keyframe: true,
    };
    signaling.route_message(&session_id, frame_msg.clone()).await.unwrap();

    // Viewer retrieves pending messages
    let msgs = signaling.get_pending_messages(&session_id).await;
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], frame_msg);
}

#[tokio::test]
async fn test_relay_rejects_expired_session_token() {
    let db = Database::open_in_memory("test").unwrap();
    let auth = AuthService::new(db);

    let keypair = KeyPair::generate();
    let id = auth.register_new_device(&keypair.public_key_bytes()).unwrap();
    auth.set_permanent_password(&id, b"testpassword123").unwrap();

    // Create token then expire it
    let _token = auth.create_session_token(&id, &keypair).unwrap();
    auth.expire_session_token_for_test(&id, 0).unwrap();

    // Verification must fail for expired token
    // We need the original token bytes to verify — recreate with known expiry
    // Since create_session_token stores in DB, we can retrieve and verify
    // Actually, we already created and expired it. Let's create a fresh one, expire it, then verify.
    let keypair2 = KeyPair::generate();
    let id2 = auth.register_new_device(&keypair2.public_key_bytes()).unwrap();
    let token2 = auth.create_session_token(&id2, &keypair2).unwrap();
    auth.expire_session_token_for_test(&id2, 0).unwrap();

    let result = auth.verify_session_token(&token2);
    assert!(result.is_err(), "Expired token should be rejected");
}