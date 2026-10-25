use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 右侧面板的等级页签。等级由 pattern 段数推导（弦式含 'L'）。
pub const LEVELS: [&str; 4] = ["一级", "二级", "三级", "弦式"];

/// 手势等级归类（面板筛选用）
pub fn level_of(pattern: &str) -> &'static str {
    if pattern.contains('L') {
        return "弦式";
    }
    match pattern.chars().count() {
        1 => "一级",
        2 => "二级",
        _ => "三级",
    }
}

/// 旧分类（保留用于徽章配色）
pub const CATEGORIES: [&str; 7] = ["编辑", "导航", "窗口", "浏览器", "系统", "应用", "其他"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Right,
    Middle,
}

/// 捕捉目标：手势作用域按哪个窗口判定
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetMode {
    /// 前台活动窗口（键盘焦点所在）
    #[default]
    Focused,
    /// 鼠标所指窗口（手势终点落点）
    UnderCursor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDef {
    RunApp { path: String, args: String },
    Command { cmd: String },
    Url { url: String },
    SendKeys { keys: String },
    /// 按键组合，写法如 "ctrl+shift+t"、"alt+tab"、"win+d"
    Hotkey { combo: String },
    /// 窗口：minimize | maximize | restore | close | topmost | snap_left | snap_right
    Window { op: String },
    /// 音量：up | down | mute
    Volume { op: String },
    /// 系统：show_desktop | task_view | clipboard_history | lock
    System { op: String },
    /// 浏览器：back | forward | refresh | stop | open_link_new_tab | open_link_new_window
    /// | close_tabs_right | close_other_tabs
    Browser { op: String },
    /// 粘贴并回车：浏览器里定位地址栏→粘贴→回车；其他程序=普通粘贴
    PasteAndGo,
    Script { code: String },
}

impl ActionDef {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ActionDef::RunApp { .. } => "运行程序",
            ActionDef::Command { .. } => "命令行",
            ActionDef::Url { .. } => "打开网址",
            ActionDef::SendKeys { .. } => "发送按键",
            ActionDef::Hotkey { .. } => "按键组合",
            ActionDef::Window { .. } => "窗口控制",
            ActionDef::Volume { .. } => "音量",
            ActionDef::System { .. } => "系统",
            ActionDef::Browser { .. } => "浏览器",
            ActionDef::PasteAndGo => "粘贴并回车",
            ActionDef::Script { .. } => "脚本",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ActionDef::RunApp { path, args } => {
                if args.is_empty() { path.clone() } else { format!("{path} {args}") }
            }
            ActionDef::Command { cmd } => cmd.clone(),
            ActionDef::Url { url } => url.clone(),
            ActionDef::SendKeys { keys } => format!("发送 {keys}"),
            ActionDef::Hotkey { combo } => {
                if combo.is_empty() { "（未绑定）".into() } else { combo.clone() }
            }
            ActionDef::Window { op } => format!("窗口·{}", window_op_label(op)),
            ActionDef::Volume { op } => format!("音量·{op}"),
            ActionDef::System { op } => format!("系统·{}", system_op_label(op)),
            ActionDef::Browser { op } => format!("浏览器·{}", browser_op_label(op)),
            ActionDef::PasteAndGo => "浏览器地址栏粘贴回车，其余粘贴".into(),
            ActionDef::Script { code } => code.chars().take(20).collect(),
        }
    }
}

pub fn window_op_label(op: &str) -> &str {
    match op {
        "minimize" => "最小化",
        "maximize" => "最大化/还原",
        "restore" => "还原窗口",
        "close" => "关闭",
        "topmost" => "置顶切换",
        "snap_left" => "左贴边/还原",
        "snap_right" => "右贴边/还原",
        other => other,
    }
}

pub fn system_op_label(op: &str) -> &str {
    match op {
        "show_desktop" => "显示桌面",
        "task_view" => "任务视图",
        "clipboard_history" => "剪贴板历史",
        "lock" => "锁屏",
        other => other,
    }
}

pub fn browser_op_label(op: &str) -> &str {
    match op {
        "back" => "后退",
        "forward" => "前进",
        "refresh" => "刷新",
        "stop" => "停止加载",
        "open_link_new_tab" => "链接新标签打开",
        "open_link_new_window" => "链接新窗口打开",
        "close_tabs_right" => "关闭右侧标签",
        "close_other_tabs" => "关闭其他标签",
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureDef {
    pub name: String,
    /// 方向序列，字符取自 8 方向小键盘编码：8↑ 2↓ 4← 6→ 7↖ 9↗ 1↙ 3↘；
    /// 末尾可带 'L' 表示弦式（按住触发键期间单击左键）
    pub pattern: String,
    pub enabled: bool,
    pub action: ActionDef,
    /// 分类标签（徽章配色用）
    #[serde(default)]
    pub category: String,
}

impl GestureDef {
    pub fn level(&self) -> &'static str {
        level_of(&self.pattern)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub trigger: MouseButton,
    pub threshold_px: f32,
    /// 转折阈值（滞回）：新方向投影需超出当前方向投影这么多才算真转折。
    /// 越大越不容易把"左+轻微上漂"拆成两段，越大越难画出多段手势。
    #[serde(default = "default_turn")]
    pub turn_px: f32,
    /// 模糊匹配容差：允许与录入序列的最大编辑距离（0=精确）
    #[serde(default = "default_fuzzy")]
    pub fuzzy_tolerance: u32,
    /// 捕捉目标：活动窗口 / 鼠标所指窗口
    #[serde(default)]
    pub target_mode: TargetMode,
    pub gestures: Vec<GestureDef>,
    /// 配置结构版本；<6 时手势表整体换代（v0.5.3 起取消程序作用域，全部全局）
    #[serde(default)]
    pub schema: u32,
}

fn default_fuzzy() -> u32 {
    1
}
fn default_turn() -> f32 {
    20.0
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: MouseButton::Right,
            threshold_px: 30.0,
            turn_px: default_turn(),
            fuzzy_tolerance: default_fuzzy(),
            target_mode: TargetMode::Focused,
            gestures: default_gestures(),
            schema: 6,
        }
    }
}

fn g(category: &str, name: &str, pattern: &str, enabled: bool, action: ActionDef) -> GestureDef {
    GestureDef {
        name: name.into(),
        pattern: pattern.into(),
        enabled,
        action,
        category: category.into(),
    }
}

/// v0.5 默认手势表（用户已确认的分配方案）。
/// 一级 8（斜向只作单手势）+ 弦式 2 + 二级 12（上下左右两段全用满，镜像成对，
/// 全部单快捷键零参数）+ 三级 16 启用 + 20 备选（相邻段不重复的 36 种全量）。
pub fn default_gestures() -> Vec<GestureDef> {
    use ActionDef::*;
    let mut v = Vec::new();

    // ---------- 一级：8 个单方向，最常用 ----------
    v.push(g("编辑", "复制", "8", true, Hotkey { combo: "ctrl+c".into() }));
    v.push(g("编辑", "粘贴", "2", true, Hotkey { combo: "ctrl+v".into() }));
    v.push(g("窗口", "窗口左贴边/还原", "4", true, Window { op: "snap_left".into() }));
    v.push(g("窗口", "窗口右贴边/还原", "6", true, Window { op: "snap_right".into() }));
    v.push(g("窗口", "最大化/还原", "9", true, Window { op: "maximize".into() }));
    v.push(g("窗口", "最小化", "1", true, Window { op: "minimize".into() }));
    v.push(g("编辑", "撤销", "7", true, Hotkey { combo: "ctrl+z".into() }));
    v.push(g("编辑", "重做", "3", true, Hotkey { combo: "ctrl+y".into() }));

    // ---------- 弦式：方向 + 左键单击 ----------
    v.push(g("编辑", "剪切（弦式 ↑+左点）", "8L", true, Hotkey { combo: "ctrl+x".into() }));
    v.push(g("编辑", "粘贴并回车（弦式 ↓+左点）", "2L", true, PasteAndGo));

    // ---------- 二级：上下左右两段 12 种全用满，镜像成对 ----------
    v.push(g("导航", "切换左侧标签页", "87", true, Hotkey { combo: "ctrl+shift+tab".into() }));
    v.push(g("导航", "切换右侧标签页", "89", true, Hotkey { combo: "ctrl+tab".into() }));
    v.push(g("导航", "恢复已关闭标签页", "21", true, Hotkey { combo: "ctrl+shift+t".into() }));
    v.push(g("导航", "关闭当前标签页", "23", true, Hotkey { combo: "ctrl+w".into() }));
    v.push(g("导航", "刷新", "47", true, Browser { op: "refresh".into() }));
    v.push(g("导航", "强制刷新", "69", true, Hotkey { combo: "ctrl+f5".into() }));
    v.push(g("窗口", "关闭窗口", "43", true, Window { op: "close".into() }));
    v.push(g("窗口", "新建窗口", "63", true, Hotkey { combo: "ctrl+n".into() }));
    v.push(g("系统", "任务视图", "46", true, System { op: "task_view".into() }));
    v.push(g("系统", "显示桌面", "64", true, System { op: "show_desktop".into() }));
    v.push(g("编辑", "保存", "82", true, Hotkey { combo: "ctrl+s".into() }));
    v.push(g("编辑", "全选", "28", true, Hotkey { combo: "ctrl+a".into() }));

    // ---------- 三级：36 种中默认绑定 16 条（单快捷键零参数、尽量成对） ----------
    let three: [(&str, &str, &str, bool, ActionDef); 16] = [
        ("导航", "新建标签页", "286", true, Hotkey { combo: "ctrl+t".into() }),
        ("编辑", "查找", "824", true, Hotkey { combo: "ctrl+f".into() }),
        ("系统", "剪贴板历史", "826", true, System { op: "clipboard_history".into() }),
        ("系统", "截图", "284", true, Hotkey { combo: "win+shift+s".into() }),
        ("系统", "任务管理器", "482", true, Hotkey { combo: "ctrl+shift+esc".into() }),
        ("应用", "资源管理器", "648", true, RunApp { path: "explorer.exe".into(), args: String::new() }),
        ("系统", "锁屏", "642", true, System { op: "lock".into() }),
        ("系统", "打开设置", "464", true, Hotkey { combo: "win+i".into() }),
        ("系统", "上一个虚拟桌面", "242", true, Hotkey { combo: "win+ctrl+left".into() }),
        ("系统", "下一个虚拟桌面", "262", true, Hotkey { combo: "win+ctrl+right".into() }),
        ("窗口", "窗口置顶切换", "282", true, Window { op: "topmost".into() }),
        ("系统", "音量静音", "828", true, Volume { op: "mute".into() }),
        ("导航", "停止加载", "268", true, Hotkey { combo: "esc".into() }),
        ("窗口", "最小化所有窗口", "848", true, Hotkey { combo: "win+m".into() }),
        ("窗口", "还原所有窗口", "868", true, Hotkey { combo: "win+shift+m".into() }),
        ("导航", "定位地址栏", "628", true, Hotkey { combo: "ctrl+l".into() }),
    ];
    for (cat, name, pat, en, act) in three {
        v.push(g(cat, name, pat, en, act));
    }

    // ---------- 三级备选 20 条：默认停用，可在面板中自由绑定 ----------
    // （36 种 = 上下左右三段、相邻段不重复；16 条已绑定，其余为备选）
    let spare = [
        "842", "846", "862", "864", "248", "246", "264", "484", "486", "468", "462", "424",
        "428", "423", "686", "684", "682", "646", "626", "624",
    ];
    for pat in spare {
        v.push(g("其他", &format!("三级备选 {pat}"), pat, false, Hotkey { combo: String::new() }));
    }

    v
}

/// 旧版 UDLR 序列 → 8 方向数字编码，并折叠相邻重复（旧版 "DD" 不可达，归一为 "2"）。
/// 仅当模式完全由旧编码字母组成时才迁移；新式模式（含 'L' 弦式标记）原样保留。
fn migrate_pattern(p: &str) -> String {
    let is_legacy = !p.is_empty()
        && p.chars().all(|c| matches!(c, 'U' | 'u' | 'D' | 'd' | 'L' | 'l' | 'R' | 'r'));
    if !is_legacy {
        return p.to_string();
    }
    let mut out = String::new();
    for c in p.chars() {
        let ch = match c {
            'U' | 'u' => '8',
            'D' | 'd' => '2',
            'L' | 'l' => '4',
            'R' | 'r' => '6',
            other => other,
        };
        if out.chars().last() != Some(ch) {
            out.push(ch);
        }
    }
    out
}

impl Config {
    /// 保存前冲突检查——重复即报错、不允许保存（全部全局，不存在作用域覆盖）。
    /// 只与"已启用"的手势比（备选/停用不算占用）；exclude = 编辑已有手势时跳过它自己。
    /// 返回错误文案列表；非空则调用方必须阻止保存。
    pub fn blocking_conflicts(&self, cand: &GestureDef, exclude: Option<usize>) -> Vec<String> {
        let mut out = Vec::new();
        for (i, g) in self.gestures.iter().enumerate() {
            if Some(i) == exclude || !g.enabled {
                continue;
            }
            if !cand.pattern.is_empty() && g.pattern == cand.pattern {
                out.push(format!(
                    "方向序列 {} 已被「{}」占用，请换一个手势或先停用/删除它",
                    crate::gesture::arrows(&cand.pattern),
                    g.name
                ));
            }
            if !cand.name.trim().is_empty() && g.name.trim() == cand.name.trim() {
                out.push(format!("名称「{}」已存在，请改名", cand.name.trim()));
            }
        }
        out.dedup();
        out
    }

    fn path() -> PathBuf {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("GestureCraft").join("config.json")
    }

    pub fn load_or_default() -> Config {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => match serde_json::from_str::<Config>(&s) {
                Ok(mut c) => {
                    for gd in &mut c.gestures {
                        gd.pattern = migrate_pattern(&gd.pattern);
                    }
                    // 旧配置（schema<6，含程序作用域/程序预设）：手势表整体换代，保留触发设置
                    if c.schema < 6 {
                        let (trigger, th, turn, fuzzy, tm) = (
                            c.trigger,
                            c.threshold_px,
                            c.turn_px,
                            c.fuzzy_tolerance,
                            c.target_mode,
                        );
                        c = Config {
                            trigger,
                            threshold_px: th,
                            turn_px: turn,
                            fuzzy_tolerance: fuzzy,
                            target_mode: tm,
                            ..Config::default()
                        };
                        let _ = c.save();
                    }
                    c
                }
                Err(e) => {
                    eprintln!("配置解析失败，使用默认配置: {e}");
                    Config::default()
                }
            },
            Err(_) => {
                let c = Config::default();
                let _ = c.save();
                c
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, serde_json::to_string_pretty(self).unwrap())
    }
}
