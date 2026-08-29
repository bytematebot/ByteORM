//! Process metrics. Linux-only: read straight from `/proc/self/status`.

/// Peak resident set size of this process, in kilobytes (`VmHWM`).
/// `None` on platforms without `/proc`.
pub fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_hwm(&status)
}

/// Current resident set size in kilobytes (`VmRSS`).
pub fn current_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_field(&status, "VmRSS:")
}

fn parse_vm_hwm(status: &str) -> Option<u64> {
    parse_field(status, "VmHWM:")
}

fn parse_field(status: &str, field: &str) -> Option<u64> {
    status
        .lines()
        .find(|line| line.starts_with(field))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_kilobytes_from_status_text() {
        let status = "Name:\tbench\nVmHWM:\t  123456 kB\nVmRSS:\t   65432 kB\n";
        assert_eq!(parse_vm_hwm(status), Some(123456));
        assert_eq!(parse_field(status, "VmRSS:"), Some(65432));
    }
}
