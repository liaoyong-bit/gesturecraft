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

/// 作用域是否命中：scope 支持逗号分隔多个进程名关键字，双向包含即命中
fn scope_matches(gd: &GestureDef, proc_name: Option<&str>) -> bool {
    if gd.is_global() {
        return true;
    }
    let Some(p) = proc_name else { return false };
    gd.scope
        .to_ascii_lowercase()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|s| p.contains(s) || s.contains(p))
}

/// 在一组手势里尝试匹配并执行。命中返回 true。
/// under_cursor 模式：执行动作前先在轨迹起点单击激活该窗口。
fn attempt(
    ctx: &Ctx,
    seq: &str,
    gestures: &[GestureDef],
    tolerance: u32,
    start: (i32, i32),
    under_cursor: bool,
) -> bool {
    let (best, _) = match_gestures(seq, gestures, tolerance);
    if let Some((gd, d)) = best {
        let arrow = gesture::arrows(&gd.pattern);
        if under_cursor {
            action::activate_window_at(start.0, start.1);
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        crate::overlay::show_toast(format!("✓ {} {arrow}", gd.name));
        ctx.log(format!(
            "[手势] 命中「{}」{arrow}（画出 {}，容差差距 {d}）→ 执行动作",
            gd.name,
            gesture::arrows(seq)
        ));
        action::execute(&gd.action, &ctx.logs);
        true
    } else {
        false
    }
}

/// 钩子线程收到完整轨迹后调用（已在新线程中）。chord = 期间单击过左键。
/// 作用域规则：能识别所画窗口的进程 → 先匹配该程序专属手势（scope 非空且进程命中），
/// 没命中再回退全局手势；桌面/公共区域（Progman/WorkerW）直接用全局手势。
/// "鼠标所指窗口"模式按轨迹起点判窗，命中后先单击激活该窗口再发动作。
pub fn handle_path(ctx: &Ctx, pts: Vec<(i32, i32)>, chord: bool) {
    if ctx.paused.load(Ordering::Relaxed) {
        return;
    }
    let cfg = ctx.config.read().unwrap();
    let (tolerance, turn_px, under_cursor, gestures) = (
        cfg.fuzzy_tolerance,
        cfg.turn_px,
        cfg.target_mode == crate::config::TargetMode::UnderCursor,
        cfg.gestures.clone(),
    );
    drop(cfg);

    let mut seq = gesture::sequence_from_i32(&pts, SEGMENT_PX, turn_px);
    if chord {
        seq.push('L');
    }
    if seq.is_empty() {
        return;
    }

    // 手势轨迹：起点用于"鼠标所指窗口"判窗与激活
    let start = pts.first().copied().unwrap_or((0, 0));
    let (proc_name, is_desktop) = action::foreground_context(if under_cursor { Some(start) } else { None });

    // 1) 应用专属手势（桌面/公共区域跳过）
    if !is_desktop {
        let app_gestures: Vec<GestureDef> = gestures
            .iter()
            .filter(|g| !g.is_global() && scope_matches(g, proc_name.as_deref()))
            .cloned()
            .collect();
        if attempt(ctx, &seq, &app_gestures, tolerance, start, under_cursor) {
            return;
        }
    }

    // 2) 全局手势
    let global_gestures: Vec<GestureDef> =
        gestures.iter().filter(|g| g.is_global()).cloned().collect();
    if attempt(ctx, &seq, &global_gestures, tolerance, start, under_cursor) {
        return;
    }

    // 3) 都没命中：给出最接近提示（在全量手势里找）
    let (_, nearest) = match_gestures(&seq, &gestures, tolerance);
    let scope_desc = if is_desktop {
        "桌面".to_string()
    } else {
        proc_name.unwrap_or_else(|| "未知程序".into())
    };
    let near = nearest
        .map(|(gd, d)| format!("最接近「{}」差 {d}", gd.name))
        .unwrap_or_else(|| "没有启用的手势".into());
    crate::overlay::show_toast(format!("✗ 未匹配 {}", gesture::arrows(&seq)));
    ctx.log(format!(
        "[手势] 未匹配：{}（作用域 {scope_desc}；{near}）",
        gesture::arrows(&seq)
    ));
}
