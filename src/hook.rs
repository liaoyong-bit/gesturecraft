//! Windows 低级鼠标钩子线程。
//! 触发键按下 → 位移超过阈值进入手势模式（光标照常移动，仅松开时吞掉 UP
//! 防止弹出上下文菜单）→ 完整轨迹交给引擎线程匹配，并请求 UI 重绘显示日志。
//! 钩子回调内绝不等待长锁：threshold 在触发键按下时一次性缓存到原子变量，
//! 移动事件路径上只碰无争用的 Mutex/Atomic，避免 Windows 因回调超时移除钩子。

use crate::config::MouseButton;
use crate::engine::{self, Ctx};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

struct HookState {
    start: Mutex<Option<(i32, i32)>>,
    points: Mutex<Vec<(i32, i32)>>,
    gesturing: AtomicBool,
    /// 触发键按下时缓存的起手势阈值（像素），移动热路径不再读 config
    threshold: AtomicU32,
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
        threshold: AtomicU32::new(30),
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

unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use winapi::um::winuser::{
        CallNextHookEx, MSLLHOOKSTRUCT, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
        WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    if code < 0 {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    }

    let (Some(state), Some(ctx)) = (STATE.get(), CTX.get()) else {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    };

    let info = &*(lparam as *const MSLLHOOKSTRUCT);
    let msg = wparam as u32;
    let (x, y) = (info.pt.x, info.pt.y);

    match msg {
        m if m == WM_RBUTTONDOWN || m == WM_MBUTTONDOWN => {
            // 只有配置的触发键才进入录制；threshold 此时读一次 config
            let want = ctx.config.read().unwrap().trigger;
            let matched = match want {
                MouseButton::Right => m == WM_RBUTTONDOWN,
                MouseButton::Middle => m == WM_MBUTTONDOWN,
            };
            if matched {
                let th = ctx.config.read().unwrap().threshold_px;
                state.threshold.store(th as u32, Ordering::Relaxed);
                *state.start.lock().unwrap() = Some((x, y));
                state.points.lock().unwrap().clear();
                state.gesturing.store(false, Ordering::Relaxed);
            }
        }
        WM_MOUSEMOVE => {
            if state.gesturing.load(Ordering::Relaxed) {
                state.points.lock().unwrap().push((x, y));
                // 放行移动事件：光标照常跟随，不再冻结
            } else if let Some((sx, sy)) = *state.start.lock().unwrap() {
                let threshold = f32::from_bits(state.threshold.load(Ordering::Relaxed));
                let d = ((x - sx) as f32).hypot((y - sy) as f32);
                if d >= threshold {
                    state.gesturing.store(true, Ordering::Relaxed);
                    let mut pts = state.points.lock().unwrap();
                    pts.push((sx, sy));
                    pts.push((x, y));
                }
            }
        }
        m if m == WM_RBUTTONUP || WM_MBUTTONUP == m => {
            let was_gesturing = state.gesturing.swap(false, Ordering::Relaxed);
            let had_start = state.start.lock().unwrap().take().is_some();
            if was_gesturing {
                let pts = state.points.lock().unwrap().clone();
                std::thread::spawn(move || {
                    if let Some(c) = CTX.get() {
                        engine::handle_path(c, pts);
                    }
                });
                return 1; // 吞掉抬起，防止弹菜单
            }
            let _ = had_start;
        }
        _ => {}
    }

    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
