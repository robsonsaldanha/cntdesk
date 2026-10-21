//! Windows screen capture backend using DXGI Desktop Duplication API.
//!
//! Captures real screen frames via IDXGIOutputDuplication, returning BGRA
//! pixel data suitable for encoding or direct transmission.

use std::time::{SystemTime, UNIX_EPOCH};

use cntdesk_core::error::CoreError;
use cntdesk_core::frame::{CapturedFrame, PixelFormat};
use crate::capture::{CaptureConfig, ScreenCaptureBackend};

#[cfg(target_os = "windows")]
mod dxgi_impl {
    use super::*;
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIAdapter1, IDXGIOutput, IDXGIOutput1,
        IDXGIOutputDuplication, IDXGIResource, DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_OUTPUT_DESC, DXGI_OUTDUPL_FRAME_INFO,
        DXGI_ERROR_WAIT_TIMEOUT, DXGI_ERROR_ACCESS_LOST,
    };
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_SDK_VERSION, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D_FEATURE_LEVEL_11_0,
        D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11_CPU_ACCESS_READ,
        D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    };
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL};
    use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

    pub struct DxgiInner {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        duplication: IDXGIOutputDuplication,
        staging_texture: ID3D11Texture2D,
        width: u32,
        height: u32,
        running: bool,
        frame_seq: u64,
    }

    impl DxgiInner {
        pub fn new() -> Result<Self, CoreError> {
            unsafe {
                // Create DXGI factory and enumerate adapters
                let factory: IDXGIFactory1 = CreateDXGIFactory1()
                    .map_err(|e| CoreError::Protocol(format!("CreateDXGIFactory1 failed: {}", e)))?;

                let adapter: IDXGIAdapter1 = factory.EnumAdapters1(0)
                    .map_err(|e| CoreError::Protocol(format!("EnumAdapters1 failed: {}", e)))?;

                let output: IDXGIOutput = adapter.EnumOutputs(0)
                    .map_err(|e| CoreError::Protocol(format!("EnumOutputs failed: {}", e)))?;

                let output1: IDXGIOutput1 = output.cast::<IDXGIOutput1>()
                    .map_err(|e| CoreError::Protocol(format!("Cast to IDXGIOutput1 failed: {}", e)))?;

                // Get output description for dimensions
                let mut desc = DXGI_OUTPUT_DESC::default();
                output.GetDesc(&mut desc)
                    .map_err(|e| CoreError::Protocol(format!("GetDesc failed: {}", e)))?;

                let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
                let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;

                // Create D3D11 device with BGRA support
                let mut feature_level = D3D_FEATURE_LEVEL::default();
                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;

                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    Some(&mut context),
                ).map_err(|e| CoreError::Protocol(format!("D3D11CreateDevice failed: {}", e)))?;

                let device = device.ok_or_else(|| CoreError::Protocol("D3D11CreateDevice returned null device".into()))?;
                let context = context.ok_or_else(|| CoreError::Protocol("D3D11CreateDevice returned null context".into()))?;

                // Create desktop duplication
                let duplication = output1.DuplicateOutput(&device)
                    .map_err(|e| CoreError::Protocol(format!("DuplicateOutput failed: {}", e)))?;

                // Create staging texture for CPU readback
                let tex_desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: Default::default(),
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0,
                    MiscFlags: Default::default(),
                };

                let mut staging_texture: Option<ID3D11Texture2D> = None;
                device.CreateTexture2D(&tex_desc, None, Some(&mut staging_texture))
                    .map_err(|e| CoreError::Protocol(format!("CreateTexture2D failed: {}", e)))?;
                let staging_texture = staging_texture.ok_or_else(|| CoreError::Protocol("CreateTexture2D returned null".into()))?;

                Ok(Self {
                    device,
                    context,
                    duplication,
                    staging_texture,
                    width,
                    height,
                    running: false,
                    frame_seq: 0,
                })
            }
        }

        pub fn capture_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
            if !self.running {
                return Ok(None);
            }

            unsafe {
                // Acquire next frame with 16ms timeout (~60fps max)
                let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<IDXGIResource> = None;

                let result = self.duplication.AcquireNextFrame(
                    16,
                    &mut frame_info,
                    &mut resource,
                );

                match result {
                    Ok(()) => {
                        if let Some(res) = resource {
                            let desktop_texture: ID3D11Texture2D = res.cast::<ID3D11Texture2D>()
                                .map_err(|e| CoreError::Protocol(format!("Cast to texture failed: {}", e)))?;

                            // Copy desktop texture to staging texture
                            self.context.CopyResource(&self.staging_texture, &desktop_texture);

                            // Map staging texture for CPU read
                            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                            self.context.Map(
                                &self.staging_texture,
                                0,
                                D3D11_MAP_READ,
                                0,
                                Some(&mut mapped),
                            ).map_err(|e| CoreError::Protocol(format!("Map failed: {}", e)))?;

                            // Copy pixel data (BGRA, row-pitched)
                            let pitch = mapped.RowPitch as usize;
                            let expected_pitch = self.width as usize * 4;
                            let mut data = vec![0u8; (self.width * self.height * 4) as usize];

                            if pitch == expected_pitch {
                                // Fast path: no padding
                                std::ptr::copy_nonoverlapping(
                                    mapped.pData as *const u8,
                                    data.as_mut_ptr(),
                                    data.len(),
                                );
                            } else {
                                // Slow path: copy row by row
                                for y in 0..self.height as usize {
                                    let src_offset = y * pitch;
                                    let dst_offset = y * expected_pitch;
                                    std::ptr::copy_nonoverlapping(
                                        (mapped.pData as *const u8).add(src_offset),
                                        data.as_mut_ptr().add(dst_offset),
                                        expected_pitch,
                                    );
                                }
                            }

                            self.context.Unmap(&self.staging_texture, 0);

                            // Release frame
                            let _ = self.duplication.ReleaseFrame();

                            let timestamp_us = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_micros() as u64;

                            self.frame_seq += 1;

                            Ok(Some(CapturedFrame {
                                data,
                                width: self.width,
                                height: self.height,
                                format: PixelFormat::Bgra,
                                timestamp_us,
                            }))
                        } else {
                            Ok(None)
                        }
                    }
                    Err(e) => {
                        // DXGI_ERROR_WAIT_TIMEOUT is normal when screen hasn't changed
                        let code = e.code();
                        if code == DXGI_ERROR_WAIT_TIMEOUT {
                            Ok(None)
                        } else if code == DXGI_ERROR_ACCESS_LOST {
                            // Display mode changed, need to recreate duplication
                            tracing::warn!("DXGI access lost, will retry");
                            let _ = self.duplication.ReleaseFrame();
                            Ok(None)
                        } else {
                            Err(CoreError::Protocol(format!("AcquireNextFrame error: {}", e)))
                        }
                    }
                }
            }
        }

        pub fn start(&mut self) {
            self.running = true;
            self.frame_seq = 0;
            tracing::info!(
                width = self.width,
                height = self.height,
                "DXGI Desktop Duplication capture started"
            );
        }

        pub fn stop(&mut self) {
            self.running = false;
            tracing::info!("DXGI capture stopped");
        }

        pub fn resize(&mut self, max_width: u32, max_height: u32) {
            // DXGI captures at native resolution; downscaling handled post-capture if needed
            tracing::debug!(
                max_width,
                max_height,
                native_width = self.width,
                native_height = self.height,
                "Resize requested (native resolution used)"
            );
        }
    }
}

/// DXGI-based screen capture for Windows using Desktop Duplication API.
pub struct DxgiCapture {
    #[cfg(target_os = "windows")]
    inner: Option<dxgi_impl::DxgiInner>,
    #[cfg(not(target_os = "windows"))]
    _phantom: (),
    fallback: FallbackCapture,
}

/// Synthetic fallback for non-Windows builds or when DXGI init fails.
struct FallbackCapture {
    width: u32,
    height: u32,
    running: bool,
    frame_seq: u64,
}

impl FallbackCapture {
    fn new() -> Self {
        Self {
            width: 1920,
            height: 1080,
            running: false,
            frame_seq: 0,
        }
    }

    fn generate_synthetic_frame(&mut self) -> CapturedFrame {
        let timestamp_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let mut data = vec![0u8; (self.width * self.height * 4) as usize];
        let offset = (self.frame_seq % 256) as u8;
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = ((y * self.width + x) * 4) as usize;
                data[idx] = ((x as u8).wrapping_add(offset)) as u8;
                data[idx + 1] = (y as u8).wrapping_add(offset) as u8;
                data[idx + 2] = offset;
                data[idx + 3] = 255;
            }
        }

        self.frame_seq += 1;

        CapturedFrame {
            data,
            width: self.width,
            height: self.height,
            format: PixelFormat::Bgra,
            timestamp_us,
        }
    }
}

impl DxgiCapture {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            match dxgi_impl::DxgiInner::new() {
                Ok(inner) => {
                    tracing::info!("DXGI Desktop Duplication initialized successfully");
                    Self {
                        inner: Some(inner),
                        fallback: FallbackCapture::new(),
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "DXGI init failed, falling back to synthetic capture"
                    );
                    Self {
                        inner: None,
                        fallback: FallbackCapture::new(),
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Self {
                _phantom: (),
                fallback: FallbackCapture::new(),
            }
        }
    }
}

impl Default for DxgiCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCaptureBackend for DxgiCapture {
    fn start(&mut self, config: &CaptureConfig) -> Result<(), CoreError> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut inner) = self.inner {
                if config.max_width > 0 && config.max_height > 0 {
                    inner.resize(config.max_width, config.max_height);
                }
                inner.start();
                return Ok(());
            }
        }

        // Fallback path
        if config.max_width > 0 && config.max_height > 0 {
            self.fallback.width = config.max_width;
            self.fallback.height = config.max_height;
        }
        self.fallback.running = true;
        self.fallback.frame_seq = 0;
        tracing::info!(
            width = self.fallback.width,
            height = self.fallback.height,
            "Using synthetic fallback capture"
        );
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CoreError> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut inner) = self.inner {
                return inner.capture_frame();
            }
        }

        // Fallback path
        if !self.fallback.running {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        Ok(Some(self.fallback.generate_synthetic_frame()))
    }

    fn stop(&mut self) -> Result<(), CoreError> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref mut inner) = self.inner {
                inner.stop();
                return Ok(());
            }
        }

        self.fallback.running = false;
        tracing::info!("Capture stopped (fallback)");
        Ok(())
    }
}