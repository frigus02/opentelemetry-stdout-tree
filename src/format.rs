use std::time::{Duration, SystemTime};

pub(crate) fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs > 7200 {
        format!("{}h", secs / 3600)
    } else if secs > 120 {
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
    let scale = available_width as f64 / parent_duration.as_secs_f64();
    let start_gap = start.duration_since(parent_start).unwrap_or_default();
    let start_len = ((start_gap.as_secs_f64() * scale).round() as usize).min(available_width - 1);
    let fill_len = ((duration.as_secs_f64() * scale).round() as usize).max(1);

    format!(
        "{start}{fill}{end}",
        start = " ".repeat(start_len),
        fill = fill_char.to_string().repeat(fill_len),
        end = " ".repeat(available_width - start_len - fill_len)
    )
}
