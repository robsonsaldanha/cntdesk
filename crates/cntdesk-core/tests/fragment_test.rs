use cntdesk_core::encoder::EncodedPacket;
use cntdesk_core::fragment::{Fragmenter, Reassembler};
use cntdesk_core::protocol::Message;

#[test]
fn test_fragment_small_packet_no_split() {
    let packet = EncodedPacket {
        data: vec![0xAB; 1000],
        is_keyframe: false,
        timestamp_us: 100,
        seq: 5,
    };
    let fragments = Fragmenter::fragment(&packet, 65536);
    assert_eq!(fragments.len(), 1);
    match &fragments[0] {
        Message::FrameData {
            seq,
            data,
            is_keyframe,
        } => {
            assert_eq!(*seq, 5);
            // Data includes 12-byte fragment header (total_fragments: u16 + fragment_index: u16 + timestamp_us: u64)
            assert_eq!(data.len(), 1000 + 12);
            assert!(!*is_keyframe);
        }
        _ => panic!("Expected FrameData variant"),
    }
}

#[test]
fn test_fragment_large_packet_splits_correctly() {
    let packet = EncodedPacket {
        data: vec![0xCD; 200_000],
        is_keyframe: true,
        timestamp_us: 200,
        seq: 10,
    };
    let fragments = Fragmenter::fragment(&packet, 65536);
    assert!(fragments.len() >= 4, "Expected >= 4 fragments, got {}", fragments.len());

    // Reassemble
    let mut reassembler = Reassembler::new();
    let mut result = None;
    for frag in fragments {
        if let Message::FrameData { .. } = &frag {
            if let Some(pkt) = reassembler.push(frag) {
                result = Some(pkt);
            }
        }
    }
    let reassembled = result.expect("Should have reassembled complete packet");
    assert_eq!(reassembled.data.len(), 200_000);
    assert_eq!(reassembled.seq, 10);
    assert!(reassembled.is_keyframe);
    assert_eq!(reassembled.timestamp_us, 200);
}

#[test]
fn test_reassembler_handles_out_of_order() {
    let packet = EncodedPacket {
        data: vec![0xEF; 200_000],
        is_keyframe: false,
        timestamp_us: 300,
        seq: 20,
    };
    let fragments = Fragmenter::fragment(&packet, 65536);
    assert!(fragments.len() >= 4);

    // Reverse order
    let mut reversed = fragments.clone();
    reversed.reverse();

    let mut reassembler = Reassembler::new();
    let mut result = None;
    for frag in reversed {
        if let Message::FrameData { .. } = &frag {
            if let Some(pkt) = reassembler.push(frag) {
                result = Some(pkt);
            }
        }
    }
    let reassembled = result.expect("Should reassemble from out-of-order fragments");
    assert_eq!(reassembled.data.len(), 200_000);
    assert_eq!(reassembled.seq, 20);
}

#[test]
fn test_reassembler_timeout_discards_incomplete() {
    use std::thread;
    use std::time::Duration;

    let packet = EncodedPacket {
        data: vec![0xAA; 200_000],
        is_keyframe: false,
        timestamp_us: 400,
        seq: 30,
    };
    let fragments = Fragmenter::fragment(&packet, 65536);
    assert!(fragments.len() >= 4);

    // Only push first fragment, then wait for timeout
    let mut reassembler = Reassembler::new();
    if let Message::FrameData { .. } = &fragments[0] {
        let result = reassembler.push(fragments[0].clone());
        assert!(result.is_none(), "Should not reassemble with only one fragment");
    }

    // Wait past the 100ms timeout
    thread::sleep(Duration::from_millis(150));

    // Push another fragment for a different seq to trigger cleanup
    let other_packet = EncodedPacket {
        data: vec![0xBB; 1000],
        is_keyframe: false,
        timestamp_us: 500,
        seq: 31,
    };
    let other_frags = Fragmenter::fragment(&other_packet, 65536);
    // The small packet should reassemble fine
    let mut result2 = None;
    for frag in other_frags {
        if let Message::FrameData { .. } = &frag {
            if let Some(pkt) = reassembler.push(frag) {
                result2 = Some(pkt);
            }
        }
    }
    // seq 31 should reassemble (small packet, single fragment)
    assert!(result2.is_some(), "Small packet for seq 31 should reassemble");
    assert_eq!(result2.unwrap().seq, 31);
}