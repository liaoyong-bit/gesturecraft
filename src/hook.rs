//! Windows 低级鼠标钩子线程。
//! 钩子回调内维护录制状态：触发键按下 → 位移超过阈值进入手势模式（吞掉事件防止
//! 弹出上下文菜单）→ 松开时把整条轨迹交给引擎线程执行匹配。

use crate::config::MouseButton;
use crate::engine::{self, Ctx};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

struct HookState {
    start: Mutex<Option<(i32, i32)>>,
    points: Mutex<Vec<(i32, i32)>>,
    gesturing: AtomicBool,
}

static STATE: OnceLock<HookState> = OnceLock::new();
static CTX: OnceLock<Ctx> = OnceLock::new();

pub fn start_hook_thread(ctx: Ctx) {
    std::thread::Builder::new()
        .name("gesturecraft-hook".into())
        .spawn(move || unsafe { run_hook(ctx) })
        .expect("无法启动钩子线程");
}

unsafe fn run_hook(ctx: Ctx) {
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::{
        GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG, WH_MOUSE_LL,
    };

    let _ = STATE.set(HookState {
        start: Mutex::new(None),
        points: Mutex::new(Vec::new()),
        gesturing: AtomicBool::new(false),
    });
    let _ = CTX.set(ctx);

    let hmod = GetModuleHandleW(std::ptr::null());
    let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hmod, 0);
    if hook.is_null() {
        eprintln!("安装鼠标钩子失败");
        return;
    }

    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) != 0 {}
    UnhookWindowsHookEx(hook);
}

unsafe fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use winapi::um::winuser::{
        CallNextHookEx, MSLLHOOKSTRUCT, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
        WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    let ret_pass = |code: i32, wparam: usize, lparam: isize| {
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    };

    if code < 0 {
        return ret_pass(code, wparam, lparam);
    }

    let (Some(state), Some(ctx)) = (STATE.get(), CTX.get()) else {
        return ret_pass(code, wparam, lparam);
    };

    let info = &*(lparam as *const MSLLHOOKSTRUCT);
    let msg = wparam as u32;
    let (x, y) = (info.pt.x, info.pt.y);

    // 触发键与配置是否一致
    let trigger_matches = |down_msg: u32| -> bool {
        let want = ctx.config.read().unwrap().trigger;
        match want {
            MouseButton::Right => down_msg == WM_RBUTTONDOWN,
            MouseButton::Middle => down_msg == WM_MBUTTONDOWN,
        }
    };

    match msg {
        m if m == WM_RBUTTONDOWN || m == WM_MBUTTONDOWN => {
            if trigger_matches(m) {
                *state.start.lock().unwrap() = Some((x, y));
                state.points.lock().unwrap().clear();
                state.gesturing.store(false, Ordering::SeqCst);
            }
        }
        WM_MOUSEMOVE => {
            if state.gesturing.load(Ordering::SeqCst) {
                state.points.lock().unwrap().push((x, y));
                return 1; // 手势中吞掉移动事件
            }
            if let Some((sx, sy)) = *state.start.lock().unwrap() {
                let threshold = ctx.config.read().unwrap().threshold_px as f32;
                let d = ((x - sx) as f32).hypot((y - sy) as f32);
                if d >= threshold {
                    state.gesturing.store(true, Ordering::SeqCst);
                    let mut pts = state.points.lock().unwrap();
                    pts.push((sx, sy));
                    pts.push((x, y));
                }
            }
        }
        m if m == WM_RBUTTONUP || m == WM_MBUTTONUP => {
            let was_gesturing = state.gesturing.swap(false, Ordering::SeqCst);
            let had_start = state.start.lock().unwrap().take().is_some();
            if was_gesturing {
                let pts = state.points.lock().unwrap().clone();
                let ctx2 = ctx as *const Ctx;
                let pts2 = pts;
                std::thread::spawn(move || {
                    engine::handle_path(&*ctx2, pts2);
                });
                return 1; // 吞掉抬起，防止弹菜单
            }
            if !had_start {
                // 与触发无关的按键，正常放行
            }
        }
        _ => {}
    }

    ret_pass(code, wparam, lparam)
}
