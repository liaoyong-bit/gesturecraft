/// 手势轨迹 → 方向序列。
/// 以累计位移超过阈值为一段，取主导轴方向，字符集 U/D/L/R，折叠相邻重复。
pub fn sequence_from_f32(points: &[(f32, f32)], min_seg: f32) -> String {
    let mut seq = String::new();
    if points.len() < 2 {
        return seq;
    }
    let mut anchor = points[0];
    for &p in &points[1..] {
        let dx = p.0 - anchor.0;
        let dy = p.1 - anchor.1;
        if dx.hypot(dy) >= min_seg {
            let ch = if dx.abs() >= dy.abs() {
                if dx > 0.0 { 'R' } else { 'L' }
            } else if dy > 0.0 {
                'D' // 屏幕 y 轴向下
            } else {
                'U'
            };
            if seq.chars().last() != Some(ch) {
                seq.push(ch);
            }
            anchor = p;
        }
    }
    seq
}

pub fn sequence_from_i32(points: &[(i32, i32)], min_seg: f32) -> String {
    let f: Vec<(f32, f32)> = points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
    sequence_from_f32(&f, min_seg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_then_down() {
        let pts = vec![(0, 0), (50, 0), (100, 5), (100, 60), (100, 120)];
        assert_eq!(sequence_from_i32(&pts, 30.0), "RD");
    }

    #[test]
    fn jitter_is_filtered() {
        let pts = vec![(0, 0), (5, 3), (8, -2), (60, 0)];
        assert_eq!(sequence_from_i32(&pts, 30.0), "R");
    }
}
