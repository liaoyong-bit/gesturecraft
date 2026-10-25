/// 8 方向手势：小键盘方位编码，直观且可组合。
/// 8=↑ 2=↓ 4=← 6=→ 7=↖ 9=↗ 1=↙ 3=↘
/// 连贯拆分：锚点分段——轨迹每累计 min_seg 净位移就判一次主导方向（45° 扇区），
/// 方向变了即记新符号，无需抬手。组合空间：1 段 8 种、2 段 64 种、3 段 512 种。
///
/// 弦式手势（chord）：'L' 表示"按住触发键期间单击左键"，
/// 模式如 "2L" = 向下拖 + 左键单击。含 'L' 的模式只做精确匹配，避免误触。

/// 顺时针方位环：→ ↘ ↓ ↙ ← ↖ ↑ ↗（模糊匹配的角度距离按环上档位计）
const RING: [char; 8] = ['6', '3', '2', '1', '4', '7', '8', '9'];

pub fn arrow_for(c: char) -> &'static str {
    match c {
        '8' => "↑",
        '2' => "↓",
        '4' => "←",
        '6' => "→",
        '7' => "↖",
        '9' => "↗",
        '1' => "↙",
        '3' => "↘",
        'L' => "左点",
        _ => "?",
    }
}

pub fn arrows(pattern: &str) -> String {
    pattern.chars().map(arrow_for).collect()
}

pub fn is_chord(pattern: &str) -> bool {
    pattern.contains('L')
}

/// 屏幕坐标（y 向下）。角度以正右方为 0、顺时针为正，45° 一扇区。
fn sector_of(dx: f32, dy: f32) -> char {
    let mut ang = dy.atan2(dx).to_degrees();
    if ang < 0.0 {
        ang += 360.0;
    }
    match ((ang + 22.5) / 45.0) as i32 % 8 {
        0 => '6',
        1 => '3',
        2 => '2',
        3 => '1',
        4 => '4',
        5 => '7',
        6 => '8',
        _ => '9',
    }
}

/// 方向字符 → 单位向量（屏幕坐标，y 向下）。田字格图示与转折判定共用。
pub fn axis_of(c: char) -> (f32, f32) {
    let k = std::f32::consts::FRAC_1_SQRT_2;
    match c {
        '8' => (0.0, -1.0),
        '2' => (0.0, 1.0),
        '4' => (-1.0, 0.0),
        '6' => (1.0, 0.0),
        '7' => (-k, -k),
        '9' => (k, -k),
        '1' => (-k, k),
        '3' => (k, k),
        _ => (0.0, 0.0),
    }
}

/// 轨迹 → 方向序列（带转折滞回）。
/// 当前方向 c 已定格后，仅当"新方向轴上的投影比当前方向轴上的投影多出 turn_px 以上"
/// 才认为真的转折、记新段并移动锚点；否则锚点原地不动继续累计，
/// 让轻微漂移（画 ← 时手向上带一点）始终留在原段里。
pub fn sequence_from_f32(points: &[(f32, f32)], min_seg: f32, turn_px: f32) -> String {
    let mut seq = String::new();
    if points.len() < 2 {
        return seq;
    }
    let mut anchor = points[0];
    let mut cur: Option<char> = None;
    for &p in &points[1..] {
        let dx = p.0 - anchor.0;
        let dy = p.1 - anchor.1;
        if dx.hypot(dy) < min_seg {
            continue;
        }
        let ch = sector_of(dx, dy);
        match cur {
            None => {
                seq.push(ch);
                cur = Some(ch);
                anchor = p;
            }
            Some(c) if ch == c => {
                anchor = p; // 同向继续：锚点跟着走，便于尽快发现下一次转折
            }
            Some(c) => {
                let (cx, cy) = axis_of(c);
                let (nx, ny) = axis_of(ch);
                let proj_cur = dx * cx + dy * cy;
                let proj_new = dx * nx + dy * ny;
                if proj_new - proj_cur >= turn_px && proj_new >= turn_px {
                    seq.push(ch);
                    cur = Some(ch);
                    anchor = p;
                }
                // 未达转折阈值：锚点不动，位移继续累计直到果断转向
            }
        }
    }
    seq
}

pub fn sequence_from_i32(points: &[(i32, i32)], min_seg: f32, turn_px: f32) -> String {
    let f: Vec<(f32, f32)> = points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
    sequence_from_f32(&f, min_seg, turn_px)
}

fn ring_index(c: char) -> Option<usize> {
    RING.iter().position(|&r| r == c)
}

/// 两方向在方位环上的档位距离：相邻 45° = 1，相对 180° = 4。
/// 'L'（左键单击段）与任何方向都视为 4（实质不可替换），与自身为 0。
fn dir_dist(a: char, b: char) -> usize {
    if a == 'L' || b == 'L' {
        return if a == b { 0 } else { 4 };
    }
    match (ring_index(a), ring_index(b)) {
        (Some(ia), Some(ib)) => {
            let d = ia.abs_diff(ib);
            d.min(8 - d)
        }
        _ => 4,
    }
}

/// 带权编辑距离：替换代价 = 角度档位距离，插入/删除 = 1。
/// 人手画得不标准（歪一点、多拐一下、少拐一下）都能算出"差多少"。
pub fn fuzzy_distance(seq: &str, pattern: &str) -> usize {
    let a: Vec<char> = seq.chars().collect();
    let b: Vec<char> = pattern.chars().collect();
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate().skip(1) {
        row[0] = i;
    }
    for j in 1..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let sub = dp[i - 1][j - 1] + dir_dist(a[i - 1], b[j - 1]);
            let del = dp[i - 1][j] + 1;
            let ins = dp[i][j - 1] + 1;
            dp[i][j] = sub.min(del).min(ins);
        }
    }
    dp[a.len()][b.len()]
}

/// 有效容差 = min(用户容差, pattern 长度/2)；长度 1 的手势只精确匹配，避免误触。
/// 弦式手势（含 'L'）一律只精确匹配：左键单击被吞掉代价高，宁缺勿滥。
pub fn tolerance_for(pattern: &str, user_tolerance: u32) -> u32 {
    if is_chord(pattern) {
        return 0;
    }
    let len = pattern.chars().count() as u32;
    if len == 0 {
        return 0;
    }
    user_tolerance.min(len / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_down_is_single_dir() {
        // 直线不再产出 "DD"，折叠后只有一个方向
        let pts = vec![(0, 0), (0, 30), (0, 60), (0, 100)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "2");
    }

    #[test]
    fn continuous_stroke_splits_directions() {
        // 连贯一笔：下 → 左 → 右上，不抬手
        let pts = vec![(0, 0), (0, 60), (-60, 60), (0, 20)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "249");
    }

    #[test]
    fn diagonal_is_its_own_dir() {
        let pts = vec![(0, 0), (40, -40)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "9");
    }

    #[test]
    fn jitter_filtered() {
        let pts = vec![(0, 0), (3, 2), (-2, 4), (60, 5)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "6");
    }

    #[test]
    fn drift_does_not_split() {
        // 画 ← 时手向上带一点漂移：不应多出 ↖ 段（转折阈值的核心场景）
        let pts = vec![(0, 0), (-30, 0), (-60, -8), (-90, -15), (-120, -18)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "4");
    }

    #[test]
    fn decisive_turn_still_splits() {
        // 果断的 ← 然后 ↖：必须能拆成两段
        let pts = vec![(0, 0), (-40, 0), (-80, 0), (-100, -40), (-120, -80)];
        assert_eq!(sequence_from_i32(&pts, 25.0, 20.0), "47");
    }

    #[test]
    fn fuzzy_close_dirs() {
        assert_eq!(fuzzy_distance("2", "1"), 1); // ↓ vs ↙ 差 45°
        // 相反方向：替换代价 4 被 DP 的"删+插"（1+1=2）取代，等效于删掉重画
        assert_eq!(fuzzy_distance("2", "8"), 2);
        assert_eq!(fuzzy_distance("82", "8"), 1); // 少画一段
        assert_eq!(fuzzy_distance("8", "82"), 1); // 多画一段
        assert_eq!(fuzzy_distance("42", "24"), 2); // 顺序画反
        assert_eq!(fuzzy_distance("6", "6"), 0);
    }

    #[test]
    fn chord_matching() {
        // 弦式：方向段 + 左键单击
        assert_eq!(fuzzy_distance("2L", "2L"), 0);
        assert_eq!(fuzzy_distance("2L", "2"), 1); // 少了左键单击
        assert_eq!(fuzzy_distance("2", "2L"), 1);
        // 方向与 L 不可互相替换（代价 4 > 删+插 2）
        assert_eq!(fuzzy_distance("2L", "8L"), 2);
        // 弦式只精确匹配
        assert_eq!(tolerance_for("2L", 2), 0);
        assert_eq!(tolerance_for("2", 2), 0);
        assert_eq!(is_chord("2L"), true);
        assert_eq!(is_chord("42"), false);
    }

    #[test]
    fn tolerance_guard() {
        assert_eq!(tolerance_for("42", 2), 1);
        assert_eq!(tolerance_for("468", 2), 1);
    }
}
