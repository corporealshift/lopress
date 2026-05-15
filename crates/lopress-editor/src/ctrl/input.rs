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

// Clicks: SetWindowPos(HWND_TOP, NOACTIVATE) brings lopress to the top of the
// Z-order without stealing keyboard focus, then SendInput delivers the click to
// the now-topmost window at the target screen coords — regardless of which
// process is foreground. The natural click activation then makes lopress the
// foreground window, so subsequent keyboard SendInput works correctly.
//
// Keyboard/text: PostMessage(WM_KEYDOWN / WM_CHAR) delivers directly to the
// HWND's message queue without requiring focus at call time. GetKeyState
// updates per-message as the queue drains, so modifier state is correct when
// the main key message is processed.

pub(crate) fn handle_input(body: &str) -> Result<(), String> {
    let cmd: InputCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    match cmd {
        InputCmd::Text { text } => platform::send_text(&text),
        InputCmd::Keys { keys } => platform::send_keys(&keys),
    }
}

pub(crate) fn handle_click(body: &str) -> Result<(), String> {
    let cmd: ClickCmd = serde_json::from_str(body).map_err(|e| e.to_string())?;
    platform::send_click(cmd.x, cmd.y)
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_TYPE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS,
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
        MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10,
        VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME,
        VK_LEFT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetSystemMetrics, PostMessageW, SetWindowPos, SM_CXSCREEN, SM_CYSCREEN,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_CHAR, WM_KEYDOWN, WM_KEYUP,
    };
    use windows::core::PCWSTR;

    // HWND(0) = HWND_TOP (place at top of Z-order, below TOPMOST windows)
    const HWND_TOP: HWND = HWND(0);

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

    // lParam for WM_KEYDOWN: bits 16-23 = scan code, bit 0 = repeat count 1
    fn lparam_keydown(scan: u16) -> LPARAM {
        LPARAM(((scan as u32) << 16 | 1) as isize)
    }

    // lParam for WM_KEYUP: scan code + bits 30-31 set (was-down, transition-to-up)
    fn lparam_keyup(scan: u16) -> LPARAM {
        LPARAM((0xC000_0001u32 | ((scan as u32) << 16)) as isize)
    }

    pub(super) fn send_click(x: i32, y: i32) -> Result<(), String> {
        let hwnd = find_hwnd()?;
        unsafe {
            // Bring to Z-order top so SendInput's absolute coords hit lopress,
            // not whatever window is currently overlapping it.
            let _ = SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

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

    pub(super) fn send_text(text: &str) -> Result<(), String> {
        let hwnd = find_hwnd()?;
        unsafe {
            // WM_CHAR delivers one UTF-16 code unit per message.
            // GetKeyState updates as the queue drains, so this works when the
            // window has focus from a prior click.
            for ch in text.encode_utf16() {
                let _ = PostMessageW(hwnd, WM_CHAR, WPARAM(ch as usize), LPARAM(1));
            }
        }
        Ok(())
    }

    pub(super) fn send_keys(keys: &str) -> Result<(), String> {
        let hwnd = find_hwnd()?;

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

        unsafe {
            // PostMessage delivers the modifier key-down events first. The target
            // thread's GetKeyState reflects these as each message is dequeued,
            // so the modifier is seen as active when the main key is processed.
            for &m in &mods {
                let scan = MapVirtualKeyW(u32::from(m.0), MAPVK_VK_TO_VSC) as u16;
                let _ =
                    PostMessageW(hwnd, WM_KEYDOWN, WPARAM(m.0 as usize), lparam_keydown(scan));
            }
            let scan = MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) as u16;
            let _ = PostMessageW(hwnd, WM_KEYDOWN, WPARAM(vk.0 as usize), lparam_keydown(scan));
            let _ = PostMessageW(hwnd, WM_KEYUP, WPARAM(vk.0 as usize), lparam_keyup(scan));
            for &m in mods.iter().rev() {
                let scan = MapVirtualKeyW(u32::from(m.0), MAPVK_VK_TO_VSC) as u16;
                let _ =
                    PostMessageW(hwnd, WM_KEYUP, WPARAM(m.0 as usize), lparam_keyup(scan));
            }
        }
        Ok(())
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
}

// ── Non-Windows stub ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod platform {
    pub(super) fn send_text(_text: &str) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
    pub(super) fn send_keys(_keys: &str) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
    pub(super) fn send_click(_x: i32, _y: i32) -> Result<(), String> {
        Err("input injection only supported on Windows".to_string())
    }
}
