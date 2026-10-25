use crate::action;
use crate::config::Config;
use crate::gesture;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub struct Ctx {
    pub config: Arc<RwLock<Config>>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub paused: Arc<AtomicBool>,
}

impl Ctx {
    pub fn log(&self, line: String) {
        action::push_log(&self.logs, line);
    }
}

/// 钩子线程收到完整轨迹后调用（已在新线程中）。
pub fn handle_path(ctx: &Ctx, pts: Vec<(i32, i32)>) {
    if ctx.paused.load(Ordering::Relaxed) {
        return;
    }
    let (threshold, gestures) = {
        let cfg = ctx.config.read().unwrap();
        (cfg.threshold_px, cfg.gestures.clone())
    };
    let seq = gesture::sequence_from_i32(&pts, threshold);
    if seq.is_empty() {
        return;
    }
    match gestures
        .iter()
        .find(|g| g.enabled && g.pattern == seq)
    {
        Some(g) => {
            ctx.log(format!("[手势] 命中 {} ({}), 执行动作", g.name, seq));
            action::execute(&g.action, &ctx.logs);
        }
        None => ctx.log(format!("[手势] 未匹配: {seq}")),
    }
}
