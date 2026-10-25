//! 手势结果提示：原生 Win32 悬浮小窗（置顶、穿透、暖色圆角、白字）。
//! 固定显示在屏幕中央；每次提示开一个独立短命线程：
//! 建窗 → 绘制 → 停留 700ms → 300ms 渐隐 → 自毁，总时长 1 秒。

use std::sync::atomic::{AtomicUsize, Ordering};

/// 并发提示数上限：已有提示在显示时，新的直接覆盖计数（最后一个生效）
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

const HOLD_MS: i32 = 700; // 全不透明停留时长
const FADE_MS: i32 = 300; // 渐隐时长
const STEP_MS: i32 = 50; // 定时器步长
const FULL_ALPHA: u8 = 238;
const HIT_BG: u32 = 0x0021_78C8; // COLORREF = 0x00BBGGRR → 暖琥珀 (R200,G120,B33)
const MISS_BG: u32 = 0x005A_8C96; // 暖褐 (R150,G140,B90)

pub fn show_toast(text: String) {
    if ACTIVE.load(Ordering::Relaxed) > 4 {
        return; // 洪水保护
    }
    let _ = std::thread::Builder::new()
        .name("gesturecraft-toast".into())
        .spawn(move || unsafe { run_toast(text) });
}

unsafe fn run_toast(text: String) {
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::{
        CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, SetLayeredWindowAttributes,
        SetTimer, SetWindowPos, TranslateMessage, HWND_TOPMOST, MSG, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP,
    };

    ACTIVE.fetch_add(1, Ordering::Relaxed);

    let class_name: Vec<u16> = {
        let n = ACTIVE.load(Ordering::Relaxed);
        format!("GCToast{n}\0").encode_utf16().collect()
    };
    let hinstance = GetModuleHandleW(std::ptr::null());
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(toast_wndproc);
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name.as_ptr();
    wc.hbrBackground = std::ptr::null_mut();
    RegisterClassW(&wc); // 每次用新类名，基本不会重复

    // 尺寸按文字粗算：中文约 26px/字，英文数字约 13px
    let visual: usize = text
        .chars()
        .map(|c| if c.is_ascii() { 13 } else { 26 })
        .sum();
    let w = (visual as i32 + 56).clamp(140, 420);
    let h = 50;

    // 固定屏幕中央
    let (sw, sh) = screen_size();
    let px = (sw - w) / 2;
    let py = sh * 2 / 5 - h / 2;

    let hit = text.starts_with('✓');
    // 存背景色与文字，WM_PAINT 里取用
    TEXT.with(|t| *t.borrow_mut() = Some((text, if hit { HIT_BG } else { MISS_BG })));

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE,
        class_name.as_ptr(),
        std::ptr::null_mut(),
        WS_POPUP,
        px,
        py,
        w,
        h,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinstance,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        ACTIVE.fetch_sub(1, Ordering::Relaxed);
        return;
    }
    SetLayeredWindowAttributes(hwnd, 0, FULL_ALPHA, 0x02); // LWA_ALPHA
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    winapi::um::winuser::ShowWindow(hwnd, 8); // SW_SHOWNOACTIVATE
    SetTimer(hwnd, 1, STEP_MS as u32, None);

    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    winapi::um::winuser::DestroyWindow(hwnd);
    ACTIVE.fetch_sub(1, Ordering::Relaxed);
}

thread_local! {
    static TEXT: std::cell::RefCell<Option<(String, u32)>> = const { std::cell::RefCell::new(None) };
    static TICKS: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

unsafe extern "system" fn toast_wndproc(
    hwnd: *mut winapi::shared::windef::HWND__,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use winapi::um::wingdi::{
        CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, PS_NULL, RoundRect, SelectObject,
        SetBkMode, SetTextColor,
    };
    use winapi::um::winuser::{
        BeginPaint, DefWindowProcW, DrawTextW, EndPaint, GetClientRect, PostQuitMessage,
        SetLayeredWindowAttributes, WM_PAINT, WM_TIMER, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    };
    match msg {
        WM_PAINT => {
            let mut ps: winapi::um::winuser::PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = winapi::shared::windef::RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut rc);
            let (text, colorref) = TEXT.with(|t| t.borrow().clone()).unwrap_or_default();
            // 暖色圆角底
            let bg = CreateSolidBrush(colorref);
            let pen = CreatePen(PS_NULL as i32, 0, 0);
            let old_brush = SelectObject(hdc, bg as *mut winapi::ctypes::c_void);
            let old_pen = SelectObject(hdc, pen as *mut winapi::ctypes::c_void);
            RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 24, 24);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            DeleteObject(bg as *mut winapi::ctypes::c_void);
            DeleteObject(pen as *mut winapi::ctypes::c_void);
            // 白字（微软雅黑 UI，粗体）
            let font_name: Vec<u16> = "Microsoft YaHei UI\0".encode_utf16().collect();
            let font = CreateFontW(
                -28, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 0, 0,
                font_name.as_ptr() as *mut _,
            );
            let wide: Vec<u16> = text.encode_utf16().collect();
            let old_font = SelectObject(hdc, font as *mut winapi::ctypes::c_void);
            SetBkMode(hdc, 1); // TRANSPARENT
            SetTextColor(hdc, 0x00FFFFFF);
            DrawTextW(
                hdc,
                wide.as_ptr(),
                wide.len() as i32,
                &mut rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            SelectObject(hdc, old_font);
            DeleteObject(font as *mut winapi::ctypes::c_void);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            // 总时长 1 秒：前 700ms 全不透明，之后 300ms 线性渐隐，最后退出自毁
            TICKS.with(|t| {
                let ms = t.get() + STEP_MS;
                t.set(ms);
                if ms >= HOLD_MS + FADE_MS {
                    PostQuitMessage(0);
                } else if ms >= HOLD_MS {
                    let k = (ms - HOLD_MS) as f32 / FADE_MS as f32;
                    let a = ((1.0 - k) * FULL_ALPHA as f32) as u8;
                    SetLayeredWindowAttributes(hwnd, 0, a, 0x02); // LWA_ALPHA
                }
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn screen_size() -> (i32, i32) {
    #[cfg(windows)]
    unsafe {
        use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    }
    #[cfg(not(windows))]
    {
        (1920, 1080)
    }
}
