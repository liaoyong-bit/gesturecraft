//! Windows 低级鼠标钩子线程。
//! 「延迟触发」设计：触发键"按下"即被吞掉，应用层完全感知不到；
//! 松开时若位移超过阈值 → 判定为手势，轨迹交给引擎匹配（无菜单弹出）；
//! 若只是普通点击 → 由后台线程补发一次完整的按下+抬起，上下文菜单照常弹出。
//! 好处：应用从头到尾收不到"悬空的按下"，不会再出现拖拽状态卡死、
//! 需要再点一下右键才能结束的问题。
//!
//! 弦式手势：触发键按住期间的左键单击被吞掉并记为 'L' 段（不穿透给目标程序），
//! 手势结束时随轨迹一起交给引擎，形成如 "2L"（下拖+左点）的组合。
//!
//! 热路径上不读 config RwLock：threshold 在按下时缓存到原子量（f32 位模式一致存取）。

use crate::config::MouseButton;
use crate::engine::{self, Ctx};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

struct HookState {
    start: Mutex<Option<(i32, i32)>>,
    points: Mutex<Vec<(i32, i32)>>,
    gesturing: AtomicBool,
    /// 触发键按下时缓存的起手势阈值（f32 位模式），移动热路径不再读 config
    threshold_bits: AtomicU32,
    /// 本次按住触发键期间发生过左键单击（弦式标记）
    left_clicked: AtomicBool,
    /// 已吞掉左键按下（对应抬起也要吞掉，保持事件配对）
    left_swallowed: AtomicBool,
}

static STATE: OnceLock<HookState> = OnceLock::new();
static CTX: OnceLock<Ctx> = OnceLock::new();
/// 注入补偿点击期间置位，钩子对注入事件直接放行，避免自触发
static INJECTING: AtomicBool = AtomicBool::new(false);

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
        threshold_bits: AtomicU32::new(30.0f32.to_bits()),
        left_clicked: AtomicBool::new(false),
        left_swallowed: AtomicBool::new(false),
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

/// 普通点击补偿：后台线程注入完整按下+抬起（钩子内不能阻塞等待）
fn inject_click_async(right: bool, x: i32, y: i32) {
    std::thread::spawn(move || {
        use winapi::ctypes::c_int;
        use winapi::um::winuser::{
            SendInput, SetCursorPos, INPUT, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        };
        unsafe {
            SetCursorPos(x, y);
        }
        let (down_flag, up_flag) = if right {
            (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
        } else {
            (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP)
        };
        let mk = |flag: u32| -> INPUT {
            let mut input: INPUT = unsafe { std::mem::zeroed() };
            input.type_ = 0; // INPUT_MOUSE
            unsafe {
                input.u.mi_mut().dwFlags = flag;
            }
            input
        };
        std::thread::sleep(std::time::Duration::from_millis(15));
        INJECTING.store(true, Ordering::SeqCst);
        unsafe {
            let mut ev = [mk(down_flag), mk(up_flag)];
            SendInput(2, ev.as_mut_ptr(), std::mem::size_of::<INPUT>() as c_int);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        INJECTING.store(false, Ordering::SeqCst);
    });
}

unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use winapi::um::winuser::{
        CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
        WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    let pass = || CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);

    if code < 0 || INJECTING.load(Ordering::SeqCst) {
        return pass();
    }

    let (Some(state), Some(_ctx)) = (STATE.get(), CTX.get()) else {
        return pass();
    };

    let info = &*(lparam as *const MSLLHOOKSTRUCT);
    let msg = wparam as u32;
    let (x, y) = (info.pt.x, info.pt.y);

    match msg {
        m if m == WM_RBUTTONDOWN || m == WM_MBUTTONDOWN => {
            let want = _ctx.config.read().unwrap().trigger;
            let matched = match want {
                MouseButton::Right => m == WM_RBUTTONDOWN,
                MouseButton::Middle => m == WM_MBUTTONDOWN,
            };
            if matched {
                // 延迟触发：吞掉按下，等松开时再决定是手势还是普通点击
                let th = _ctx.config.read().unwrap().threshold_px;
                state.threshold_bits.store(th.to_bits(), Ordering::Relaxed);
                *state.start.lock().unwrap() = Some((x, y));
                state.points.lock().unwrap().clear();
                state.gesturing.store(false, Ordering::Relaxed);
                state.left_clicked.store(false, Ordering::Relaxed);
                state.left_swallowed.store(false, Ordering::Relaxed);
                return 1;
            }
            pass()
        }
        // 弦式：触发键按住期间，左键单击被吞掉并记为 'L' 段
        WM_LBUTTONDOWN => {
            if state.start.lock().unwrap().is_some() {
                state.left_clicked.store(true, Ordering::Relaxed);
                state.left_swallowed.store(true, Ordering::Relaxed);
                return 1;
            }
            pass()
        }
        WM_LBUTTONUP => {
            if state.left_swallowed.swap(false, Ordering::Relaxed) {
                return 1;
            }
            pass()
        }
        WM_MOUSEMOVE => {
            if state.gesturing.load(Ordering::Relaxed) {
                state.points.lock().unwrap().push((x, y));
                pass() // 光标照常跟随
            } else if let Some((sx, sy)) = *state.start.lock().unwrap() {
                let threshold = f32::from_bits(state.threshold_bits.load(Ordering::Relaxed));
                let d = ((x - sx) as f32).hypot((y - sy) as f32);
                if d >= threshold {
                    state.gesturing.store(true, Ordering::Relaxed);
                    let mut pts = state.points.lock().unwrap();
                    pts.push((sx, sy));
                    pts.push((x, y));
                }
                pass()
            } else {
                pass()
            }
        }
        m if m == WM_RBUTTONUP || m == WM_MBUTTONUP => {
            let was_gesturing = state.gesturing.swap(false, Ordering::Relaxed);
            if state.start.lock().unwrap().take().is_some() {
                if was_gesturing {
                    // 手势结束：立即把轨迹交给引擎检测；吞掉抬起（应用从没收到过按下）
                    let pts = state.points.lock().unwrap().clone();
                    let chord = state.left_clicked.swap(false, Ordering::Relaxed);
                    std::thread::spawn(move || {
                        if let Some(c) = CTX.get() {
                            engine::handle_path(c, pts, chord);
                        }
                    });
                    return 1;
                }
                // 普通点击：补发完整的按下+抬起，恢复上下文菜单等原生行为
                let right = msg == WM_RBUTTONUP;
                let chord = state.left_clicked.swap(false, Ordering::Relaxed);
                if chord {
                    // 按住触发键时点过左键但没画手势：左键事件已消费，不补发
                    // 避免把误吞的左键变成一次突兀的右键菜单点击
                    return 1;
                }
                inject_click_async(right, x, y);
                return 1;
            }
            pass()
        }
        _ => pass(),
    }
}
