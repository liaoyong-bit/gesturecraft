use crate::action;
use crate::config::{
    browser_op_label, default_gestures, system_op_label, window_op_label, ActionDef, Config,
    GestureDef, MouseButton, CATEGORIES, LEVELS,
};
use crate::engine;
use crate::gesture;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

// ---------- 主题 ----------
const ACCENT: egui::Color32 = egui::Color32::from_rgb(47, 111, 237); // 主色·蓝
const ACCENT_DEEP: egui::Color32 = egui::Color32::from_rgb(32, 84, 196);
const NAV_BG: egui::Color32 = egui::Color32::from_rgb(23, 32, 54); // 侧栏·深藏青
const NAV_TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(178, 188, 208);
const PAGE_BG: egui::Color32 = egui::Color32::from_rgb(243, 246, 251); // 页面底色
const CARD_BG: egui::Color32 = egui::Color32::WHITE;
const CARD_STROKE: egui::Color32 = egui::Color32::from_rgb(228, 232, 240);
const INK: egui::Color32 = egui::Color32::from_rgb(35, 42, 58);
const MUTED: egui::Color32 = egui::Color32::from_rgb(128, 137, 155);
const GOOD: egui::Color32 = egui::Color32::from_rgb(24, 140, 66);
const BAD: egui::Color32 = egui::Color32::from_rgb(198, 52, 52);

const GLOBAL_KEY: &str = "__global__";

fn cat_color(cat: &str) -> (egui::Color32, egui::Color32) {
    match cat {
        "编辑" => (egui::Color32::from_rgb(226, 242, 233), egui::Color32::from_rgb(24, 120, 70)),
        "导航" => (egui::Color32::from_rgb(228, 238, 252), egui::Color32::from_rgb(43, 94, 196)),
        "窗口" => (egui::Color32::from_rgb(238, 232, 250), egui::Color32::from_rgb(112, 72, 190)),
        "浏览器" => (egui::Color32::from_rgb(223, 242, 244), egui::Color32::from_rgb(16, 124, 134)),
        "系统" => (egui::Color32::from_rgb(253, 238, 224), egui::Color32::from_rgb(186, 98, 20)),
        "应用" => (egui::Color32::from_rgb(250, 230, 242), egui::Color32::from_rgb(168, 52, 124)),
        _ => (egui::Color32::from_rgb(234, 236, 241), egui::Color32::from_rgb(100, 108, 124)),
    }
}

/// 白底圆角卡片 + 细描边 + 柔和投影
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::none()
        .fill(CARD_BG)
        .rounding(egui::Rounding::same(10.0))
        .stroke(egui::Stroke::new(1.0_f32, CARD_STROKE))
        .shadow(egui::epaint::Shadow {
            offset: egui::vec2(0.0, 2.0),
            blur: 10.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(14),
        })
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, add);
}

fn badge(ui: &mut egui::Ui, text: &str, fill: egui::Color32, fg: egui::Color32) {
    egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(6.0, 1.5))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(fg));
        });
}

fn cat_badge(ui: &mut egui::Ui, cat: &str) {
    let (bg, fg) = cat_color(cat);
    badge(ui, cat, bg, fg);
}

fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
        .fill(ACCENT)
        .rounding(egui::Rounding::same(8.0))
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Gestures,
    Settings,
    About,
}

pub struct UiApp {
    config: Arc<RwLock<Config>>,
    logs: Arc<Mutex<Vec<String>>>,
    paused: Arc<AtomicBool>,
    tab: Tab,
    /// 当前作用域：GLOBAL_KEY=全局，否则为进程名关键字
    scope_sel: String,
    /// 等级页签：0=一级 1=二级 2=三级 3=弦式
    level_sel: usize,
    /// 添加程序弹窗
    show_add_program: bool,
    prog_query: String,
    /// 正在编辑的手势在 config.gestures 中的下标；None 表示新建
    edit_index: Option<usize>,
    /// 新建态显式标志（修复"新建按钮点了没反应"）
    creating: bool,
    draft: GestureDef,
    pad_points: Vec<egui::Vec2>,
    /// 测试框状态：最近一次拖动的轨迹与匹配结果
    test_points: Vec<egui::Vec2>,
    test_result: Option<Result<String, String>>,
    status: String,
    /// 保存时检测到的重复项（非空时需用户点「仍然保存」二次确认）
    pending_conflicts: Vec<String>,
    /// 全局捕获提示（与引擎共享，显示 ~1.2 秒）
    toast: Arc<Mutex<Option<(String, std::time::Instant)>>>,
}

pub fn run_ui(
    config: Arc<RwLock<Config>>,
    logs: Arc<Mutex<Vec<String>>>,
    paused: Arc<AtomicBool>,
    ui_ctx: Arc<std::sync::OnceLock<egui::Context>>,
    toast: Arc<Mutex<Option<(String, std::time::Instant)>>>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 780.0])
            .with_min_inner_size([940.0, 680.0])
            .with_title("GestureCraft"),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "GestureCraft",
        options,
        Box::new(move |cc| {
            engine::Ctx::set_ui_ctx_once(&ui_ctx, cc.egui_ctx.clone());
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            setup_style(&cc.egui_ctx);
            load_cjk_font(&cc.egui_ctx);
            let draft = default_draft();
            Box::new(UiApp {
                config,
                logs,
                paused,
                tab: Tab::Gestures,
                scope_sel: GLOBAL_KEY.to_string(),
                level_sel: 0,
                show_add_program: false,
                prog_query: String::new(),
                edit_index: None,
                creating: false,
                draft,
                pad_points: Vec::new(),
                test_points: Vec::new(),
                test_result: None,
                status: String::new(),
                pending_conflicts: Vec::new(),
                toast,
            })
        }),
    );
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    let r = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = r;
    style.visuals.widgets.active.rounding = r;
    style.visuals.widgets.inactive.rounding = r;
    style.visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(214, 220, 232));
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(238, 242, 248);
    ctx.set_style(style);
}

fn load_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            let name = "cjk".to_string();
            fonts.font_data.insert(name.clone(), egui::FontData::from_owned(data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, name);
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn default_draft() -> GestureDef {
    GestureDef {
        name: "新手势".into(),
        pattern: String::new(),
        enabled: true,
        action: ActionDef::RunApp { path: String::new(), args: String::new() },
        category: "编辑".into(),
        scope: String::new(),
    }
}

// ---------- 田字格手势图示 ----------

fn dashed_line(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, color: egui::Color32) {
    let v = b - a;
    let len = v.length();
    if len < 1.0 {
        return;
    }
    let dir = v / len;
    let mut t = 0.0_f32;
    while t < len {
        let t2 = (t + 3.0).min(len);
        p.line_segment([a + dir * t, a + dir * t2], egui::Stroke::new(0.5_f32, color));
        t += 6.0;
    }
}

/// 米字格 + 中心起笔的方向折线；每个转折点带序号圆点，弦式的左键单击画 "L" 点。
fn draw_tian(ui: &mut egui::Ui, side: f32, pattern: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let grid = egui::Color32::from_rgb(196, 204, 220);
    let diag = egui::Color32::from_rgb(216, 222, 234);
    p.rect_filled(rect, 5.0, egui::Color32::WHITE);
    p.rect_stroke(rect, 5.0, (0.5, egui::Color32::from_rgb(158, 166, 184)));
    let c = rect.center();
    dashed_line(
        &p,
        egui::pos2(rect.left() + 3.0, c.y),
        egui::pos2(rect.right() - 3.0, c.y),
        grid,
    );
    dashed_line(
        &p,
        egui::pos2(c.x, rect.top() + 3.0),
        egui::pos2(c.x, rect.bottom() - 3.0),
        grid,
    );
    dashed_line(
        &p,
        egui::pos2(rect.left() + 5.0, rect.top() + 5.0),
        egui::pos2(rect.right() - 5.0, rect.bottom() - 5.0),
        diag,
    );
    dashed_line(
        &p,
        egui::pos2(rect.right() - 5.0, rect.top() + 5.0),
        egui::pos2(rect.left() + 5.0, rect.bottom() - 5.0),
        diag,
    );

    if pattern.is_empty() {
        p.circle_stroke(c, 4.0, (1.0, grid));
        return;
    }
    let segs = pattern.chars().filter(|ch| *ch != 'L').count().max(1);
    let unit = match segs {
        1 => side * 0.30,
        2 => side * 0.27,
        _ => side * 0.25,
    };
    let mut pt = c;
    let mut pts = vec![c];
    let mut chord_pos = None;
    for ch in pattern.chars() {
        if ch == 'L' {
            chord_pos = Some(egui::pos2(pt.x + unit * 0.45, pt.y - unit * 0.45));
            continue;
        }
        let (ux, uy) = gesture::axis_of(ch);
        pt = egui::pos2(pt.x + ux * unit, pt.y + uy * unit);
        pts.push(pt);
    }
    p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(2.5_f32, ACCENT)));
    for (i, &v) in pts.iter().enumerate().skip(1) {
        p.circle_filled(v, 6.5, ACCENT);
        p.text(
            v,
            egui::Align2::CENTER_CENTER,
            i.to_string(),
            egui::FontId::monospace(9.0),
            egui::Color32::WHITE,
        );
    }
    if let Some(cp) = chord_pos {
        p.circle_filled(cp, 7.0, ACCENT_DEEP);
        p.text(
            cp,
            egui::Align2::CENTER_CENTER,
            "L",
            egui::FontId::monospace(8.0),
            egui::Color32::WHITE,
        );
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_nav(ctx);

        // 底部日志面板必须先于 CentralPanel 添加，否则被挤成零高度不可见
        egui::TopBottomPanel::bottom("log")
            .frame(
                egui::Frame::none().fill(CARD_BG).inner_margin(egui::Margin::symmetric(12.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("运行日志").strong().small().color(MUTED));
                let logs = self.logs.lock().unwrap();
                egui::ScrollArea::vertical()
                    .max_height(84.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let start = logs.len().saturating_sub(6);
                        for line in &logs[start..] {
                            ui.label(
                                egui::RichText::new(line.as_str())
                                    .small()
                                    .color(MUTED)
                                    .monospace(),
                            );
                        }
                    });
            });

        // 手势库页：左侧作用域双栏（全局 / 程序）
        if self.tab == Tab::Gestures {
            egui::SidePanel::left("scope_col")
                .exact_width(186.0)
                .resizable(false)
                .frame(egui::Frame::none().fill(PAGE_BG).inner_margin(egui::Margin::same(0.0)))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Frame::none()
                            .outer_margin(egui::Margin::same(0.0))
                            .show(ui, |ui| self.show_scope_col(ui));
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(PAGE_BG).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::Gestures => self.show_gestures(ui),
                    Tab::Settings => self.show_settings(ui),
                    Tab::About => self.show_about(ui),
                });
            });

        self.show_add_program_dialog(ctx);
        self.show_toast(ctx);
    }
}

impl UiApp {
    // ---------- 框架 ----------

    fn show_nav(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .exact_width(182.0)
            .frame(egui::Frame::none().fill(NAV_BG).inner_margin(egui::Margin::symmetric(14.0, 0.0)))
            .show(ctx, |ui| {
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("🖱").size(22.0));
                    ui.label(
                        egui::RichText::new("GestureCraft")
                            .strong()
                            .size(17.0)
                            .color(egui::Color32::WHITE),
                    );
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("原创全局鼠标手势").small().color(NAV_TEXT_DIM));
                });
                ui.add_space(16.0);

                for (tab, icon, label) in [
                    (Tab::Gestures, "🪄", "手势库"),
                    (Tab::Settings, "⚙", "设置与测试"),
                    (Tab::About, "ℹ", "关于"),
                ] {
                    let sel = self.tab == tab;
                    let text = egui::RichText::new(format!("{icon}  {label}"))
                        .size(15.0)
                        .color(if sel { egui::Color32::WHITE } else { NAV_TEXT_DIM });
                    if ui
                        .add_sized(
                            [ui.available_width(), 34.0],
                            egui::SelectableLabel::new(sel, text),
                        )
                        .clicked()
                    {
                        self.tab = tab;
                    }
                }

                ui.add_space(14.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("v0.5.0").small().color(NAV_TEXT_DIM));
                    ui.add_space(8.0);
                    let paused = self.paused.load(Ordering::Relaxed);
                    let label = if paused { "▶  恢复监听" } else { "⏸  暂停监听" };
                    let btn = egui::Button::new(
                        egui::RichText::new(label).size(14.0).color(egui::Color32::WHITE),
                    )
                    .fill(if paused {
                        ACCENT
                    } else {
                        egui::Color32::from_rgb(52, 76, 130)
                    })
                    .rounding(egui::Rounding::same(8.0))
                    .min_size(egui::vec2(ui.available_width(), 34.0));
                    if ui.add(btn).clicked() {
                        self.paused.store(!paused, Ordering::Relaxed);
                    }
                    if paused {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("● 已暂停，手势不生效")
                                .small()
                                .color(egui::Color32::from_rgb(255, 170, 90)),
                        );
                    }
                    ui.add_space(0.0);
                });
            });
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let toast_active = {
            let mut t = self.toast.lock().unwrap();
            match t.as_ref() {
                Some((_, t0)) if t0.elapsed() < std::time::Duration::from_millis(1200) => {
                    Some(t0.elapsed())
                }
                Some(_) => {
                    *t = None;
                    None
                }
                None => None,
            }
        };
        if let Some(elapsed) = toast_active {
            let text = self.toast.lock().unwrap().as_ref().map(|(s, _)| s.clone()).unwrap_or_default();
            egui::Area::new(egui::Id::new("capture_toast"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
                .show(ctx, |ui| {
                    let alpha = if elapsed.as_millis() > 900 { 0.45 } else { 1.0 };
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_unmultiplied(23, 32, 54, (alpha * 255.0) as u8))
                        .rounding(egui::Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(18.0, 10.0))
                        .shadow(egui::epaint::Shadow {
                            offset: egui::vec2(0.0, 3.0),
                            blur: 16.0,
                            spread: 0.0,
                            color: egui::Color32::from_black_alpha(40),
                        })
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(text)
                                    .strong()
                                    .size(16.0)
                                    .color(egui::Color32::WHITE),
                            );
                        });
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    // ---------- 作用域双栏 ----------

    fn program_display(key: &str) -> String {
        action::candidate_programs()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|| {
                let mut c = key.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => key.to_string(),
                }
            })
    }

    fn show_scope_col(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("作用域").strong().size(15.0).color(INK));
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let sel = self.scope_sel == GLOBAL_KEY;
            let text = egui::RichText::new("🌐  全局").size(14.0).color(if sel {
                egui::Color32::WHITE
            } else {
                INK
            });
            let label = egui::SelectableLabel::new(sel, text);
            if ui.add_sized([ui.available_width() - 16.0, 32.0], label).clicked() {
                self.scope_sel = GLOBAL_KEY.to_string();
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("程序").small().strong().color(MUTED));
        });
        ui.add_space(4.0);

        let programs: Vec<String> = {
            let cfg = self.config.read().unwrap();
            cfg.programs.clone()
        };
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_min_width(ui.available_width());
                for key in &programs {
                    let sel = self.scope_sel == *key;
                    let disp = Self::program_display(key);
                    ui.horizontal(|ui| {
                        let text = egui::RichText::new(format!("🗔  {disp}"))
                            .size(13.5)
                            .color(if sel { egui::Color32::WHITE } else { INK });
                        if ui
                            .add_sized(
                                [ui.available_width() - 26.0, 28.0],
                                egui::SelectableLabel::new(sel, text),
                            )
                            .clicked()
                        {
                            self.scope_sel = key.clone();
                        }
                        if ui.small_button("✕").on_hover_text("移除该程序作用域").clicked() {
                            let mut cfg = self.config.write().unwrap();
                            cfg.programs.retain(|k| k != key);
                            cfg.gestures.retain(|g| g.scope != *key);
                            let _ = cfg.save();
                            if self.scope_sel == *key {
                                self.scope_sel = GLOBAL_KEY.to_string();
                            }
                        }
                    });
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            if ui
                .add_sized([ui.available_width() - 16.0, 30.0], egui::Button::new("＋ 添加程序"))
                .clicked()
            {
                self.show_add_program = true;
                self.prog_query.clear();
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("全局手势所有程序共用；\n程序手势仅在该程序窗口\n中优先生效，桌面直接\n走全局。")
                    .small()
                    .color(MUTED),
            );
        });
    }

    fn show_add_program_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_add_program {
            return;
        }
        let mut open = self.show_add_program;
        egui::Window::new("添加程序作用域")
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("勾选要添加的程序（含当前运行中的程序与常用程序）")
                        .small()
                        .color(MUTED),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.prog_query)
                        .hint_text("搜索…")
                        .desired_width(ui.available_width()),
                );
                ui.add_space(4.0);
                let query = self.prog_query.to_lowercase();
                let current: Vec<String> =
                    self.config.read().unwrap().programs.clone();
                let cands = action::candidate_programs();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (key, disp) in &cands {
                        if !query.is_empty()
                            && !key.contains(&query)
                            && !disp.to_lowercase().contains(&query)
                        {
                            continue;
                        }
                        let mut checked = current.contains(key);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut checked, disp).changed() {
                                let mut cfg = self.config.write().unwrap();
                                if checked {
                                    if !cfg.programs.contains(key) {
                                        cfg.programs.push(key.clone());
                                    }
                                } else {
                                    cfg.programs.retain(|k| k != key);
                                    cfg.gestures.retain(|g| g.scope != *key);
                                    if self.scope_sel == *key {
                                        self.scope_sel = GLOBAL_KEY.to_string();
                                    }
                                }
                                let _ = cfg.save();
                            }
                            ui.label(
                                egui::RichText::new(key.as_str()).small().color(MUTED).monospace(),
                            );
                        });
                    }
                });
            });
        self.show_add_program = open;
    }

    // ---------- 手势库 ----------

    fn scope_gesture_visible(&self, g: &GestureDef) -> bool {
        if self.scope_sel == GLOBAL_KEY {
            g.is_global()
        } else {
            g.scope
                .to_ascii_lowercase()
                .split(',')
                .any(|s| s.trim() == self.scope_sel)
        }
    }

    fn show_gestures(&mut self, ui: &mut egui::Ui) {
        let scope_title = if self.scope_sel == GLOBAL_KEY {
            "全局手势".to_string()
        } else {
            format!("「{}」的专属手势", Self::program_display(&self.scope_sel))
        };

        // 标题行
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&scope_title).strong().size(22.0).color(INK));
                ui.label(
                    egui::RichText::new("按一级 / 二级 / 三级 / 弦式分面板；所有手势可自由改绑动作")
                        .small()
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("恢复默认手势").on_hover_text("用 v0.5 出厂默认表替换当前全部手势").clicked() {
                    let mut cfg = self.config.write().unwrap();
                    cfg.gestures = default_gestures();
                    let _ = cfg.save();
                    self.status = "已恢复默认手势表".into();
                }
                if ui.add(primary_button("＋ 新建手势")).clicked() {
                    self.edit_index = None;
                    self.creating = true;
                    self.draft = default_draft();
                    if self.scope_sel != GLOBAL_KEY {
                        self.draft.scope = self.scope_sel.clone();
                    }
                    self.pad_points.clear();
                }
            });
        });
        ui.add_space(6.0);

        // 等级页签
        let counts: [usize; 4] = {
            let cfg = self.config.read().unwrap();
            let mut c = [0usize; 4];
            for g in cfg.gestures.iter().filter(|g| self.scope_gesture_visible(g)) {
                for (i, lv) in LEVELS.iter().enumerate() {
                    if g.level() == *lv {
                        c[i] += 1;
                    }
                }
            }
            c
        };
        ui.horizontal(|ui| {
            for (i, lv) in LEVELS.iter().enumerate() {
                if ui
                    .add(egui::SelectableLabel::new(
                        self.level_sel == i,
                        egui::RichText::new(format!("{lv} {}", counts[i])).size(14.0),
                    ))
                    .clicked()
                {
                    self.level_sel = i;
                }
            }
        });
        ui.add_space(4.0);

        // 手势行
        let snapshot: Vec<(usize, GestureDef)> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures
                .iter()
                .cloned()
                .enumerate()
                .filter(|(_, g)| self.scope_gesture_visible(g))
                .filter(|(_, g)| g.level() == LEVELS[self.level_sel])
                .collect()
        };
        if snapshot.is_empty() {
            card(ui, |ui| {
                ui.label(
                    egui::RichText::new("此面板暂无手势。点右上角「＋ 新建手势」（作用域已自动带上当前程序），或到「恢复默认手势」。")
                        .color(MUTED),
                );
            });
        }
        for (i, gd) in snapshot {
            let name = gd.name.clone();
            let arrow = gesture::arrows(&gd.pattern);
            let summary = gd.action.summary();
            let cat = gd.category.clone();
            let is_spare = gd.name.starts_with("三级备选");
            let side = if gd.level() == "三级" { 62.0 } else { 54.0 };
            let mut en = gd.enabled;
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    draw_tian(ui, side, &gd.pattern);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&name).strong().size(15.5).color(INK),
                            );
                            if !cat.is_empty() {
                                cat_badge(ui, &cat);
                            }
                            if is_spare {
                                badge(
                                    ui,
                                    "备选",
                                    egui::Color32::from_rgb(238, 240, 246),
                                    MUTED,
                                );
                            }
                            if !gd.enabled && !is_spare {
                                ui.label(egui::RichText::new("已停用").small().color(MUTED));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&arrow).strong().size(15.0).color(ACCENT),
                            );
                            ui.label(egui::RichText::new("·").color(MUTED));
                            ui.label(egui::RichText::new(&summary).color(MUTED));
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("删除").clicked() {
                            self.config.write().unwrap().gestures.remove(i);
                            let _ = self.config.read().unwrap().save();
                            self.status = format!("已删除: {name}");
                        }
                        if ui.small_button("编辑").clicked() {
                            self.edit_index = Some(i);
                            self.creating = false;
                            self.draft = self.config.read().unwrap().gestures[i].clone();
                            self.pad_points.clear();
                        }
                        if ui.checkbox(&mut en, "启用").changed() {
                            self.config.write().unwrap().gestures[i].enabled = en;
                            let _ = self.config.read().unwrap().save();
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }

        if !self.status.is_empty() {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(&self.status).small().color(GOOD));
        }

        // ---------- 编辑器 ----------
        if self.edit_index.is_some() || self.creating {
            ui.add_space(10.0);
            self.show_editor(ui);
        }
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let title = if self.creating { "新建手势" } else { "编辑手势" };
        card(ui, |ui| {
            ui.label(egui::RichText::new(title).strong().size(16.0).color(INK));
            ui.add_space(2.0);

            egui::Grid::new("editor")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("名称").color(INK));
                    ui.add_sized([320.0, 24.0], egui::TextEdit::singleline(&mut self.draft.name));
                    ui.end_row();

                    ui.label(egui::RichText::new("分类").color(INK));
                    egui::ComboBox::from_id_source("cat")
                        .selected_text(self.draft.category.clone())
                        .show_ui(ui, |ui| {
                            for c in CATEGORIES {
                                ui.selectable_value(&mut self.draft.category, c.to_string(), c);
                            }
                        });
                    ui.end_row();

                    ui.label(egui::RichText::new("作用域").color(INK));
                    ui.vertical(|ui| {
                        ui.add_sized(
                            [320.0, 24.0],
                            egui::TextEdit::singleline(&mut self.draft.scope)
                                .hint_text("留空=全局生效；填进程名关键字如 msedge、chrome"),
                        );
                        ui.label(
                            egui::RichText::new(
                                "命中该程序的窗口时优先使用本手势（可逗号分隔多个），否则走全局手势",
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                    ui.end_row();

                    ui.label(egui::RichText::new("动作类型").color(INK));
                    let kinds = [
                        "运行程序", "命令行", "打开网址", "发送按键", "按键组合", "窗口控制",
                        "音量", "系统", "浏览器", "粘贴并回车", "脚本",
                    ];
                    let current = kinds
                        .iter()
                        .position(|k| *k == self.draft.action.kind_name())
                        .unwrap_or(0);
                    egui::ComboBox::from_id_source("kind")
                        .selected_text(self.draft.action.kind_name())
                        .show_ui(ui, |ui| {
                            for (i, k) in kinds.iter().enumerate() {
                                if ui.selectable_label(i == current, *k).clicked() {
                                    self.draft.action = make_action(i);
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(egui::RichText::new("动作参数").color(INK));
                    match &mut self.draft.action {
                        ActionDef::RunApp { path, args } => {
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(path).hint_text("程序路径"),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(args)
                                        .hint_text("参数（空格分隔）"),
                                );
                            });
                        }
                        ActionDef::Command { cmd } => {
                            ui.add(egui::TextEdit::singleline(cmd).hint_text("命令行"));
                        }
                        ActionDef::Url { url } => {
                            ui.add(egui::TextEdit::singleline(url).hint_text("https://..."));
                        }
                        ActionDef::SendKeys { keys } => {
                            ui.add(
                                egui::TextEdit::singleline(keys)
                                    .hint_text("如 hello{ENTER}，支持 {TAB} {SLEEP 100}"),
                            );
                        }
                        ActionDef::Hotkey { combo } => {
                            ui.add(
                                egui::TextEdit::singleline(combo)
                                    .hint_text("任意组合键：ctrl+alt+d、f9、win+shift+1…手势触发即替你按下"),
                            );
                        }
                        ActionDef::Window { op } => {
                            egui::ComboBox::from_id_source("wop")
                                .selected_text(window_op_label(op))
                                .show_ui(ui, |ui| {
                                    for (v, label) in [
                                        ("minimize", "最小化"),
                                        ("maximize", "最大化"),
                                        ("restore", "还原"),
                                        ("close", "关闭"),
                                        ("topmost", "置顶切换"),
                                        ("snap_left", "左贴边"),
                                        ("snap_right", "右贴边"),
                                    ] {
                                        ui.selectable_value(op, v.into(), label);
                                    }
                                });
                        }
                        ActionDef::Volume { op } => {
                            egui::ComboBox::from_id_source("vop")
                                .selected_text(op.clone())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(op, "up".into(), "增大");
                                    ui.selectable_value(op, "down".into(), "减小");
                                    ui.selectable_value(op, "mute".into(), "静音");
                                });
                        }
                        ActionDef::System { op } => {
                            egui::ComboBox::from_id_source("sop")
                                .selected_text(system_op_label(op))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(op, "show_desktop".into(), "显示桌面");
                                    ui.selectable_value(op, "task_view".into(), "任务视图");
                                    ui.selectable_value(
                                        op,
                                        "clipboard_history".into(),
                                        "剪贴板历史",
                                    );
                                    ui.selectable_value(op, "lock".into(), "锁屏");
                                });
                        }
                        ActionDef::Browser { op } => {
                            egui::ComboBox::from_id_source("bop")
                                .selected_text(browser_op_label(op))
                                .show_ui(ui, |ui| {
                                    for (v, label) in [
                                        ("back", "后退"),
                                        ("forward", "前进"),
                                        ("refresh", "刷新"),
                                        ("stop", "停止加载"),
                                        ("open_link_new_tab", "链接新标签打开（中键）"),
                                        ("open_link_new_window", "链接新窗口打开（Shift+单击）"),
                                        ("close_tabs_right", "关闭右侧标签（暂不支持）"),
                                        ("close_other_tabs", "关闭其他标签（暂不支持）"),
                                    ] {
                                        ui.selectable_value(op, v.into(), label);
                                    }
                                });
                        }
                        ActionDef::PasteAndGo => {
                            ui.label(
                                egui::RichText::new(
                                    "浏览器：定位地址栏 → 粘贴 → 回车；其他程序：普通粘贴",
                                )
                                .small()
                                .color(MUTED),
                            );
                        }
                        ActionDef::Script { code } => {
                            ui.add_sized([420.0, 90.0], egui::TextEdit::multiline(code));
                        }
                    }
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("方向序列（点九宫格快录，或在下方手绘板拖画；弦式可追加左键单击）")
                    .color(INK),
            );
            ui.horizontal(|ui| {
                egui::Grid::new("dirpad")
                    .num_columns(3)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        for row in [["7", "8", "9"], ["4", "", "6"], ["1", "2", "3"]] {
                            for cell in row {
                                if cell.is_empty() {
                                    ui.label("");
                                } else if ui
                                    .button(egui::RichText::new(
                                        gesture::arrow_for(cell.chars().next().unwrap()),
                                    )
                                    .size(18.0))
                                    .clicked()
                                {
                                    self.draft.pattern.push_str(cell);
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.vertical(|ui| {
                    draw_tian(ui, 76.0, &self.draft.pattern);
                    ui.horizontal(|ui| {
                        if ui.small_button("⌫ 退格").clicked() {
                            self.draft.pattern.pop();
                        }
                        if ui.small_button("清空").clicked() {
                            self.draft.pattern.clear();
                            self.pad_points.clear();
                        }
                        if ui.small_button("＋ 左键单击").clicked() {
                            self.draft.pattern.push('L');
                        }
                    });
                    ui.label(
                        egui::RichText::new("弦式：按住触发键拖出方向后单击左键\n含左键单击的手势只做精确匹配")
                            .small()
                            .color(MUTED),
                    );
                });
            });

            // 手绘板
            let (rect, resp) = ui
                .allocate_exact_size(egui::vec2(360.0, 150.0), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(246, 248, 252));
            painter.rect_stroke(
                rect,
                8.0,
                (
                    1.5,
                    if resp.dragged() {
                        ACCENT
                    } else {
                        egui::Color32::from_rgb(210, 216, 228)
                    },
                ),
            );
            if resp.dragged() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.pad_points.push(p - rect.min);
                }
            }
            if resp.drag_stopped() {
                let pts: Vec<(f32, f32)> = self.pad_points.iter().map(|v| (v.x, v.y)).collect();
                let turn = self.config.read().unwrap().turn_px;
                self.draft.pattern = gesture::sequence_from_f32(&pts, 25.0, turn);
            }
            if !self.pad_points.is_empty() {
                let stroke = egui::Stroke::new(3.0_f32, ACCENT);
                painter.add(egui::Shape::line(
                    self.pad_points.iter().map(|p| rect.min + *p).collect(),
                    stroke,
                ));
            }

            ui.add_space(6.0);
            // 保存前冲突提示：允许重复，但需二次确认
            if !self.pending_conflicts.is_empty() {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(255, 247, 230))
                    .rounding(egui::Rounding::same(8.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(230, 180, 90)))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("⚠ 检测到以下重复设置（允许保存，请确认）：")
                                .strong()
                                .color(egui::Color32::from_rgb(150, 96, 10)),
                        );
                        for w in &self.pending_conflicts {
                            ui.label(egui::RichText::new(format!("· {w}")).color(egui::Color32::from_rgb(150, 96, 10)));
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("返回修改").clicked() {
                                self.pending_conflicts.clear();
                            }
                            ui.label(
                                egui::RichText::new("确认无误就再点一次「仍然保存」")
                                    .small()
                                    .color(MUTED),
                            );
                        });
                    });
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                let save_label = if self.pending_conflicts.is_empty() { "保存" } else { "仍然保存" };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(save_label).strong().color(egui::Color32::WHITE),
                        )
                        .fill(if self.pending_conflicts.is_empty() {
                            ACCENT
                        } else {
                            egui::Color32::from_rgb(216, 138, 26)
                        })
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(egui::vec2(90.0, 32.0)),
                    )
                    .clicked()
                {
                    if self.draft.pattern.is_empty() {
                        self.status = "方向序列不能为空".into();
                    } else if self.draft.name.trim().is_empty() {
                        self.status = "名称不能为空".into();
                    } else if self.pending_conflicts.is_empty() {
                        // 第一次点：查重并展示提示（有重复则不落盘）
                        let conflicts = {
                            let cfg = self.config.read().unwrap();
                            cfg.conflict_warnings(&self.draft.clone(), self.edit_index)
                        };
                        if conflicts.is_empty() {
                            self.save_draft();
                        } else {
                            self.pending_conflicts = conflicts;
                        }
                    } else {
                        // 第二次点：用户已确认，允许重复保存
                        self.save_draft();
                    }
                }
                if ui
                    .add(
                        egui::Button::new("取消")
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(egui::vec2(80.0, 32.0)),
                    )
                    .clicked()
                {
                    self.edit_index = None;
                    self.creating = false;
                    self.draft = default_draft();
                    self.pad_points.clear();
                    self.pending_conflicts.clear();
                }
            });
        });
    }

    /// 把 draft 写入配置并复位编辑器状态
    fn save_draft(&mut self) {
        let mut cfg = self.config.write().unwrap();
        match self.edit_index {
            Some(i) if i < cfg.gestures.len() => cfg.gestures[i] = self.draft.clone(),
            _ => cfg.gestures.push(self.draft.clone()),
        }
        let _ = cfg.save();
        drop(cfg);
        self.edit_index = None;
        self.creating = false;
        self.draft = default_draft();
        self.pad_points.clear();
        self.pending_conflicts.clear();
        self.status = "已保存".into();
    }

    // ---------- 设置 / 测试 ----------

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("设置与测试").strong().size(22.0).color(INK));
        ui.add_space(4.0);

        card(ui, |ui| {
            ui.label(egui::RichText::new("触发与识别").strong().size(15.0).color(INK));
            ui.add_space(2.0);

            let (mut trigger, mut th, mut turn, mut fuzzy) = {
                let cfg = self.config.read().unwrap();
                (cfg.trigger, cfg.threshold_px, cfg.turn_px, cfg.fuzzy_tolerance)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("触发键").color(INK));
                if ui.radio(trigger == MouseButton::Right, "鼠标右键").clicked() {
                    trigger = MouseButton::Right;
                }
                if ui.radio(trigger == MouseButton::Middle, "鼠标中键").clicked() {
                    trigger = MouseButton::Middle;
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("起手势位移阈值: {:.0} px", th)).color(INK),
                );
                ui.add(egui::Slider::new(&mut th, 10.0..=100.0));
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("转折阈值: {:.0} px", turn)).color(INK),
                    );
                    ui.add(egui::Slider::new(&mut turn, 5.0..=60.0));
                });
                ui.label(
                    egui::RichText::new(
                        "越大越不容易把「画←时手向上漂」误拆成两段，但画多段手势时转折也要更果断。推荐 15~25。",
                    )
                    .small()
                    .color(MUTED),
                );
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("模糊匹配容差").color(INK));
                ui.add(egui::Slider::new(&mut fuzzy, 0..=2u32));
                ui.label(match fuzzy {
                    0 => "精确匹配".to_string(),
                    1 => "宽松（推荐）：允许歪 45°、多/少拐一段".to_string(),
                    _ => "最宽松：单方向手势也会近似命中，易误触".to_string(),
                });
            });
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("保存设置").strong().color(egui::Color32::WHITE),
                    )
                    .fill(ACCENT)
                    .rounding(egui::Rounding::same(8.0)),
                )
                .clicked()
            {
                let mut cfg = self.config.write().unwrap();
                cfg.trigger = trigger;
                cfg.threshold_px = th;
                cfg.turn_px = turn;
                cfg.fuzzy_tolerance = fuzzy;
                match cfg.save() {
                    Ok(_) => self.status = "设置已保存".into(),
                    Err(e) => self.status = format!("保存失败: {e}"),
                }
            }
        });

        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(egui::RichText::new("手势测试").strong().size(15.0).color(INK));
            ui.label(
                egui::RichText::new("在下方区域内按住左键拖动手势，松开即检测已启用的手势（含弦式以外的全部匹配逻辑）")
                    .small()
                    .color(MUTED),
            );
            ui.add_space(4.0);

            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(560.0), 200.0),
                egui::Sense::click_and_drag(),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 10.0, egui::Color32::from_rgb(246, 248, 252));
            painter.rect_stroke(
                rect,
                10.0,
                (
                    2.0,
                    if resp.dragged() {
                        ACCENT
                    } else {
                        egui::Color32::from_rgb(206, 214, 230)
                    },
                ),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "在此区域内按住左键拖出手势",
                egui::TextStyle::Heading.resolve(ui.style()),
                egui::Color32::from_rgb(198, 206, 220),
            );
            if resp.dragged() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.test_points.push(p - rect.min);
                }
            }
            if resp.drag_stopped() {
                let pts: Vec<(f32, f32)> = self.test_points.iter().map(|v| (v.x, v.y)).collect();
                let turn = self.config.read().unwrap().turn_px;
                let seq = gesture::sequence_from_f32(&pts, 25.0, turn);
                if seq.is_empty() {
                    self.test_result = Some(Err("位移太小，没识别出方向".into()));
                } else {
                    let hit: Option<(String, String, usize)> = {
                        let cfg = self.config.read().unwrap();
                        let (best, _) =
                            engine::match_gestures(&seq, &cfg.gestures, cfg.fuzzy_tolerance);
                        best.map(|(gd, d)| (gd.name.clone(), gesture::arrows(&gd.pattern), d))
                    };
                    self.test_result = Some(match hit {
                        Some((name, arrow, d)) => {
                            Ok(format!("✓ 测试成功：「{name}」{arrow}（差距 {d}）"))
                        }
                        None => Err(format!("✗ 未匹配，画出：{}", gesture::arrows(&seq))),
                    });
                }
            }
            if !self.test_points.is_empty() {
                let stroke = egui::Stroke::new(3.0_f32, ACCENT);
                painter.add(egui::Shape::line(
                    self.test_points.iter().map(|p| rect.min + *p).collect(),
                    stroke,
                ));
            }
            if let Some(res) = &self.test_result {
                ui.add_space(4.0);
                match res {
                    Ok(msg) => {
                        ui.label(egui::RichText::new(msg).strong().color(GOOD));
                    }
                    Err(msg) => {
                        ui.label(egui::RichText::new(msg).color(BAD));
                    }
                };
            }
        });
    }

    fn show_about(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("关于").strong().size(22.0).color(INK));
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("GestureCraft v0.5.0").strong().size(20.0).color(ACCENT),
            );
            ui.label(egui::RichText::new("原创 Rust 实现的 Windows 全局鼠标手势工具。").color(INK));
            ui.add_space(6.0);
            for line in [
                "· 一级 8 个单方向手势 + 弦式 2 个（方向+左键单击），斜向只作单手势",
                "· 二级 12 种两段联动全用满（镜像成对，全部单快捷键零参数）",
                "· 三级 36 种三段联动全量列出，默认绑 16 条、备选 20 条，可自由改绑",
                "· 田字格图示：米字格中心起笔，序号标注分段顺序",
                "· 转折阈值：画 ← 手微漂不再误拆成两段，大小可调",
                "· 作用域双栏：全局 / 程序专属手势（Edge 默认预载官方 16 手势）",
                "· 普通右键点击不受影响（延迟触发，无菜单异常）",
            ] {
                ui.label(egui::RichText::new(line).color(INK));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("功能概念参考了 WGestures 2 的产品形态，全部代码为原创实现。")
                    .small()
                    .color(MUTED),
            );
            ui.label(egui::RichText::new("License: MIT").small().color(MUTED));
        });
    }
}

fn make_action(kind_index: usize) -> ActionDef {
    match kind_index {
        0 => ActionDef::RunApp { path: String::new(), args: String::new() },
        1 => ActionDef::Command { cmd: String::new() },
        2 => ActionDef::Url { url: String::new() },
        3 => ActionDef::SendKeys { keys: String::new() },
        4 => ActionDef::Hotkey { combo: String::new() },
        5 => ActionDef::Window { op: "minimize".into() },
        6 => ActionDef::Volume { op: "up".into() },
        7 => ActionDef::System { op: "show_desktop".into() },
        8 => ActionDef::Browser { op: "back".into() },
        9 => ActionDef::PasteAndGo,
        _ => ActionDef::Script { code: String::new() },
    }
}
