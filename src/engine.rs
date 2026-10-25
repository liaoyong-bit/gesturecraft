use crate::action;
use crate::config::{Config, GestureDef};
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

    /// UI 线程启动时注册 egui 上下文
    pub fn set_ui_ctx_once(
        cell: &Arc<OnceLock<eframe::egui::Context>>,
        ctx: eframe::egui::Context,
    ) {
        let _ = cell.set(ctx);
    }
}

/// 手势分段阈值（识别段长），与起手阈值解耦：短段才能拆出连贯的拐弯
const SEGMENT_PX: f32 = 25.0;

/// 模糊匹配核心：在启用手势里找编辑距离最小、且 ≤ 有效容差者。
/// 返回 (命中者, 最接近者)，均带距离。
pub fn match_gestures<'a>(
    seq: &str,
    gestures: &'a [GestureDef],
    user_tolerance: u32,
) -> (Option<(&'a GestureDef, usize)>, Option<(&'a GestureDef, usize)>) {
    let mut best: Option<(&GestureDef, usize)> = None;
    let mut nearest: Option<(&GestureDef, usize)> = None;
    for gd in gestures.iter().filter(|g| g.enabled && !g.pattern.is_empty()) {
        let d = gesture::fuzzy_distance(seq, &gd.pattern);
        if nearest.map_or(true, |(_, nd)| d < nd) {
            nearest = Some((gd, d));
        }
        let tol = gesture::tolerance_for(&gd.pattern, user_tolerance);
        if d <= tol as usize && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((gd, d));
        }
    }
    (best, nearest)
}

/// 钩子线程收到完整轨迹后调用（已在新线程中）。
pub fn handle_path(ctx: &Ctx, pts: Vec<(i32, i32)>) {
    if ctx.paused.load(Ordering::Relaxed) {
        return;
    }
    let cfg = ctx.config.read().unwrap();
    let (tolerance, gestures) = (cfg.fuzzy_tolerance, cfg.gestures.clone());
    drop(cfg);

    let seq = gesture::sequence_from_i32(&pts, SEGMENT_PX);
    if seq.is_empty() {
        return;
    }
    let (best, nearest) = match_gestures(&seq, &gestures, tolerance);
    match best {
        Some((gd, d)) => {
            let arrow = gesture::arrows(&gd.pattern);
            ctx.log(format!(
                "[手势] 命中「{}」{arrow}（画出 {}，容差差距 {d}）→ 执行动作",
                gd.name,
                gesture::arrows(&seq)
            ));
            action::execute(&gd.action, &ctx.logs);
        }
        None => {
            let near = nearest
                .map(|(gd, d)| format!("最接近「{}」差 {d}", gd.name))
                .unwrap_or_else(|| "没有启用的手势".into());
            ctx.log(format!("[手势] 未匹配：{}（{near}）", gesture::arrows(&seq)));
        }
    }
}
