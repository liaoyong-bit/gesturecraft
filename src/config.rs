use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 手势分类（标签页筛选用）
pub const CATEGORIES: [&str; 7] = ["编辑", "导航", "窗口", "浏览器", "系统", "应用", "其他"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Right,
    Middle,
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
    /// 粘贴并转到：浏览器里定位地址栏→粘贴→回车；其他程序=普通粘贴
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
            ActionDef::PasteAndGo => "粘贴并转到",
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
            ActionDef::Hotkey { combo } => combo.clone(),
            ActionDef::Window { op } => format!("窗口·{}", window_op_label(op)),
            ActionDef::Volume { op } => format!("音量·{op}"),
            ActionDef::System { op } => format!("系统·{}", system_op_label(op)),
            ActionDef::Browser { op } => format!("浏览器·{}", browser_op_label(op)),
            ActionDef::PasteAndGo => "定位地址栏粘贴并跳转".into(),
            ActionDef::Script { code } => code.chars().take(20).collect(),
        }
    }
}

pub fn window_op_label(op: &str) -> &str {
    match op {
        "minimize" => "最小化",
        "maximize" => "最大化",
        "restore" => "还原",
        "close" => "关闭",
        "topmost" => "置顶切换",
        "snap_left" => "左半屏",
        "snap_right" => "右半屏",
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
    /// 分类标签（编辑/导航/窗口/浏览器/系统/应用/其他）
    #[serde(default)]
    pub category: String,
    /// 作用域：进程名关键字（如 msedge、notepad），空=全局。
    /// 命中时优先于全局手势；画在桌面/公共区域时直接用全局。
    #[serde(default)]
    pub scope: String,
}

impl GestureDef {
    pub fn is_global(&self) -> bool {
        self.scope.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub trigger: MouseButton,
    pub threshold_px: f32,
    /// 模糊匹配容差：允许与录入序列的最大编辑距离（0=精确）
    #[serde(default = "default_fuzzy")]
    pub fuzzy_tolerance: u32,
    pub gestures: Vec<GestureDef>,
}

fn default_fuzzy() -> u32 {
    1
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: MouseButton::Right,
            threshold_px: 30.0,
            fuzzy_tolerance: default_fuzzy(),
            gestures: Vec::new(),
        }
    }
}

/// 内置手势库。编辑类常用项排最前，Edge 浏览器 16 手势全部收录；
/// 一键添加，用户可在编辑器里对方向、分类和动作做微调。
pub fn builtin_presets() -> Vec<GestureDef> {
    use ActionDef::*;
    vec![
        // ---------- 编辑 ----------
        g("编辑", "复制", "68", Hotkey { combo: "ctrl+c".into() }),
        g("编辑", "粘贴", "62", Hotkey { combo: "ctrl+v".into() }),
        g("编辑", "剪切", "86", Hotkey { combo: "ctrl+x".into() }),
        g("编辑", "撤销", "29", Hotkey { combo: "ctrl+z".into() }),
        // 弦式：按住右键拖方向后单击左键
        g("编辑", "粘贴并转到（弦式）", "2L", PasteAndGo),
        g("编辑", "剪切（弦式）", "8L", Hotkey { combo: "ctrl+x".into() }),
        // ---------- 导航（Edge 单方向手势） ----------
        g("导航", "后退", "4", Browser { op: "back".into() }),
        g("导航", "前进", "6", Browser { op: "forward".into() }),
        g("导航", "滚动到顶部", "8", Hotkey { combo: "ctrl+home".into() }),
        g("导航", "滚动到底部", "2", Hotkey { combo: "ctrl+end".into() }),
        // ---------- 窗口 ----------
        g("窗口", "最小化前台窗口", "2", Window { op: "minimize".into() }),
        g("窗口", "最大化前台窗口", "8", Window { op: "maximize".into() }),
        g("窗口", "关闭前台窗口", "42", Window { op: "close".into() }),
        g("窗口", "窗口置顶切换", "48", Window { op: "topmost".into() }),
        g("窗口", "左半屏贴靠", "4", Window { op: "snap_left".into() }),
        g("窗口", "右半屏贴靠", "6", Window { op: "snap_right".into() }),
        g("窗口", "切换窗口", "242", Hotkey { combo: "alt+tab".into() }),
        // ---------- 浏览器（Edge 16 手势完整复刻） ----------
        g("浏览器", "关闭当前标签页", "23", Hotkey { combo: "ctrl+w".into() }),
        g("浏览器", "恢复已关闭标签页", "21", Hotkey { combo: "ctrl+shift+t".into() }),
        g("浏览器", "切换到右侧标签页", "89", Hotkey { combo: "ctrl+tab".into() }),
        g("浏览器", "切换到左侧标签页", "87", Hotkey { combo: "ctrl+shift+tab".into() }),
        g("浏览器", "链接在新标签页打开", "69", Browser { op: "open_link_new_tab".into() }),
        g("浏览器", "链接在新窗口打开", "63", Browser { op: "open_link_new_window".into() }),
        g("浏览器", "刷新页面", "47", Browser { op: "refresh".into() }),
        g("浏览器", "停止加载页面", "41", Browser { op: "stop".into() }),
        g("浏览器", "新建标签页", "28", Hotkey { combo: "ctrl+t".into() }),
        g("浏览器", "复制当前标签页", "82", Hotkey { combo: "ctrl+shift+k".into() }),
        g("浏览器", "关闭右侧所有标签页", "64", Browser { op: "close_tabs_right".into() }),
        g("浏览器", "关闭其他标签页", "46", Browser { op: "close_other_tabs".into() }),
        g("浏览器", "关闭标签页", "262", Hotkey { combo: "ctrl+w".into() }),
        g("浏览器", "恢复关闭的标签", "232", Hotkey { combo: "ctrl+shift+t".into() }),
        g("浏览器", "浏览器刷新", "636", Browser { op: "refresh".into() }),
        // ---------- 系统 ----------
        g("系统", "显示桌面", "828", System { op: "show_desktop".into() }),
        g("系统", "任务视图", "898", System { op: "task_view".into() }),
        g("系统", "剪贴板历史", "878", System { op: "clipboard_history".into() }),
        g("系统", "锁屏", "212", System { op: "lock".into() }),
        g("系统", "音量静音", "121", Volume { op: "mute".into() }),
        // ---------- 应用 ----------
        g("应用", "打开资源管理器", "46", RunApp { path: "explorer.exe".into(), args: String::new() }),
        g("应用", "打开记事本", "63", RunApp { path: "notepad.exe".into(), args: String::new() }),
        g("应用", "打开百度", "82", Url { url: "https://www.baidu.com".into() }),
    ]
}

fn g(category: &str, name: &str, pattern: &str, action: ActionDef) -> GestureDef {
    GestureDef {
        name: name.into(),
        pattern: pattern.into(),
        enabled: true,
        action,
        category: category.into(),
        scope: String::new(),
    }
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
