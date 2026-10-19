use cntdesk_core::protocol::Message;
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