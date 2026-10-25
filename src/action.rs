use crate::config::ActionDef;
use std::sync::{Arc, Mutex};

pub fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    let mut l = logs.lock().unwrap();
    if l.len() > 200 {
        l.drain(0..100);
    }
    l.push(line);
}

pub fn execute(a: &ActionDef, logs: &Arc<Mutex<Vec<String>>>) {
    let result: Result<String, String> = match a {
        ActionDef::RunApp { path, args } => {
            let mut c = std::process::Command::new(path);
            for arg in args.split_whitespace() {
                c.arg(arg);
            }
            spawn_detached(c)
        }
        ActionDef::Command { cmd } => {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", cmd]);
            spawn_detached(c)
        }
        ActionDef::Url { url } => {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", "", url]);
            spawn_detached(c)
        }
        ActionDef::SendKeys { keys } => send_keys(keys).map(|_| "按键完成".into()),
        ActionDef::Hotkey { combo } => send_combo(combo),
        ActionDef::Window { op } => window_op(op),
        ActionDef::Volume { op } => volume_op(op),
        ActionDef::System { op } => system_op(op),
        ActionDef::Browser { op } => browser_op(op),
        ActionDef::Script { code } => run_script(code),
    };
    match result {
        Ok(msg) => push_log(logs, format!("[动作] {}: {}", a.kind_name(), msg)),
        Err(e) => push_log(logs, format!("[动作] {} 失败: {}", a.kind_name(), e)),
    }
}

fn spawn_detached(mut c: std::process::Command) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    match c.spawn() {
        Ok(_) => Ok("已启动".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn window_op(op: &str) -> Result<String, String> {
    #[cfg(windows)]
    unsafe {
        use winapi::ctypes::c_void;
        use winapi::shared::windef::RECT;
        use winapi::um::winuser::{
            GetForegroundWindow, GetWindowLongW, PostMessageW, SetWindowPos, ShowWindow,
            SystemParametersInfoW, GWL_EXSTYLE, HWND_TOP, HWND_NOTOPMOST, HWND_TOPMOST,
            SPI_GETWORKAREA, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE,
            WM_CLOSE, WS_EX_TOPMOST,
        };
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return Err("没有前台窗口".into());
        }
        match op {
            "minimize" => {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
            "maximize" => {
                ShowWindow(hwnd, SW_MAXIMIZE);
            }
            "restore" => {
                ShowWindow(hwnd, SW_RESTORE);
            }
            "close" => {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            "topmost" => {
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                let is_top = ex & WS_EX_TOPMOST != 0;
                SetWindowPos(
                    hwnd,
                    if is_top { HWND_NOTOPMOST } else { HWND_TOPMOST },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
            }
            "snap_left" | "snap_right" => {
                let mut wa: RECT = std::mem::zeroed();
                if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut wa as *mut RECT as *mut c_void, 0)
                    == 0
                {
                    return Err("获取工作区失败".into());
                }
                let w = (wa.right - wa.left) / 2;
                let h = wa.bottom - wa.top;
                let x = if op == "snap_left" { wa.left } else { wa.left + w };
                SetWindowPos(hwnd, HWND_TOP, x, wa.top, w, h, 0);
            }
            other => return Err(format!("未知窗口操作: {other}")),
        };
        Ok(format!("{} 完成", crate::config::window_op_label(op)))
    }
    #[cfg(not(windows))]
    {
        let _ = op;
        Err("仅支持 Windows".into())
    }
}

/// 解析 "ctrl+shift+t" 形式的组合键并注入：修饰键按下 → 点按主键 → 修饰键抬起
#[cfg(windows)]
fn send_combo(combo: &str) -> Result<String, String> {
    use winapi::ctypes::c_int;
    use winapi::um::winuser::SendInput;

    let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err("空组合键".into());
    }
    let vk_of = |name: &str| -> Option<u16> {
        use winapi::um::winuser::{
            VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
        };
        Some(match name.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => VK_CONTROL as u16,
            "SHIFT" => VK_SHIFT as u16,
            "ALT" => VK_MENU as u16,
            "WIN" | "META" => VK_LWIN as u16,
            other => {
                if let Some(vk) = vk_special(other) {
                    vk
                } else {
                    vk_for_char(other.chars().next()?)?
                }
            }
        })
    };
    let (mods, main) = parts.split_at(parts.len() - 1);
    let main_vk = vk_of(main[0]).ok_or_else(|| format!("未知按键: {}", main[0]))?;
    let mut mod_vks = Vec::new();
    for m in mods {
        mod_vks.push(vk_of(m).ok_or_else(|| format!("未知修饰键: {m}"))?);
    }

    let mut events: Vec<winapi::um::winuser::INPUT> = Vec::new();
    for &vk in &mod_vks {
        events.push(key_event(vk, false));
    }
    events.push(key_event(main_vk, false));
    events.push(key_event(main_vk, true));
    for &vk in mod_vks.iter().rev() {
        events.push(key_event(vk, true));
    }
    let sent = unsafe {
        SendInput(events.len() as u32, events.as_mut_ptr(), std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int)
    };
    if sent == events.len() as u32 {
        Ok(format!("组合键 {combo} 已发送"))
    } else {
        Err(format!("组合键被系统拦截 ({sent}/{})", events.len()))
    }
}

#[cfg(not(windows))]
fn send_combo(_combo: &str) -> Result<String, String> {
    Err("仅支持 Windows".into())
}

#[cfg(windows)]
fn tap_vk(vk: u16) -> Result<String, String> {
    use winapi::ctypes::c_int;
    use winapi::um::winuser::SendInput;
    let mut events = [key_event(vk, false), key_event(vk, true)];
    let sent = unsafe { SendInput(2u32, events.as_mut_ptr(), std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int) };
    if sent == 2 {
        Ok("完成".into())
    } else {
        Err("按键被系统拦截".into())
    }
}

fn volume_op(op: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        use winapi::um::winuser::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
        match op {
            "up" => tap_vk(VK_VOLUME_UP as u16),
            "down" => tap_vk(VK_VOLUME_DOWN as u16),
            "mute" => tap_vk(VK_VOLUME_MUTE as u16),
            other => Err(format!("未知音量操作: {other}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = op;
        Err("仅支持 Windows".into())
    }
}

fn browser_op(op: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        use winapi::um::winuser::{VK_BROWSER_BACK, VK_BROWSER_FORWARD, VK_BROWSER_REFRESH};
        match op {
            "back" => tap_vk(VK_BROWSER_BACK as u16),
            "forward" => tap_vk(VK_BROWSER_FORWARD as u16),
            "refresh" => tap_vk(VK_BROWSER_REFRESH as u16),
            other => Err(format!("未知浏览器操作: {other}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = op;
        Err("仅支持 Windows".into())
    }
}

fn system_op(op: &str) -> Result<String, String> {
    let combo = match op {
        "show_desktop" => "win+d",
        "task_view" => "win+tab",
        "clipboard_history" => "win+v",
        "lock" => "win+l",
        other => return Err(format!("未知系统操作: {other}")),
    };
    send_combo(combo)
}

fn vk_for_char(c: char) -> Option<u16> {
    match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as u16),
        'A'..='Z' => Some(c as u16),
        '0'..='9' => Some(c as u16),
        ' ' => Some(0x20),
        '\n' => Some(0x0D),
        '-' => Some(0xBD),
        '=' => Some(0xBB),
        '.' => Some(0xBE),
        ',' => Some(0xBC),
        '/' => Some(0xBF),
        ';' => Some(0xBA),
        '\'' => Some(0xDE),
        '[' => Some(0xDB),
        ']' => Some(0xDD),
        '\\' => Some(0xDC),
        '`' => Some(0xC0),
        _ => None,
    }
}

fn vk_special(name: &str) -> Option<u16> {
    Some(match name.to_ascii_uppercase().as_str() {
        "ENTER" => 0x0D,
        "TAB" => 0x09,
        "ESC" => 0x1B,
        "BACKSPACE" => 0x08,
        "DELETE" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,
        _ => return None,
    })
}

#[cfg(windows)]
fn key_event(vk: u16, up: bool) -> winapi::um::winuser::INPUT {
    use winapi::um::winuser::{INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP};
    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.type_ = INPUT_KEYBOARD;
    unsafe {
        *input.u.ki_mut() = KEYBDINPUT {
            wVk: vk,
            wScan: 0,
            dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
            time: 0,
            dwExtraInfo: 0,
        };
    }
    input
}

#[cfg(windows)]
fn send_keys(spec: &str) -> Result<String, String> {
    use winapi::um::winuser::{SendInput, VK_SHIFT};
    use winapi::ctypes::c_int;

    let mut events: Vec<winapi::um::winuser::INPUT> = Vec::new();
    let mut chars = spec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            // {NAME} 或 {SLEEP n}
            let mut token = String::new();
            for t in chars.by_ref() {
                if t == '}' {
                    break;
                }
                token.push(t);
            }
            let upper = token.to_ascii_uppercase();
            if let Some(rest) = upper.strip_prefix("SLEEP ") {
                let ms: u64 = rest.trim().parse().map_err(|_| format!("SLEEP 参数无效: {token}"))?;
                std::thread::sleep(std::time::Duration::from_millis(ms));
                continue;
            }
            if let Some(vk) = vk_special(&token) {
                events.push(key_event(vk, false));
                events.push(key_event(vk, true));
            } else {
                return Err(format!("未知特殊键: {{{token}}}"));
            }
        } else if let Some(vk) = vk_for_char(c) {
            let need_shift = c.is_ascii_uppercase() || "\\~!@#$%^&*()_+|{}:\"<>?".contains(c);
            if need_shift {
                events.push(key_event(VK_SHIFT as u16, false));
                events.push(key_event(vk, false));
                events.push(key_event(vk, true));
                events.push(key_event(VK_SHIFT as u16, true));
            } else {
                events.push(key_event(vk, false));
                events.push(key_event(vk, true));
            }
        } else {
            return Err(format!("不支持字符: {c}"));
        }
    }
    if events.is_empty() {
        return Ok("空按键序列".into());
    }
    let sent = unsafe { SendInput(events.len() as u32, events.as_mut_ptr(), std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int) };
    if sent == events.len() as u32 {
        Ok(format!("发送 {} 个按键事件", sent))
    } else {
        Err(format!("部分按键被系统拦截 ({sent}/{})", events.len()))
    }
}

#[cfg(not(windows))]
fn send_keys(_spec: &str) -> Result<String, String> {
    Err("仅支持 Windows".into())
}

fn run_script(code: &str) -> Result<String, String> {
    let mut engine = rhai::Engine::new();
    engine.on_print(|s| println!("[脚本] {s}"));
    match engine.eval::<rhai::Dynamic>(code) {
        Ok(v) => Ok(if v.is_unit() {
            "脚本执行完成".into()
        } else {
            format!("脚本返回: {v}")
        }),
        Err(e) => Err(format!("脚本错误: {e}")),
    }
}
