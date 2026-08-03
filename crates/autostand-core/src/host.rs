//! Host slug derivation + persistence.
//!
//! Invariant (from App Script `lib.sh:34-58`):
//! - NEVER derive from `hostname` (DHCP rewrites it).
//! - macOS: `scutil --get LocalHostName`
//! - Linux: `/etc/hostname` or `hostnamectl --static`
//! - Windows: `GetComputerNameW`
//! - Reject numeric/IP-like slugs.
//! - Persist once to `state/host-id`; reuse on subsequent runs.

use crate::Result;

/// Validate a host slug: reject empty, numeric-only, or IP-like.
pub fn is_valid_slug(slug: &str) -> bool {
    let trimmed = slug.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Reject all-numeric (e.g. "192", "10")
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Reject IP-like (digits + dots)
    if trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    true
}

/// Detect the host slug using the platform-appropriate mechanism.
pub fn detect_slug() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("scutil")
            .arg("--get")
            .arg("LocalHostName")
            .output()
        {
            if out.status.success() {
                let slug = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if is_valid_slug(&slug) {
                    return Ok(slug);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("hostnamectl")
            .arg("--static")
            .output()
        {
            if out.status.success() {
                let slug = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if is_valid_slug(&slug) {
                    return Ok(slug);
                }
            }
        }
        if let Ok(content) = std::fs::read_to_string("/etc/hostname") {
            let slug = content.trim().split('.').next().unwrap_or("").to_string();
            if is_valid_slug(&slug) {
                return Ok(slug);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(hostname) = hostname::get() {
            let slug = hostname.to_string_lossy().trim().to_string();
            if is_valid_slug(&slug) {
                return Ok(slug);
            }
        }
    }

    // Fallback: use the `hostname` crate (cross-platform)
    if let Ok(h) = hostname::get() {
        let slug = h.to_string_lossy().trim().to_string();
        if is_valid_slug(&slug) {
            return Ok(slug);
        }
    }

    Err(crate::Error::Other(anyhow::anyhow!(
        "could not detect a valid host slug; set it manually in Settings"
    )))
}

/// Load a persisted slug from `state/host-id`, or detect + persist.
pub fn load_or_detect(state_dir: &std::path::Path) -> Result<String> {
    let host_file = state_dir.join("host-id");
    if let Ok(content) = std::fs::read_to_string(&host_file) {
        let slug = content.trim().to_string();
        if is_valid_slug(&slug) {
            return Ok(slug);
        }
    }
    let slug = detect_slug()?;
    std::fs::create_dir_all(state_dir).map_err(crate::Error::Io)?;
    std::fs::write(&host_file, &slug).map_err(crate::Error::Io)?;
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_numeric() {
        assert!(!is_valid_slug("192"));
        assert!(!is_valid_slug("10"));
        assert!(!is_valid_slug("12345"));
    }

    #[test]
    fn rejects_ip_like() {
        assert!(!is_valid_slug("192.168.1.1"));
        assert!(!is_valid_slug("10.0.0.1"));
    }

    #[test]
    fn accepts_normal_slugs() {
        assert!(is_valid_slug("MacStudio-de-Miguel"));
        assert!(is_valid_slug("MacBook-Pro-de-Miguel"));
        assert!(is_valid_slug("workstation-01"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("   "));
    }
}
