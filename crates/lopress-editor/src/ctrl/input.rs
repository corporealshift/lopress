#![allow(unsafe_code)]
// Debug-only input injection: the Win32 SendInput / scan-code / screen-
// coordinate math inherently needs casts and integer division that the
// workspace lints forbid in production code.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::integer_division
)]

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

// All input injection goes through `SendInput`, which feeds the real Windows
// input pipeline. This is the only mechanism that updates the kernel keyboard
// state that winit reads (via `GetKeyboardState` / `ToUnicodeEx`) to compute
// key text and modifier state. `PostMessage(WM_KEYDOWN/WM_CHAR)` was tried
// previously: it puts messages in the window's queue but never updates that
// kernel state, so winit dropped orphan `WM_CHAR`s and never saw Ctrl/Shift/
// Alt held — typing and every modified chord silently no-opped.
//
// `SendInput` delivers to whichever window has keyboard focus, so every
// keyboard call first brings lopress to the foreground (and refuses to inject
// if it cannot — otherwise the keystrokes would land in another app).

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
    use std::time::Duration;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD,
        INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_BACK,
        VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4,
        VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT,
        VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
    };
    use windows::Win32::System::Threading::AttachThreadInput;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, FindWindowW, GetForegroundWindow, GetSystemMetrics,
        GetWindowThreadProcessId, SetForegroundWindow, SetWindowPos, SM_CXSCREEN, SM_CYSCREEN,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

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

    /// What a single key token resolves to: which virtual key, whether it is an
    /// extended key (nav cluster), and whether the active layout needs Shift to
    /// produce the requested character.
    #[derive(Debug, Clone, Copy)]
    struct KeySpec {
        vk: VIRTUAL_KEY,
        extended: bool,
        shift: bool,
    }

    /// Bring the lopress window to the foreground so `SendInput` keystrokes land
    /// in it. Returns an error (rather than injecting blindly) if focus could
    /// not be taken — keystrokes injected into another app would be harmful.
    fn ensure_foreground(hwnd: HWND) -> Result<(), String> {
        unsafe {
            if GetForegroundWindow().0 == hwnd.0 {
                return Ok(());
            }
            let _ = SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

            // SetForegroundWindow from a background process is restricted. The
            // standard workaround: attach our window thread's input queue to
            // the current foreground thread's, which lifts the restriction for
            // the duration of the attachment.
            let target_thread = GetWindowThreadProcessId(hwnd, None);
            let fg = GetForegroundWindow();
            let fg_thread = if fg.0 == 0 {
                0
            } else {
                GetWindowThreadProcessId(fg, None)
            };
            if fg_thread != 0 && fg_thread != target_thread {
                let _ = AttachThreadInput(fg_thread, target_thread, BOOL(1));
                let _ = SetForegroundWindow(hwnd);
                let _ = BringWindowToTop(hwnd);
                let _ = AttachThreadInput(fg_thread, target_thread, BOOL(0));
            } else {
                let _ = SetForegroundWindow(hwnd);
            }

            // Let the activation settle before the input stream is fed.
            std::thread::sleep(Duration::from_millis(40));

            if GetForegroundWindow().0 == hwnd.0 {
                Ok(())
            } else {
                Err("could not bring the lopress window to the foreground; \
                     click it once and retry"
                    .to_string())
            }
        }
    }

    /// One `INPUT` event for a virtual key (down or up).
    fn vk_event(vk: VIRTUAL_KEY, extended: bool, up: bool) -> INPUT {
        let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as u16;
        let mut flags = 0u32;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY.0;
        }
        if up {
            flags |= KEYEVENTF_KEYUP.0;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: KEYBD_EVENT_FLAGS(flags),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// One `INPUT` event for a UTF-16 code unit typed as literal text.
    fn unicode_event(unit: u16, up: bool) -> INPUT {
        let mut flags = KEYEVENTF_UNICODE.0;
        if up {
            flags |= KEYEVENTF_KEYUP.0;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYBD_EVENT_FLAGS(flags),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) {
        if inputs.is_empty() {
            return;
        }
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
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
                    r#type: INPUT_MOUSE,
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
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            mouseData: 0,
                            dwFlags: MOUSE_EVENT_FLAGS(
                                MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_LEFTUP.0,
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
        ensure_foreground(hwnd)?;

        let mut inputs: Vec<INPUT> = Vec::new();
        for unit in text.encode_utf16() {
            inputs.push(unicode_event(unit, false));
            inputs.push(unicode_event(unit, true));
        }
        send(&inputs);
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

        let spec = parse_key(key_str)?;
        // A character whose layout position needs Shift (e.g. `?`) gets Shift
        // added automatically unless the caller already asked for it.
        if spec.shift && !mods.contains(&VK_LSHIFT) {
            mods.push(VK_LSHIFT);
        }

        ensure_foreground(hwnd)?;

        // Whole chord in one SendInput batch so no real input interleaves:
        // modifier-downs, key down, key up, modifier-ups in reverse.
        let mut inputs: Vec<INPUT> = Vec::new();
        for &m in &mods {
            inputs.push(vk_event(m, false, false));
        }
        inputs.push(vk_event(spec.vk, spec.extended, false));
        inputs.push(vk_event(spec.vk, spec.extended, true));
        for &m in mods.iter().rev() {
            inputs.push(vk_event(m, false, true));
        }
        send(&inputs);
        Ok(())
    }

    fn parse_key(s: &str) -> Result<KeySpec, String> {
        // Named keys. `extended` marks the nav-cluster keys, which winit
        // distinguishes from their numpad twins via the extended-key flag.
        let named = |vk, extended| {
            Ok(KeySpec {
                vk,
                extended,
                shift: false,
            })
        };
        match s.to_lowercase().as_str() {
            "enter" | "return" => named(VK_RETURN, false),
            "backspace" => named(VK_BACK, false),
            "delete" | "del" => named(VK_DELETE, true),
            "tab" => named(VK_TAB, false),
            "escape" | "esc" => named(VK_ESCAPE, false),
            "space" => named(VK_SPACE, false),
            "up" => named(VK_UP, true),
            "down" => named(VK_DOWN, true),
            "left" => named(VK_LEFT, true),
            "right" => named(VK_RIGHT, true),
            "home" => named(VK_HOME, true),
            "end" => named(VK_END, true),
            "pageup" | "pgup" => named(VK_PRIOR, true),
            "pagedown" | "pgdn" => named(VK_NEXT, true),
            "f1" => named(VK_F1, false),
            "f2" => named(VK_F2, false),
            "f3" => named(VK_F3, false),
            "f4" => named(VK_F4, false),
            "f5" => named(VK_F5, false),
            "f6" => named(VK_F6, false),
            "f7" => named(VK_F7, false),
            "f8" => named(VK_F8, false),
            "f9" => named(VK_F9, false),
            "f10" => named(VK_F10, false),
            "f11" => named(VK_F11, false),
            "f12" => named(VK_F12, false),
            s if s.chars().count() == 1 => {
                // Resolve the character against the active keyboard layout, so
                // punctuation (`/`, `?`, …) maps to its real virtual key
                // instead of the raw code point.
                let ch = s.chars().next().unwrap_or('a');
                let scan = unsafe { VkKeyScanW(ch as u16) };
                if scan == -1 {
                    return Err(format!("no virtual key for character: {ch}"));
                }
                let vk = u16::try_from(scan & 0x00FF).unwrap_or(0);
                let shift = (scan & 0x0100) != 0;
                Ok(KeySpec {
                    vk: VIRTUAL_KEY(vk),
                    extended: false,
                    shift,
                })
            }
            other => Err(format!("unknown key: {other}")),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{parse_key, VK_NEXT, VK_PRIOR, VK_RETURN};

        #[test]
        fn parse_key_handles_page_navigation() {
            assert!(
                matches!(parse_key("pageup"), Ok(k) if k.vk == VK_PRIOR && k.extended),
                "pageup should map to the extended VK_PRIOR",
            );
            assert!(
                matches!(parse_key("pagedown"), Ok(k) if k.vk == VK_NEXT && k.extended),
                "pagedown should map to the extended VK_NEXT",
            );
        }

        #[test]
        fn parse_key_named_keys() {
            assert!(matches!(parse_key("enter"), Ok(k) if k.vk == VK_RETURN));
            assert!(matches!(parse_key("home"), Ok(k) if k.extended));
        }

        #[test]
        fn parse_key_single_char_uses_layout_vk() {
            // The old code mapped any one-char token to VIRTUAL_KEY(uppercased
            // char): `/` became 0x2F, which is not a real virtual-key code.
            // The layout lookup must produce something other than that.
            assert!(
                matches!(parse_key("/"), Ok(k) if k.vk.0 != u16::from(b'/')),
                "`/` must resolve to a real virtual key, not its code point",
            );
        }

        #[test]
        fn parse_key_rejects_unknown() {
            assert!(parse_key("nope").is_err());
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
