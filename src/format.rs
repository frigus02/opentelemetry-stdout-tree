use std::time::{Duration, SystemTime};

pub(crate) fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 7200 {
        format!("{}h", secs / 3600)
    } else if secs >= 120 {
        format!("{}m", secs / 60)
    } else if secs > 0 {
        format!("{}s", d.as_secs())
    } else if d.as_millis() > 0 {
        format!("{}ms", d.as_millis())
    } else {
        "0".into()
    }
}

pub(crate) fn format_timing(
    available_width: usize,
    parent_start: SystemTime,
    parent_duration: Duration,
    start: SystemTime,
    duration: Duration,
    fill_char: char,
) -> String {
    if available_width == 0 {
        return "".into();
    }
    if parent_duration.as_nanos() == 0 {
        return fill_char.to_string().repeat(available_width);
    }

    let scale = available_width as f64 / parent_duration.as_secs_f64();
    let start_gap = start.duration_since(parent_start).unwrap_or_default();
    let fill_len = ((duration.as_secs_f64() * scale).round() as usize).max(1);
    let start_len = ((start_gap.as_secs_f64() * scale).round() as usize).min(available_width - fill_len);

    format!(
        "{start}{fill}{end}",
        start = " ".repeat(start_len),
        fill = fill_char.to_string().repeat(fill_len),
        end = " ".repeat(available_width - start_len - fill_len)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(Duration::from_nanos(1),   "0"    ; "zero")]
    #[test_case(Duration::from_millis(10), "10ms" ; "millis")]
    #[test_case(Duration::from_secs(35),   "35s"  ; "seconds")]
    #[test_case(Duration::from_secs(120),  "2m"   ; "minutes")]
    #[test_case(Duration::from_secs(9000), "2h"   ; "hours")]
    fn duration(d: Duration, expected: &'static str) {
        assert_eq!(expected.to_string(), format_duration(d));
    }

    #[test_case(15, 10,  1, 2, '=', "  ===          " ; "basic case")]
    #[test_case( 0, 10,  1, 2, '=', ""                ; "zero available width")]
    #[test_case(15,  0,  1, 2, '=', "===============" ; "zero parent duration")]
    #[test_case(15, 10,  1, 0, '=', "  =            " ; "zero duration")]
    #[test_case(15, 10, -5, 1, '=', "==             " ; "starts before parent")]
    #[test_case(15, 10, 10, 1, '=', "             ==" ; "ends after parent")]
    #[test_case(15, 10,  1, 2, 'a', "  aaa          " ; "different fill char")]
    fn timing(
        available_width: usize,
        parent_duration_secs: u64,
        start_diff_secs: i64,
        duration_secs: u64,
        fill_char: char,
        expected: &'static str,
    ) {
        let parent_start = SystemTime::now();
        let parent_duration = Duration::from_secs(parent_duration_secs);
        let duration = Duration::from_secs(duration_secs);
        let start = if start_diff_secs < 0 {
            parent_start
                .checked_sub(Duration::from_secs((-start_diff_secs) as u64))
                .unwrap()
        } else {
            parent_start
                .checked_add(Duration::from_secs(start_diff_secs as u64))
                .unwrap()
        };
        assert_eq!(
            expected.to_string(),
            format_timing(
                available_width,
                parent_start,
                parent_duration,
                start,
                duration,
                fill_char
            )
        );
    }
}
