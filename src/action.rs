use crate::config::ActionDef;
use std::sync::{Arc, Mutex};

pub fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    let mut l = logs.lock().unwrap();
    if l.len() > 200 {
        l.drain(0..100);
    }
    l.push(line);
}

/// 前台窗口上下文：(进程名小写无 .exe，是否桌面/公共区域)。
/// under_cursor=false 按活动窗口（键盘焦点）；true 按鼠标所指窗口（手势终点落点）。
/// 供作用域解析与"粘贴并转到"的浏览器判定使用。
pub fn foreground_context(under_cursor: bool) -> (Option<String>, bool) {
    #[cfg(windows)]
    unsafe {
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::shared::windef::POINT;
        use winapi::um::winbase::QueryFullProcessImageNameW;
        use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
        use winapi::um::winuser::{
            GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
            WindowFromPoint,
        };
        let hwnd = if under_cursor {
            let mut pt = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return (None, false);
            }
            WindowFromPoint(pt)
        } else {
            GetForegroundWindow()
        };
        if hwnd.is_null() {
            return (None, false);
        }
        // 桌面/公共区域判定：Progman / WorkerW
        let mut cls = [0u16; 64];
        let n = GetClassNameW(hwnd, cls.as_mut_ptr(), 64);
        let cls_name = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
        let is_desktop = cls_name == "Progman" || cls_name == "WorkerW";

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let proc_name = if pid != 0 {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                None
            } else {
                let mut buf = [0u16; 512];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
                CloseHandle(h);
                if ok != 0 {
                    let full = String::from_utf16_lossy(&buf[..len as usize]);
                    full.rsplit(['\\', '/'])
                        .next()
                        .map(|s| s.trim_end_matches(".exe").trim_end_matches(".EXE").to_ascii_lowercase())
                } else {
                    None
                }
            }
        } else {
            None
        };
        (proc_name, is_desktop)
    }
    #[cfg(not(windows))]
    {
        let _ = under_cursor;
        (None, false)
    }
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
        ActionDef::PasteAndGo => paste_and_go(),
        ActionDef::Script { code } => run_script(code),
    };
    match result {
        Ok(msg) => push_log(logs, format!("[动作] {}: {}", a.kind_name(), msg)),
        Err(e) => push_log(logs, format!("[动作] {} 失败: {}", a.kind_name(), e)),
    }
}

/// 常见浏览器进程名（小写无 .exe）
const BROWSERS: [&str; 12] = [
    "msedge", "chrome", "chromium", "firefox", "opera", "brave", "vivaldi", "360se", "360chrome",
    "qqbrowser", "sogouexplorer", "maxthon",
];

/// 粘贴并转到：浏览器 → 定位地址栏(Ctrl+L) → 粘贴(Ctrl+V) → 回车；
/// 非浏览器 → 普通 Ctrl+V。
fn paste_and_go() -> Result<String, String> {
    let (proc_name, _) = foreground_context(false);
    let is_browser = proc_name.as_deref().map_or(false, |p| BROWSERS.contains(&p));
    if is_browser {
        send_combo("ctrl+l")?;
        std::thread::sleep(std::time::Duration::from_millis(150));
        send_combo("ctrl+v")?;
        std::thread::sleep(std::time::Duration::from_millis(150));
        tap_vk(0x0D)?; // Enter
        Ok("地址栏粘贴并跳转".into())
    } else {
        send_combo("ctrl+v")
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

/// 在当前光标位置注入一次中键单击（浏览器：打开悬停链接到新标签页）
#[cfg(windows)]
fn middle_click_at_cursor() -> Result<String, String> {
    use winapi::ctypes::c_int;
    use winapi::um::winuser::{
        SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    };
    let mk = |flag: u32| -> INPUT {
        let mut input: INPUT = unsafe { std::mem::zeroed() };
        input.type_ = INPUT_MOUSE;
        unsafe {
            input.u.mi_mut().dwFlags = flag;
        }
        input
    };
    let mut events = [mk(MOUSEEVENTF_MIDDLEDOWN), mk(MOUSEEVENTF_MIDDLEUP)];
    let sent = unsafe {
        SendInput(2, events.as_mut_ptr(), std::mem::size_of::<INPUT>() as c_int)
    };
    if sent == 2 {
        Ok("中键单击完成".into())
    } else {
        Err("中键单击被系统拦截".into())
    }
}

/// 在当前光标位置注入一次 Shift+左键单击（浏览器：打开悬停链接到新窗口）
#[cfg(windows)]
fn shift_left_click() -> Result<String, String> {
    use winapi::ctypes::c_int;
    use winapi::um::winuser::{
        SendInput, INPUT, INPUT_MOUSE, INPUT_KEYBOARD, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        VK_SHIFT,
    };
    let mouse = |flag: u32| -> INPUT {
        let mut input: INPUT = unsafe { std::mem::zeroed() };
        input.type_ = INPUT_MOUSE;
        unsafe {
            input.u.mi_mut().dwFlags = flag;
        }
        input
    };
    let mut events: Vec<INPUT> = Vec::new();
    let mut k: INPUT = unsafe { std::mem::zeroed() };
    k.type_ = INPUT_KEYBOARD;
    unsafe {
        *k.u.ki_mut() = winapi::um::winuser::KEYBDINPUT {
            wVk: VK_SHIFT as u16,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
    }
    events.push(k);
    events.push(mouse(MOUSEEVENTF_LEFTDOWN));
    events.push(mouse(MOUSEEVENTF_LEFTUP));
    let mut ku: INPUT = unsafe { std::mem::zeroed() };
    ku.type_ = INPUT_KEYBOARD;
    unsafe {
        *ku.u.ki_mut() = winapi::um::winuser::KEYBDINPUT {
            wVk: VK_SHIFT as u16,
            wScan: 0,
            dwFlags: winapi::um::winuser::KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };
    }
    events.push(ku);
    let sent = unsafe {
        SendInput(events.len() as u32, events.as_mut_ptr(), std::mem::size_of::<INPUT>() as c_int)
    };
    if sent == events.len() as u32 {
        Ok("Shift+单击完成".into())
    } else {
        Err("Shift+单击被系统拦截".into())
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
        use winapi::um::winuser::{VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT};
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
        SendInput(
            events.len() as u32,
            events.as_mut_ptr(),
            std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int,
        )
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
    let sent = unsafe {
        SendInput(2u32, events.as_mut_ptr(), std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int)
    };
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
            "stop" => tap_vk(0x1B), // ESC
            "open_link_new_tab" => middle_click_at_cursor(),
            "open_link_new_window" => shift_left_click(),
            // Edge/Chrome 未提供这两项的系统级快捷键，全局键鼠模拟无法实现
            "close_tabs_right" | "close_other_tabs" => {
                Err("浏览器未提供系统级快捷键，该动作暂不支持（可在编辑器中改绑其他动作）".into())
            }
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
        "lock" => return lock_workstation(),
        other => return Err(format!("未知系统操作: {other}")),
    };
    send_combo(combo)
}

/// 锁屏：直接调用系统 API（模拟 Win+L 会被部分环境拦截）
#[cfg(windows)]
fn lock_workstation() -> Result<String, String> {
    let ok = unsafe { winapi::um::winuser::LockWorkStation() };
    if ok != 0 {
        Ok("已锁屏".into())
    } else {
        Err("锁屏调用失败".into())
    }
}
#[cfg(not(windows))]
fn lock_workstation() -> Result<String, String> {
    Err("仅支持 Windows".into())
}

/// 常用程序清单（进程关键字, 显示名），供"添加程序"复选列表打底
pub fn common_programs() -> Vec<(String, String)> {
    vec![
        ("msedge".into(), "Microsoft Edge".into()),
        ("chrome".into(), "Google Chrome".into()),
        ("firefox".into(), "Firefox".into()),
        ("explorer".into(), "资源管理器".into()),
        ("notepad".into(), "记事本".into()),
        ("weixin".into(), "微信".into()),
        ("qq".into(), "QQ".into()),
        ("code".into(), "VS Code".into()),
        ("cmd".into(), "命令提示符".into()),
        ("powershell".into(), "PowerShell".into()),
        ("potplayer".into(), "PotPlayer".into()),
        ("everything".into(), "Everything".into()),
        ("winword".into(), "Word".into()),
        ("excel".into(), "Excel".into()),
        ("powerpnt".into(), "PowerPoint".into()),
    ]
}

/// 当前运行中、带可见窗口的程序进程名（小写无 .exe），按窗口数排序去重
pub fn running_window_processes() -> Vec<String> {
    #[cfg(windows)]
    unsafe {
        use std::collections::BTreeMap;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::um::winbase::QueryFullProcessImageNameW;
        use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
        use winapi::um::winuser::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        };

        // (pid, 窗口标题非空?) 列表 → 收集后再查进程名（回调里不方便携带复杂状态）
        let mut windows: Vec<(u32, bool)> = Vec::new();
        unsafe extern "system" fn cb(hwnd: *mut winapi::shared::windef::HWND__, lparam: isize) -> i32 {
            let out = &mut *(lparam as *mut Vec<(u32, bool)>);
            if IsWindowVisible(hwnd) != 0 {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid != 0 {
                    let mut title = [0u16; 64];
                    let n = GetWindowTextW(hwnd, title.as_mut_ptr(), 64);
                    out.push((pid, n > 0));
                }
            }
            1
        }
        EnumWindows(Some(cb), &mut windows as *mut _ as isize);

        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for (pid, has_title) in &windows {
            let name = (|| {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, *pid);
                if h.is_null() {
                    return None;
                }
                let mut buf = [0u16; 512];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
                CloseHandle(h);
                if ok == 0 {
                    return None;
                }
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit(['\\', '/'])
                    .next()
                    .map(|s| s.trim_end_matches(".exe").trim_end_matches(".EXE").to_ascii_lowercase())
            })();
            if let Some(n) = name {
                // 有标题的主窗口权重更高
                let e = counts.entry(n).or_default();
                *e += if *has_title { 2 } else { 1 };
            }
        }
        let mut out: Vec<(String, usize)> = counts.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out.into_iter().map(|(n, _)| n).collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// 添加程序候选：运行中进程（按窗口数排序）在前，常用清单打底，按进程名去重
pub fn candidate_programs() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for p in running_window_processes() {
        let display = common_programs()
            .iter()
            .find(|(k, _)| *k == p)
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|| {
                let mut c = p.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => p.clone(),
                }
            });
        seen.push(p.clone());
        out.push((p, display));
    }
    for (k, d) in common_programs() {
        if !seen.contains(&k) {
            out.push((k, d));
        }
    }
    out
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
    use winapi::ctypes::c_int;
    use winapi::um::winuser::{SendInput, VK_SHIFT};

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
    let sent = unsafe {
        SendInput(
            events.len() as u32,
            events.as_mut_ptr(),
            std::mem::size_of::<winapi::um::winuser::INPUT>() as c_int,
        )
    };
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
