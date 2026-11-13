//! Small formatting helpers.

/// Format a byte count for humans (binary units, short labels).
pub fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

/// Parse flatpak-style human sizes such as "1.2 GB" into bytes.
pub fn parse_human_size(s: &str) -> u64 {
    let s = s.trim();
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let value: f64 = num.trim().parse().unwrap_or(0.0);
    let mult: f64 = match unit.trim().to_uppercase().as_str() {
        "B" => 1.0,
        "KB" | "K" => 1024.0,
        "MB" | "M" => 1024.0 * 1024.0,
        "GB" | "G" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (value * mult) as u64
}

/// Remove an epoch prefix like "1:" from an RPM EVR string.
pub fn strip_epoch(version: &str) -> &str {
    match version.find(':') {
        Some(i) if version[..i].chars().all(|c| c.is_ascii_digit()) => &version[i + 1..],
        _ => version,
    }
}

/// Collapse whitespace and trim a summary line.
pub fn clean_summary(s: &str) -> String {
    let s = s.replace(['\n', '\r', '\t'], " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(fmt_size(900), "900 B");
        assert_eq!(fmt_size(1536), "1.5 KB");
        assert_eq!(fmt_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(parse_human_size("1.5 MB"), 1572864);
        assert_eq!(parse_human_size("2 GB"), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_human_size(""), 0);
    }

    #[test]
    fn epochs() {
        assert_eq!(strip_epoch("1:2.3-4.fc45"), "2.3-4.fc45");
        assert_eq!(strip_epoch("2.3-4.fc45"), "2.3-4.fc45");
    }
}
