#![allow(unsafe_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum InputCmd {
    Text { text: String },
    Keys { keys: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClickCmd {
    pub x: i32,
    pub y: i32,
}

pub(crate) fn handle_input(body: &str) -> Result<(), String> {
    let cmd: InputCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    bring_editor_to_front()?;
    match cmd {
        InputCmd::Text { text } => platform::send_text(&text),
        InputCmd::Keys { keys } => platform::send_keys(&keys),
    }
}

pub(crate) fn handle_click(body: &str) -> Result<(), String> {
    let cmd: ClickCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    bring_editor_to_front()?;
    platform::send_click(cmd.x, cmd.y)
}

fn bring_editor_to_front() -> Result<(), String> {
    platform::bring_to_front()
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use std::time::Duration;

    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_TYPE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS,
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
        MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
        VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9,
        VK_HOME, VK_LEFT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_RETURN, VK_RIGHT, VK_TAB,
        VK_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetForegroundWindow, GetSystemMetrics, SetForegroundWindow, SM_CXSCREEN,
        SM_CYSCREEN,
    };
    use windows::core::PCWSTR;

    fn find_hwnd() -> Result<HWND, String> {
        unsafe {
            let title: Vec<u16> = "lopress\0".encode_utf16().collect();
            let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
            if hwnd.0 == 0 {
                Err("window not found".to_string())
            } else {
                Ok(hwnd)
            }
        }
    }

    pub(super) fn bring_to_front() -> Result<(), String> {
        unsafe {
            let hwnd = find_hwnd()?;
            if GetForegroundWindow() != hwnd {
                SetForegroundWindow(hwnd);
                std::thread::sleep(Duration::from_millis(80));
            }
            Ok(())
        }
    }

    pub(super) fn send_text(text: &str) -> Result<(), String> {
        let mut inputs: Vec<INPUT> = Vec::new();
        for ch in text.encode_utf16() {
            inputs.push(INPUT {
                r#type: INPUT_TYPE(1),
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_TYPE(1),
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    pub(super) fn send_keys(keys: &str) -> Result<(), String> {
        let parts: Vec<&str> = keys.split('+').collect();
        let n = parts.len();
        let (mod_parts, key_parts) = parts.split_at(n.saturating_sub(1));
        let key_str = key_parts.first().copied().unwrap_or("");

        let mut mods: Vec<VIRTUAL_KEY> = Vec::new();
        for m in mod_parts {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => mods.push(VK_LCONTROL),
                "shift" => mods.push(VK_LSHIFT),
                "alt" => mods.push(VK_LMENU),
                other => return Err(format!("unknown modifier: {other}")),
            }
        }

        let vk = parse_key(key_str)?;
        let mut inputs: Vec<INPUT> = Vec::new();

        for &m in &mods {
            inputs.push(make_key(m, false));
        }
        inputs.push(make_key(vk, false));
        inputs.push(make_key(vk, true));
        for &m in mods.iter().rev() {
            inputs.push(make_key(m, true));
        }

        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn make_key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) as u16 };
        INPUT {
            r#type: INPUT_TYPE(1),
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn parse_key(s: &str) -> Result<VIRTUAL_KEY, String> {
        match s.to_lowercase().as_str() {
            "enter" | "return" => Ok(VK_RETURN),
            "backspace" => Ok(VK_BACK),
            "delete" => Ok(VK_DELETE),
            "tab" => Ok(VK_TAB),
            "escape" | "esc" => Ok(VK_ESCAPE),
            "up" => Ok(VK_UP),
            "down" => Ok(VK_DOWN),
            "left" => Ok(VK_LEFT),
            "right" => Ok(VK_RIGHT),
            "home" => Ok(VK_HOME),
            "end" => Ok(VK_END),
            "f1" => Ok(VK_F1),
            "f2" => Ok(VK_F2),
            "f3" => Ok(VK_F3),
            "f4" => Ok(VK_F4),
            "f5" => Ok(VK_F5),
            "f6" => Ok(VK_F6),
            "f7" => Ok(VK_F7),
            "f8" => Ok(VK_F8),
            "f9" => Ok(VK_F9),
            "f10" => Ok(VK_F10),
            "f11" => Ok(VK_F11),
            "f12" => Ok(VK_F12),
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap_or('a').to_ascii_uppercase() as u16;
                Ok(VIRTUAL_KEY(ch))
            }
            other => Err(format!("unknown key: {other}")),
        }
    }

    pub(super) fn send_click(x: i32, y: i32) -> Result<(), String> {
        let hwnd = find_hwnd()?;
        unsafe {
            let mut pt = POINT { x, y };
            ClientToScreen(hwnd, &mut pt);

            let sw = GetSystemMetrics(SM_CXSCREEN);
            let sh = GetSystemMetrics(SM_CYSCREEN);
            if sw == 0 || sh == 0 {
                return Err("could not get screen dimensions".to_string());
            }
            let ax = i32::try_from(i64::from(pt.x) * 65535 / i64::from(sw)).unwrap_or(0);
            let ay = i32::try_from(i64::from(pt.y) * 65535 / i64::from(sh)).unwrap_or(0);

            let inputs = [
                INPUT {
                    r#type: INPUT_TYPE(0),
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            mouseData: 0,
                            dwFlags: MOUSE_EVENT_FLAGS(
                                MOUSEEVENTF_ABSOLUTE.0
                                    | MOUSEEVENTF_MOVE.0
                                    | MOUSEEVENTF_LEFTDOWN.0,
                            ),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_TYPE(0),
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            mouseData: 0,
                            dwFlags: MOUSE_EVENT_FLAGS(
                                MOUSEEVENTF_ABSOLUTE.0
                                    | MOUSEEVENTF_MOVE.0
                                    | MOUSEEVENTF_LEFTUP.0,
                            ),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(())
    }
}

// ── Non-Windows stub ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod platform {
    pub(super) fn bring_to_front() -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
    pub fn send_text(_text: &str) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
    pub fn send_keys(_keys: &str) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
    pub fn send_click(_x: i32, _y: i32) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
}
