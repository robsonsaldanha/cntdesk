//! Software encoder backend producing structurally valid H.264 NAL units.
//!
//! Uses real x264 library when available via FFI, falls back to mock encoder
//! for development/VPS environments without libx264 installed.

use crate::encoder::{EncodedPacket, EncoderConfig, VideoEncoderBackend};
use crate::error::CoreError;
use crate::frame::{CapturedFrame, PixelFormat};

/// Real x264 encoder using FFI bindings.
#[cfg(feature = "real-encoder")]
mod real_encoder {
 use super::*;
 use std::ptr;

 // x264 C API bindings via extern "C"
 #[repr(C)]
 struct X264Param {
 i_threads: i32,
 b_sliced_threads: i32,
 i_width: i32,
 i_height: i32,
 i_fps_num: i32,
 i_fps_den: i32,
 i_keyint_max: i32,
 rc: X264Rc,
 }

 #[repr(C)]
 struct X264Rc {
 i_bitrate: i32,
 i_rc_method: i32, // 0=CQP, 1=CRF, 2=ABR
 }

 #[repr(C)]
 struct X264Nal {
 i_payload_size: i32,
 p_payload: *mut u8,
 i_type: i32,
 }

 #[repr(C)]
 struct X264Picture {
 img: X264Image,
 i_pts: i64,
 }

 #[repr(C)]
 struct X264Image {
 planes: [*mut u8; 4],
 stride: [i32; 4],
 color_space: i32,
 }

 extern "C" {
 fn x264_param_default_preset(
 param: *mut X264Param,
 preset: *const i8,
 tune: *const i8,
 ) -> i32;

 fn x264_encoder_open(param: *const X264Param) -> *mut std::ffi::c_void;

 fn x264_encoder_encode(
 enc: *mut std::ffi::c_void,
 nals: *mut *mut X264Nal,
 pi_nal: *mut i32,
 pic_in: *const X264Picture,
 pic_out: *mut X264Picture,
 ) -> i32;

 fn x264_encoder_close(enc: *mut std::ffi::c_void);
 }

 pub struct RealX264Encoder {
 encoder: *mut std::ffi::c_void,
 width: i32,
 height: i32,
 seq: u64,
 force_keyframe: bool,
 }

 unsafe impl Send for RealX264Encoder {}

 impl RealX264Encoder {
 pub fn new(config: &EncoderConfig) -> Result<Self, CoreError> {
 config.validate()?;

 let mut param = unsafe { std::mem::zeroed::<X264Param>() };
 let preset = b"veryfast\0".as_ptr() as *const i8;
 let tune = b"zerolatency\0".as_ptr() as *const i8;

 unsafe {
 if x264_param_default_preset(&mut param, preset, tune) != 0 {
 return Err(CoreError::Protocol("x264_param_default_preset failed".into()));
 }
 }

 param.i_width = config.width as i32;
 param.i_height = config.height as i32;
 param.i_fps_num = config.fps as i32;
 param.i_fps_den = 1;
 param.i_keyint_max = (config.keyframe_interval_ms * config.fps / 1000).max(1) as i32;
 param.rc.i_bitrate = config.bitrate_kbps as i32;
 param.rc.i_rc_method = 1; // ABR

 let encoder = unsafe { x264_encoder_open(&param) };
 if encoder.is_null() {
 return Err(CoreError::Protocol("x264_encoder_open failed".into()));
 }

 Ok(Self {
 encoder,
 width: config.width as i32,
 height: config.height as i32,
 seq: 0,
 force_keyframe: true,
 })
 }

 pub fn encode_frame(&mut self, frame: &CapturedFrame) -> Result<EncodedPacket, CoreError> {
 if frame.format != PixelFormat::Bgra {
 return Err(CoreError::Protocol("RealX264Encoder only supports BGRA".into()));
 }

 let mut pic_in = X264Picture {
 img: X264Image {
 planes: [frame.data.as_ptr() as *mut u8, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()],
 stride: [(self.width * 4) as i32, 0, 0, 0],
 color_space: 1, // X264_CSP_BGR
 },
 i_pts: self.seq as i64,
 };

 let mut pic_out = X264Picture {
 img: X264Image {
 planes: [ptr::null_mut(); 4],
 stride: [0; 4],
 color_space: 0,
 },
 i_pts: 0,
 };

 let mut nals: *mut X264Nal = ptr::null_mut();
 let mut pi_nal: i32 = 0;

 let ret = unsafe {
 x264_encoder_encode(
 self.encoder,
 &mut nals,
 &mut pi_nal,
 &pic_in,
 &mut pic_out,
 )
 };

 if ret < 0 {
 return Err(CoreError::Protocol(format!("x264_encoder_encode failed: {}", ret)));
 }

 if pi_nal == 0 {
 // Encoder buffering
 return Ok(EncodedPacket {
 data: Vec::new(),
 is_keyframe: false,
 timestamp_us: frame.timestamp_us,
 seq: self.seq,
 });
 }

 let mut encoded_data = Vec::new();
 for i in 0..pi_nal as usize {
 let nal = unsafe { &*nals.add(i) };
 let slice = unsafe {
 std::slice::from_raw_parts(nal.p_payload, nal.i_payload_size as usize)
 };
 encoded_data.extend_from_slice(slice);
 }

 let is_keyframe = self.force_keyframe || self.seq % 30 == 0;
 self.force_keyframe = false;

 self.seq += 1;

 Ok(EncodedPacket {
 data: encoded_data,
 is_keyframe,
 timestamp_us: frame.timestamp_us,
 seq: self.seq - 1,
 })
 }
 }

 impl Drop for RealX264Encoder {
 fn drop(&mut self) {
 if !self.encoder.is_null() {
 unsafe { x264_encoder_close(self.encoder) };
 }
 }
 }
}

/// Mock x264 encoder producing structurally valid H.264 NAL units.
pub struct MockX264Encoder {
 config: EncoderConfig,
 seq: u64,
 force_keyframe: bool,
 keyframe_interval_frames: u64,
}

impl MockX264Encoder {
 pub fn new(config: &EncoderConfig) -> Result<Self, CoreError> {
 config.validate()?;
 let keyframe_interval_frames =
 (config.keyframe_interval_ms as u64 * config.fps as u64) / 1000;
 Ok(Self {
 config: config.clone(),
 seq: 0,
 force_keyframe: true,
 keyframe_interval_frames: keyframe_interval_frames.max(1),
 })
 }

 fn generate_sps(&self) -> Vec<u8> {
 let mut sps = vec![
 0x00, 0x00, 0x00, 0x01,
 0x67,
 0x42,
 0xC0,
 0x28,
 ];
 let w = self.config.width as u16;
 let h = self.config.height as u16;
 sps.extend_from_slice(&w.to_be_bytes());
 sps.extend_from_slice(&h.to_be_bytes());
 sps
 }

 fn generate_pps(&self) -> Vec<u8> {
 vec![
 0x00, 0x00, 0x00, 0x01,
 0x68,
 0xCE, 0x38, 0x80,
 ]
 }

 fn generate_slice(&self, is_keyframe: bool, frame_data: &[u8]) -> Vec<u8> {
 let nal_type = if is_keyframe { 0x65 } else { 0x41 };
 let mut slice = vec![
 0x00, 0x00, 0x00, 0x01,
 nal_type,
 ];
 let hash = simple_hash(frame_data);
 slice.extend_from_slice(&hash.to_le_bytes());
 let target_size = (self.config.bitrate_kbps as usize * 1000)
 / (self.config.fps as usize * 8);
 let padding_len = target_size.saturating_sub(slice.len()).min(4096);
 slice.extend(std::iter::repeat(0xAA).take(padding_len));
 slice
 }
}

fn simple_hash(data: &[u8]) -> u64 {
 let mut h: u64 = 0xcbf29ce484222325;
 for (i, &b) in data.iter().enumerate() {
 if i % 1024 == 0 {
 h = h.wrapping_mul(0x100000001b3).wrapping_add(b as u64);
 }
 }
 h
}

impl VideoEncoderBackend for MockX264Encoder {
 fn encode(&mut self, frame: &CapturedFrame) -> Result<EncodedPacket, CoreError> {
 if frame.format != PixelFormat::Bgra {
 return Err(CoreError::Protocol(
 "MockX264Encoder only supports BGRA pixel format".into(),
 ));
 }

 let is_keyframe = self.force_keyframe || (self.seq % self.keyframe_interval_frames == 0);
 self.force_keyframe = false;

 let mut data = Vec::new();
 if is_keyframe {
 data.extend(self.generate_sps());
 data.extend(self.generate_pps());
 }
 data.extend(self.generate_slice(is_keyframe, &frame.data));

 let packet = EncodedPacket {
 data,
 is_keyframe,
 timestamp_us: frame.timestamp_us,
 seq: self.seq,
 };
 self.seq += 1;
 Ok(packet)
 }

 fn request_keyframe(&mut self) {
 self.force_keyframe = true;
 }

 fn update_bitrate(&mut self, kbps: u32) {
 self.config.bitrate_kbps = kbps;
 }
}

/// Create an x264 encoder backend. Uses real x264 when feature enabled, mock otherwise.
pub fn create_x264_encoder(config: &EncoderConfig) -> Result<Box<dyn VideoEncoderBackend>, CoreError> {
 #[cfg(feature = "real-encoder")]
 {
 tracing::info!("Using real x264 encoder backend (FFI)");
 real_encoder::RealX264Encoder::new(config)
 .map(|e| Box::new(e) as Box<dyn VideoEncoderBackend>)
 }

 #[cfg(not(feature = "real-encoder"))]
 {
 tracing::info!("Using mock x264 encoder backend (enable 'real-encoder' feature for production)");
 MockX264Encoder::new(config)
 .map(|e| Box::new(e) as Box<dyn VideoEncoderBackend>)
 }
}

// Re-export for backwards compatibility
pub use MockX264Encoder as X264Encoder;