//! Input injection for remote control — injects mouse/keyboard events into the host OS.
//!
//! On Windows, uses SendInput API with absolute coordinates for mouse movement.
//! On other platforms, logs a warning and returns Ok (no-op).

use cntdesk_core::protocol::{InputEvent, InputType};

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
        MOUSEINPUT,
    };
    use windows::Win32::Graphics::Gdi::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    pub fn inject_input(event: &InputEvent) -> Result<(), String> {
        match event.event_type {
            InputType::MouseMove => {
                if event.data.len() < 4 {
                    return Err("MouseMove requires 4 bytes (x,y)".into());
                }
                let x = i16::from_le_bytes([event.data[0], event.data[1]]) as i32;
                let y = i16::from_le_bytes([event.data[2], event.data[3]]) as i32;

                // Normalize to 0..65535 range for MOUSEEVENTF_ABSOLUTE
                let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                let abs_x = if screen_w > 0 {
                    ((x as f64 / screen_w as f64) * 65535.0) as i32
                } else {
                    x
                };
                let abs_y = if screen_h > 0 {
                    ((y as f64 / screen_h as f64) * 65535.0) as i32
                } else {
                    y
                };

                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: abs_x,
                            dy: abs_y,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };

                unsafe {
                    let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    if result == 0 {
                        return Err("SendInput failed for MouseMove".into());
                    }
                }
                Ok(())
            }
            InputType::MouseDown => {
                let flags = if !event.data.is_empty() && event.data[0] == 1 {
                    MOUSEEVENTF_RIGHTDOWN
                } else {
                    MOUSEEVENTF_LEFTDOWN
                };

                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };

                unsafe {
                    let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    if result == 0 {
                        return Err("SendInput failed for MouseDown".into());
                    }
                }
                Ok(())
            }
            InputType::MouseUp => {
                let flags = if !event.data.is_empty() && event.data[0] == 1 {
                    MOUSEEVENTF_RIGHTUP
                } else {
                    MOUSEEVENTF_LEFTUP
                };

                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };

                unsafe {
                    let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    if result == 0 {
                        return Err("SendInput failed for MouseUp".into());
                    }
                }
                Ok(())
            }
            InputType::Scroll => {
                if event.data.len() < 2 {
                    return Err("Scroll requires 2 bytes".into());
                }
                let delta = i16::from_le_bytes([event.data[0], event.data[1]]) as i32;

                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: delta as u32,
                            dwFlags: MOUSEEVENTF_WHEEL,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };

                unsafe {
                    let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    if result == 0 {
                        return Err("SendInput failed for Scroll".into());
                    }
                }
                Ok(())
            }
            InputType::KeyDown | InputType::KeyUp => {
                if event.data.len() < 2 {
                    return Err("Key event requires 2 bytes (keycode)".into());
                }
                let keycode = u16::from_le_bytes([event.data[0], event.data[1]]);

                let mut flags = KEYEVENTF_SCANCODE;
                if matches!(event.event_type, InputType::KeyUp) {
                    flags |= KEYEVENTF_KEYUP;
                }

                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: Default::default(),
                            wScan: keycode,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };

                unsafe {
                    let result = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                    if result == 0 {
                        return Err(format!("SendInput failed for {:?}", event.event_type));
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod stub_impl {
    use super::*;

    pub fn inject_input(event: &InputEvent) -> Result<(), String> {
        tracing::warn!(
            event_type = ?event.event_type,
            "Input injection not implemented for this platform"
        );
        Ok(())
    }
}

/// Inject an input event into the host OS.
pub fn inject(event: &InputEvent) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::inject_input(event)
    }

    #[cfg(not(target_os = "windows"))]
    {
        stub_impl::inject_input(event)
    }
}