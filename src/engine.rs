use crate::action;
use crate::config::Config;
use crate::gesture;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

pub struct Ctx {
    pub config: Arc<RwLock<Config>>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub paused: Arc<AtomicBool>,
    /// UI 线程启动后填入；引擎用它请求重绘，让日志即时显示
    pub ui_ctx: Arc<OnceLock<eframe::egui::Context>>,
}

impl Ctx {
    pub fn log(&self, line: String) {
        action::push_log(&self.logs, line);
        if let Some(c) = self.ui_ctx.get() {
            c.request_repaint();
        }
    }

    /// 引擎线程等待 UI 上下文就绪（最多 ~10s），保证启动即注册
    pub fn set_ui_ctx_once(cell: &Arc<OnceLock<eframe::egui::Context>>, ctx: eframe::egui::Context) {
        let _ = cell.set(ctx);
    }
}

/// 手势分段阈值：与起手阈值无关，用固定短段避免漏掉拐弯
const SEGMENT_PX: f32 = 25.0;

/// 钩子线程收到完整轨迹后调用（已在新线程中）。
pub fn handle_path(ctx: &Ctx, pts: Vec<(i32, i32)>) {
    if ctx.paused.load(Ordering::Relaxed) {
        return;
    }
    let gestures = ctx.config.read().unwrap().gestures.clone();
    let seq = gesture::sequence_from_i32(&pts, SEGMENT_PX);
    if seq.is_empty() {
        return;
    }
    match gestures.iter().find(|g| g.enabled && g.pattern == seq) {
        Some(g) => {
            ctx.log(format!("[手势] 命中 {} ({seq})，执行动作", g.name));
            action::execute(&g.action, &ctx.logs);
        }
        None => ctx.log(format!("[手势] 未匹配: {seq}")),
    }
}
