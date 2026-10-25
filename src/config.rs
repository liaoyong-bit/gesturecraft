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
        "maximize" => "最大化",
        "restore" => "还原",
        "close" => "关闭",
        "topmost" => "置顶切换",
        "snap_left" => "左贴边",
        "snap_right" => "右贴边",
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
    /// 作用域：进程名关键字（如 msedge、notepad），空=全局。
    /// 命中时优先于全局手势；画在桌面/公共区域时直接用全局。
    #[serde(default)]
    pub scope: String,
}

impl GestureDef {
    pub fn is_global(&self) -> bool {
        self.scope.trim().is_empty()
    }

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
    /// 已添加的程序作用域（进程名关键字列表）
    #[serde(default = "default_programs")]
    pub programs: Vec<String>,
    /// 捕捉目标：活动窗口 / 鼠标所指窗口
    #[serde(default)]
    pub target_mode: TargetMode,
    pub gestures: Vec<GestureDef>,
    /// 配置结构版本；<5 时用新默认表整体替换（v0.4 → v0.5 默认表大改）
    #[serde(default)]
    pub schema: u32,
}

fn default_fuzzy() -> u32 {
    1
}
fn default_turn() -> f32 {
    20.0
}
fn default_programs() -> Vec<String> {
    vec!["msedge".into()]
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: MouseButton::Right,
            threshold_px: 30.0,
            turn_px: default_turn(),
            fuzzy_tolerance: default_fuzzy(),
            programs: default_programs(),
            target_mode: TargetMode::Focused,
            gestures: default_gestures(),
            schema: 5,
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
        scope: String::new(),
    }
}

fn gs(
    category: &str,
    name: &str,
    pattern: &str,
    enabled: bool,
    scope: &str,
    action: ActionDef,
) -> GestureDef {
    let mut gd = g(category, name, pattern, enabled, action);
    gd.scope = scope.into();
    gd
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
    v.push(g("窗口", "窗口左贴边", "4", true, Window { op: "snap_left".into() }));
    v.push(g("窗口", "窗口右贴边", "6", true, Window { op: "snap_right".into() }));
    v.push(g("窗口", "最大化", "9", true, Window { op: "maximize".into() }));
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

    // ---------- Edge 程序作用域：官方 16 手势完整复刻，默认启用 ----------
    let edge = [
        ("后退", "4", Browser { op: "back".into() }),
        ("前进", "6", Browser { op: "forward".into() }),
        ("滚动到页面顶部", "8", Hotkey { combo: "ctrl+home".into() }),
        ("滚动到页面底部", "2", Hotkey { combo: "ctrl+end".into() }),
        ("关闭当前标签页", "23", Hotkey { combo: "ctrl+w".into() }),
        ("恢复已关闭标签页", "21", Hotkey { combo: "ctrl+shift+t".into() }),
        ("切换到右侧标签页", "89", Hotkey { combo: "ctrl+tab".into() }),
        ("切换到左侧标签页", "87", Hotkey { combo: "ctrl+shift+tab".into() }),
        ("链接在新标签页打开", "69", Browser { op: "open_link_new_tab".into() }),
        ("链接在新窗口打开", "63", Browser { op: "open_link_new_window".into() }),
        ("刷新页面", "47", Browser { op: "refresh".into() }),
        ("停止加载页面", "41", Browser { op: "stop".into() }),
        ("新建标签页", "28", Hotkey { combo: "ctrl+t".into() }),
        ("复制当前标签页", "82", Hotkey { combo: "ctrl+shift+k".into() }),
        ("关闭右侧所有标签页", "64", Browser { op: "close_tabs_right".into() }),
        ("关闭其他标签页", "46", Browser { op: "close_other_tabs".into() }),
    ];
    for (name, pat, act) in edge {
        v.push(gs("浏览器", &format!("Edge·{name}"), pat, true, "msedge", act));
    }

    v
}

/// Office 程序专属预置（按编辑快捷键使用频率排序）。
/// 作用域内手势与全局重复属正常覆盖，不冲突。
/// app: "winword" | "excel" | "powerpnt"
pub fn office_presets(app: &str) -> Vec<GestureDef> {
    use ActionDef::*;
    let (label, scope) = match app {
        "winword" => ("Word", "winword"),
        "excel" => ("Excel", "excel"),
        "powerpnt" => ("PPT", "powerpnt"),
        _ => return Vec::new(),
    };
    let mut items: Vec<(&str, &str, ActionDef)> = vec![
        ("撤销", "7", Hotkey { combo: "ctrl+z".into() }),
        ("重做", "3", Hotkey { combo: "ctrl+y".into() }),
        ("保存", "82", Hotkey { combo: "ctrl+s".into() }),
        ("加粗", "8", Hotkey { combo: "ctrl+b".into() }),
        ("斜体", "2", Hotkey { combo: "ctrl+i".into() }),
        ("下划线", "62", Hotkey { combo: "ctrl+u".into() }),
        ("居中", "23", Hotkey { combo: "ctrl+e".into() }),
        ("左对齐", "21", Hotkey { combo: "ctrl+l".into() }),
        ("右对齐", "29", Hotkey { combo: "ctrl+r".into() }),
        ("查找", "87", Hotkey { combo: "ctrl+f".into() }),
        ("替换", "89", Hotkey { combo: "ctrl+h".into() }),
        ("打印", "43", Hotkey { combo: "ctrl+p".into() }),
    ];
    if app == "powerpnt" {
        items.push(("幻灯片放映", "842", Hotkey { combo: "f5".into() }));
        items.push(("从头放映（当前页）", "846", Hotkey { combo: "shift+f5".into() }));
    }
    items
        .into_iter()
        .map(|(name, pat, act)| gs("编辑", &format!("{label}·{name}"), pat, true, scope, act))
        .collect()
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
    /// 保存前的重复检测——查的就是 config.json 这份"小库"，实时反映当前全部手势。
    /// 允许重复，只产生提示文案；exclude = 编辑已有手势时跳过它自己。
    pub fn conflict_warnings(&self, cand: &GestureDef, exclude: Option<usize>) -> Vec<String> {
        let mut out = Vec::new();
        if cand.pattern.is_empty() {
            return out;
        }
        let cand_scopes: Vec<String> = cand
            .scope
            .to_ascii_lowercase()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        for (i, g) in self.gestures.iter().enumerate() {
            if Some(i) == exclude || !g.enabled || g.pattern.is_empty() {
                continue;
            }
            // 作用域是否重叠：任一方为全局即覆盖面重叠；否则看进程关键字交集
            let other_scopes: Vec<String> = g
                .scope
                .to_ascii_lowercase()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            // 程序手势覆盖全局是设计行为（程序内只执行程序设定），
            // 一全局一程序不算冲突；只有 全局↔全局 或 程序↔同程序 才提示
            let overlap = if cand_scopes.is_empty() {
                other_scopes.is_empty()
            } else if other_scopes.is_empty() {
                false
            } else {
                cand_scopes.iter().any(|a| other_scopes.contains(a))
            };
            if !overlap {
                continue;
            }
            if g.pattern == cand.pattern {
                let scope_desc = if other_scopes.is_empty() { "全局" } else { "程序作用域" };
                out.push(format!(
                    "方向序列 {} 已被「{}」（{scope_desc}）占用",
                    crate::gesture::arrows(&cand.pattern),
                    g.name
                ));
            }
            if g.action == cand.action {
                out.push(format!(
                    "动作「{}」与「{}」重复绑定",
                    cand.action.summary(),
                    g.name
                ));
            }
            if g.name.trim() == cand.name.trim() {
                out.push(format!("名称「{}」已存在", cand.name.trim()));
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
                    // v0.4 及更早配置（schema<5）：默认表整体换代，保留触发设置
                    if c.schema < 5 {
                        let (trigger, th, fuzzy) = (c.trigger, c.threshold_px, c.fuzzy_tolerance);
                        c = Config {
                            trigger,
                            threshold_px: th,
                            fuzzy_tolerance: fuzzy,
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
