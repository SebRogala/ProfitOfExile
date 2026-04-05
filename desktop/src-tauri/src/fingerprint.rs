//! Device fingerprint computation.
//!
//! Generates a stable, hardware-based device ID by hashing:
//!   - Windows MachineGuid (registry)
//!   - CPU ProcessorId (WMI)
//!   - Motherboard serial number (WMI)
//!   - A build-time secret (prevents raw hardware data leaks)
//!
//! Falls back to a random UUID when hardware data is unavailable (e.g. WMI
//! service disabled). The fallback is volatile — a new ID is generated on each
//! app launch — but it still allows activity tracking within a single session.

use sha2::{Digest, Sha256};

/// Build-time secret injected via `APP_FINGERPRINT_SECRET` env var.
/// Falls back to a fixed dev-only value so local builds work without CI secrets.
const APP_SECRET: &str = match option_env!("APP_FINGERPRINT_SECRET") {
    Some(s) => s,
    None => "poe-dev-fingerprint-salt",
};

/// Compute the device fingerprint. Returns a hex-encoded SHA-256 hash.
///
/// On non-Windows platforms (or when all hardware queries fail), returns a
/// random UUID v4 string instead.
pub fn compute_device_id() -> String {
    #[cfg(windows)]
    {
        match try_hardware_fingerprint() {
            Some(fp) => fp,
            None => {
                log::warn!("Hardware fingerprint failed — using volatile random UUID");
                uuid::Uuid::new_v4().to_string()
            }
        }
    }

    #[cfg(not(windows))]
    {
        log::warn!("Non-Windows platform — using volatile random UUID as device ID");
        uuid::Uuid::new_v4().to_string()
    }
}

/// Attempt to read hardware identifiers and hash them.
#[cfg(windows)]
fn try_hardware_fingerprint() -> Option<String> {
    let machine_guid = read_machine_guid().unwrap_or_default();
    let cpu_id = read_wmi_value("Win32_Processor", "ProcessorId").unwrap_or_default();
    let board_serial = read_wmi_value("Win32_BaseBoard", "SerialNumber").unwrap_or_default();

    // Need at least one real identifier — otherwise the hash is just the secret.
    if machine_guid.is_empty() && cpu_id.is_empty() && board_serial.is_empty() {
        log::warn!("All hardware identifiers empty");
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(machine_guid.as_bytes());
    hasher.update(cpu_id.as_bytes());
    hasher.update(board_serial.as_bytes());
    hasher.update(APP_SECRET.as_bytes());

    Some(hex::encode(hasher.finalize()))
}

/// Read MachineGuid from the Windows registry.
#[cfg(windows)]
fn read_machine_guid() -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
        .ok()?;
    let guid: String = key.get_value("MachineGuid").ok()?;
    Some(guid)
}

/// Query a single WMI value via a PowerShell one-liner.
///
/// WMI COM bindings are fragile in Rust — PowerShell is reliable and only runs
/// once at startup, so the ~200ms overhead is acceptable.
#[cfg(windows)]
fn read_wmi_value(class: &str, property: &str) -> Option<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-CimInstance -ClassName {} -Property {}).{}",
                class, property, property
            ),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        log::warn!(
            "WMI query {}.{} failed: {}",
            class,
            property,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() || value == "None" {
        return None;
    }
    Some(value)
}
