use crate::config::{
    builtin_presets, browser_op_label, system_op_label, window_op_label, ActionDef, Config,
    GestureDef, MouseButton, CATEGORIES,
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

fn primary_button(text: &str) -> egui::Button {
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
    /// 分类筛选（"全部" 或分类名）
    filter: String,
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
            .with_inner_size([1020.0, 760.0])
            .with_min_inner_size([900.0, 660.0])
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
                filter: "全部".into(),
                edit_index: None,
                creating: false,
                draft,
                pad_points: Vec::new(),
                test_points: Vec::new(),
                test_result: None,
                status: String::new(),
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

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(PAGE_BG).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::Gestures => self.show_gestures(ui),
                    Tab::Settings => self.show_settings(ui),
                    Tab::About => self.show_about(ui),
                });
            });

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
                    ui.label(egui::RichText::new("v0.4.0").small().color(NAV_TEXT_DIM));
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

    // ---------- 手势库 ----------

    fn show_gestures(&mut self, ui: &mut egui::Ui) {
        // 标题行：左侧标题，右侧新建按钮
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("手势库").strong().size(22.0).color(INK));
                ui.label(egui::RichText::new("管理手势与动作；支持按分类筛选").small().color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button("＋ 新建手势")).clicked() {
                    self.edit_index = None;
                    self.creating = true;
                    self.draft = default_draft();
                    self.pad_points.clear();
                }
            });
        });
        ui.add_space(6.0);

        // 分类标签页（chips）
        let counts: Vec<usize> = {
            let cfg = self.config.read().unwrap();
            CATEGORIES
                .iter()
                .map(|c| cfg.gestures.iter().filter(|g| g.category == *c).count())
                .collect()
        };
        let total = counts.iter().sum::<usize>();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(egui::SelectableLabel::new(
                    self.filter == "全部",
                    egui::RichText::new(format!("全部 {total}")).size(13.0),
                ))
                .clicked()
            {
                self.filter = "全部".into();
            }
            for (i, cat) in CATEGORIES.iter().enumerate() {
                let (_, fg) = cat_color(cat);
                let label = egui::RichText::new(format!("{cat} {}", counts[i]))
                    .size(13.0)
                    .color(fg);
                if ui
                    .add(egui::SelectableLabel::new(self.filter == *cat, label))
                    .clicked()
                {
                    self.filter = cat.to_string();
                }
            }
        });
        ui.add_space(4.0);

        // 已设置手势（卡片列表）
        let snapshot: Vec<GestureDef> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures.clone()
        };
        let visible: Vec<(usize, GestureDef)> = snapshot
            .into_iter()
            .enumerate()
            .filter(|(_, g)| self.filter == "全部" || g.category == self.filter)
            .collect();
        if visible.is_empty() {
            card(ui, |ui| {
                ui.label(egui::RichText::new(
                    "这里还没有手势。从下方内置手势库一键添加，或点右上角「＋ 新建手势」。",
                ).color(MUTED));
            });
        } else {
            for (i, gd) in visible {
                let name = gd.name.clone();
                let arrow = gesture::arrows(&gd.pattern);
                let summary = gd.action.summary();
                let cat = gd.category.clone();
                let scope = gd.scope.clone();
                let mut en = gd.enabled;
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&name).strong().size(16.0).color(INK),
                                );
                                if !cat.is_empty() {
                                    cat_badge(ui, &cat);
                                }
                                if !scope.trim().is_empty() {
                                    badge(
                                        ui,
                                        format!("作用域·{scope}").as_str(),
                                        egui::Color32::from_rgb(232, 238, 250),
                                        ACCENT_DEEP,
                                    );
                                }
                                if !gd.enabled {
                                    ui.label(egui::RichText::new("已停用").small().color(MUTED));
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&arrow).strong().size(16.0).color(ACCENT),
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
        }

        if !self.status.is_empty() {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(&self.status).small().color(GOOD));
        }

        // ---------- 内置手势库 ----------
        ui.add_space(10.0);
        ui.label(egui::RichText::new("内置手势库").strong().size(18.0).color(INK));
        ui.label(
            egui::RichText::new("参照 WGestures / MouseInc / Edge 的常见编排，点击即添加，可在编辑器里微调方向、分类与动作")
                .small()
                .color(MUTED),
        );
        ui.add_space(4.0);

        let presets = builtin_presets();
        let existing_names: Vec<String> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures.iter().map(|gd| gd.name.clone()).collect()
        };
        let mut to_add: Option<GestureDef> = None;
        card(ui, |ui| {
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for cat in CATEGORIES {
                    let items: Vec<&GestureDef> =
                        presets.iter().filter(|p| p.category == cat).collect();
                    if items.is_empty() {
                        continue;
                    }
                    if self.filter != "全部" && self.filter != cat {
                        continue;
                    }
                    let (bg, fg) = cat_color(cat);
                    ui.add_space(2.0);
                    egui::Frame::none()
                        .fill(bg)
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(cat).strong().small().color(fg));
                        });
                    ui.add_space(2.0);
                    for p in items {
                        let already = existing_names.iter().any(|n| *n == p.name);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&p.name).color(INK));
                            ui.label(
                                egui::RichText::new(gesture::arrows(&p.pattern))
                                    .strong()
                                    .color(ACCENT),
                            );
                            ui.label(egui::RichText::new(p.action.summary()).small().color(MUTED));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if already {
                                        ui.label(egui::RichText::new("已添加").small().color(MUTED));
                                    } else if ui.small_button("＋ 添加").clicked() {
                                        to_add = Some(p.clone());
                                    }
                                },
                            );
                        });
                        ui.separator();
                    }
                }
            });
        });
        if let Some(p) = to_add {
            self.config.write().unwrap().gestures.push(p.clone());
            let _ = self.config.read().unwrap().save();
            self.status = format!("已添加: {}", p.name);
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
                        "音量", "系统", "浏览器", "粘贴并转到", "脚本",
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
                                    .hint_text("如 ctrl+shift+t、alt+tab、win+d"),
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
                                        ("snap_left", "左半屏"),
                                        ("snap_right", "右半屏"),
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
                    ui.label(
                        egui::RichText::new(gesture::arrows(&self.draft.pattern))
                            .strong()
                            .size(20.0)
                            .color(ACCENT),
                    );
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
                self.draft.pattern = gesture::sequence_from_f32(&pts, 25.0);
            }
            if !self.pad_points.is_empty() {
                let stroke = egui::Stroke::new(3.0_f32, ACCENT);
                painter.add(egui::Shape::line(
                    self.pad_points.iter().map(|p| rect.min + *p).collect(),
                    stroke,
                ));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("保存").strong().color(egui::Color32::WHITE),
                        )
                        .fill(ACCENT)
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(egui::vec2(90.0, 32.0)),
                    )
                    .clicked()
                {
                    if self.draft.pattern.is_empty() {
                        self.status = "方向序列不能为空".into();
                    } else if self.draft.name.trim().is_empty() {
                        self.status = "名称不能为空".into();
                    } else {
                        let mut cfg = self.config.write().unwrap();
                        match self.edit_index {
                            Some(i) if i < cfg.gestures.len() => {
                                cfg.gestures[i] = self.draft.clone()
                            }
                            _ => cfg.gestures.push(self.draft.clone()),
                        }
                        let _ = cfg.save();
                        drop(cfg);
                        self.edit_index = None;
                        self.creating = false;
                        self.draft = default_draft();
                        self.pad_points.clear();
                        self.status = "已保存".into();
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
                }
            });
        });
    }

    // ---------- 设置 / 测试 ----------

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("设置与测试").strong().size(22.0).color(INK));
        ui.add_space(4.0);

        card(ui, |ui| {
            ui.label(egui::RichText::new("触发与识别").strong().size(15.0).color(INK));
            ui.add_space(2.0);

            let (mut trigger, mut th, mut fuzzy) = {
                let cfg = self.config.read().unwrap();
                (cfg.trigger, cfg.threshold_px, cfg.fuzzy_tolerance)
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
                let seq = gesture::sequence_from_f32(&pts, 25.0);
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
                egui::RichText::new("GestureCraft v0.4.0").strong().size(20.0).color(ACCENT),
            );
            ui.label(egui::RichText::new("原创 Rust 实现的 Windows 全局鼠标手势工具。").color(INK));
            ui.add_space(6.0);
            for line in [
                "· 8 方向手势（↑↓←→↖↗↙↘），连贯一笔自动拆段",
                "· 模糊匹配：歪一点、多拐少拐一档也能命中，容差可调",
                "· 弦式手势：按住右键拖方向 + 单击左键（如 下拖+左点=粘贴并转到）",
                "· Edge 浏览器 16 个默认手势全部收录内置手势库",
                "· 分类标签：编辑 / 导航 / 窗口 / 浏览器 / 系统 / 应用",
                "· 进程作用域：可给某个程序配置专属手势，桌面自动走全局",
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
