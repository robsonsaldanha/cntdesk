//! Frame fragmentation and reassembly for large video packets.
//!
//! Splits encoded packets exceeding the QUIC/UDP safe MTU into multiple
//! `Message::FrameData` fragments, and reassembles them on the receiver side
//! with out-of-order tolerance and a 100ms timeout for incomplete frames.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::encoder::EncodedPacket;
use crate::protocol::Message;

/// Timeout for incomplete frame reassembly.
const REASSEMBLY_TIMEOUT: Duration = Duration::from_millis(100);

/// Fragment header layout (12 bytes):
/// [total_fragments: u16][fragment_index: u16][timestamp_us: u64][payload...]
const HEADER_SIZE: usize = 12;

/// Splits an `EncodedPacket` into one or more `Message::FrameData` messages.
pub struct Fragmenter;

impl Fragmenter {
    /// Fragment an encoded packet into messages no larger than `max_size`.
    pub fn fragment(packet: &EncodedPacket, max_size: usize) -> Vec<Message> {
        let effective_max = if max_size <= HEADER_SIZE {
            HEADER_SIZE + 1
        } else {
            max_size
        };
        let chunk_size = effective_max - HEADER_SIZE;
        let total_fragments = if packet.data.is_empty() {
            1
        } else {
            (packet.data.len() + chunk_size - 1) / chunk_size
        };

        let mut messages = Vec::with_capacity(total_fragments);
        for i in 0..total_fragments {
            let start = i * chunk_size;
            let end = std::cmp::min(start + chunk_size, packet.data.len());
            let chunk = &packet.data[start..end];

            let mut data = Vec::with_capacity(HEADER_SIZE + chunk.len());
            data.extend_from_slice(&(total_fragments as u16).to_le_bytes());
            data.extend_from_slice(&(i as u16).to_le_bytes());
            data.extend_from_slice(&packet.timestamp_us.to_le_bytes());
            data.extend_from_slice(chunk);

            messages.push(Message::FrameData {
                seq: packet.seq,
                data,
                is_keyframe: packet.is_keyframe,
            });
        }
        messages
    }
}

/// Buffered state for a single in-progress frame reassembly.
struct PendingFrame {
    total_fragments: usize,
    received: Vec<Option<Vec<u8>>>,
    received_count: usize,
    is_keyframe: bool,
    timestamp_us: u64,
    created_at: Instant,
}

/// Reassembles fragmented `Message::FrameData` back into `EncodedPacket`s.
pub struct Reassembler {
    pending: HashMap<u64, PendingFrame>,
}

impl Reassembler {
    /// Create a new empty reassembler.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Push a `Message::FrameData` fragment. Returns `Some(EncodedPacket)` when
    /// all fragments for that seq have arrived, or `None` if still incomplete.
    /// Expired entries (>100ms) are cleaned up lazily on each call.
    pub fn push(&mut self, msg: Message) -> Option<EncodedPacket> {
        self.evict_expired();

        let (seq, data, is_keyframe) = match msg {
            Message::FrameData {
                seq,
                data,
                is_keyframe,
            } => (seq, data, is_keyframe),
            _ => return None,
        };

        if data.len() < HEADER_SIZE {
            return None;
        }

        let total_fragments = u16::from_le_bytes([data[0], data[1]]) as usize;
        let fragment_index = u16::from_le_bytes([data[2], data[3]]) as usize;
        let timestamp_us = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);
        let payload = &data[HEADER_SIZE..];

        if total_fragments == 0 || fragment_index >= total_fragments {
            return None;
        }

        let entry = self.pending.entry(seq).or_insert_with(|| PendingFrame {
            total_fragments,
            received: vec![None; total_fragments],
            received_count: 0,
            is_keyframe,
            timestamp_us,
            created_at: Instant::now(),
        });

        if entry.received.get(fragment_index).map_or(false, |slot| slot.is_none()) {
            entry.received[fragment_index] = Some(payload.to_vec());
            entry.received_count += 1;
        }

        if entry.received_count == entry.total_fragments {
            let pending = self.pending.remove(&seq)?;
            let mut assembled_data = Vec::new();
            for fragment in pending.received {
                if let Some(frag) = fragment {
                    assembled_data.extend_from_slice(&frag);
                }
            }
            Some(EncodedPacket {
                data: assembled_data,
                is_keyframe: pending.is_keyframe,
                timestamp_us: pending.timestamp_us,
                seq,
            })
        } else {
            None
        }
    }

    fn evict_expired(&mut self) {
        let now = Instant::now();
        self.pending
            .retain(|_, frame| now.duration_since(frame.created_at) < REASSEMBLY_TIMEOUT);
    }
}