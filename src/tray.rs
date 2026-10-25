//! 系统托盘常驻：Shell_NotifyIconW + 隐藏窗口。
//! 注意：必须用普通隐藏窗口（不能是 HWND_MESSAGE 的 message-only 窗口），
//! 否则收不到托盘回调、TrackPopupMenu 也无法正常工作。
//! 菜单命令经 mpsc 发给 UI 线程（显示主窗口 / 暂停恢复 / 退出）。

use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayCmd {
    Show,
    TogglePause,
    Quit,
}

static TX: std::sync::Mutex<Option<Sender<TrayCmd>>> = std::sync::Mutex::new(None);
static TRAY_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

const WM_APP_TRAY: u32 = 0x8000; // WM_APP
const ID_TRAY: u32 = 1;

pub fn start_tray(tx: Sender<TrayCmd>) {
    match TX.lock() {
        Ok(mut guard) => *guard = Some(tx),
        Err(_) => return,
    }
    let _ = std::thread::Builder::new()
        .name("gesturecraft-tray".into())
        .spawn(|| unsafe { run() });
}

/// UI 线程退出前调用：移除托盘图标并结束托盘线程
pub fn shutdown() {
    let hwnd = TRAY_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            winapi::um::winuser::PostMessageW(
                hwnd as *mut _,
                0x0010, // WM_CLOSE
                0,
                0,
            );
        }
    }
}

unsafe fn run() {
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::{
        CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
        MSG, WNDCLASSW,
    };

    let class_name: Vec<u16> = "GestureCraftTray\0".encode_utf16().collect();
    let hinstance = GetModuleHandleW(std::ptr::null());

    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(tray_wndproc);
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name.as_ptr();
    RegisterClassW(&wc);

    // 普通隐藏窗口（不带 WS_VISIBLE，从不 ShowWindow）：
    // message-only 窗口收不到托盘回调消息，右键菜单会完全没反应
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        class_name.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinstance,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        return;
    }
    TRAY_HWND.store(hwnd as isize, std::sync::atomic::Ordering::Relaxed);
    add_tray_icon(hwnd);

    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    remove_tray_icon(hwnd);
}

unsafe fn add_tray_icon(hwnd: *mut winapi::shared::windef::HWND__) {
    use winapi::um::shellapi::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW};
    use winapi::um::winuser::{IDI_APPLICATION, LoadIconW};
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.hWnd = hwnd;
    nid.uID = ID_TRAY;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
    let tip: Vec<u16> = "GestureCraft 鼠标手势\0".encode_utf16().collect();
    // szTip 是 [u16; 128]
    for (i, &c) in tip.iter().take(127).enumerate() {
        nid.szTip[i] = c;
    }
    Shell_NotifyIconW(NIM_ADD, &mut nid);
}

unsafe fn remove_tray_icon(hwnd: *mut winapi::shared::windef::HWND__) {
    use winapi::um::shellapi::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.hWnd = hwnd;
    nid.uID = ID_TRAY;
    Shell_NotifyIconW(NIM_DELETE, &mut nid);
}

fn send(cmd: TrayCmd) {
    if let Ok(guard) = TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(cmd);
        }
    }
}

unsafe extern "system" fn tray_wndproc(
    hwnd: *mut winapi::shared::windef::HWND__,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use winapi::um::winuser::{
        CreatePopupMenu, DefWindowProcW, DestroyMenu, GetCursorPos, PostQuitMessage,
        SetForegroundWindow, TrackPopupMenu, AppendMenuW, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
    };
    match msg {
        WM_APP_TRAY => {
            match lparam as u32 {
                WM_LBUTTONDBLCLK => send(TrayCmd::Show),
                WM_RBUTTONUP => {
                    let hmenu = CreatePopupMenu();
                    if !hmenu.is_null() {
                        let title: Vec<u16> = "显示主窗口\0".encode_utf16().collect();
                        let pause: Vec<u16> = "暂停 / 恢复监听\0".encode_utf16().collect();
                        let quit: Vec<u16> = "关闭 GestureCraft\0".encode_utf16().collect();
                        AppendMenuW(hmenu, 0, 1, title.as_ptr());
                        AppendMenuW(hmenu, 0, 2, pause.as_ptr());
                        AppendMenuW(hmenu, 0x800, 0, std::ptr::null()); // MF_SEPARATOR
                        AppendMenuW(hmenu, 0, 3, quit.as_ptr());
                        let mut pt = winapi::shared::windef::POINT { x: 0, y: 0 };
                        GetCursorPos(&mut pt);
                        SetForegroundWindow(hwnd); // 让菜单可消失
                        let id = TrackPopupMenu(
                            hmenu,
                            TPM_RETURNCMD | TPM_RIGHTBUTTON,
                            pt.x,
                            pt.y,
                            0,
                            hwnd,
                            std::ptr::null(),
                        );
                        DestroyMenu(hmenu);
                        match id {
                            1 => send(TrayCmd::Show),
                            2 => send(TrayCmd::TogglePause),
                            3 => send(TrayCmd::Quit),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            winapi::um::winuser::DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
