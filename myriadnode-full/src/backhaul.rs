use anyhow::Result;
use std::net::IpAddr;
use std::process::Command;
use tracing::{debug, warn};

/// Backhaul detection for IP-based network interfaces
///
/// This module detects when a network interface (e.g., Wi-Fi, Ethernet) is being
/// used as the primary internet uplink (backhaul) and should not be used for
/// mesh networking to avoid interfering with the user's internet connection.
/// Backhaul status for a network interface
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackhaulStatus {
    /// Interface is the primary internet uplink (backhaul)
    IsBackhaul,
    /// Interface is not being used as backhaul, safe for mesh
    NotBackhaul,
    /// Unable to determine status
    Unknown,
}

/// Configuration for backhaul detection
#[derive(Debug, Clone)]
pub struct BackhaulConfig {
    /// Allow mesh networking on backhaul interfaces
    pub allow_backhaul_mesh: bool,
    /// Check interval for backhaul status (seconds)
    pub check_interval_secs: u64,
}

impl Default for BackhaulConfig {
    fn default() -> Self {
        Self {
            allow_backhaul_mesh: false,
            check_interval_secs: 300, // Check every 5 minutes
        }
    }
}

/// Backhaul detector
pub struct BackhaulDetector {
    config: BackhaulConfig,
}

impl BackhaulDetector {
    pub fn new(config: BackhaulConfig) -> Self {
        Self { config }
    }

    /// Check if an interface is currently being used as backhaul
    pub fn check_interface(&self, interface_name: &str) -> Result<BackhaulStatus> {
        debug!("Checking backhaul status for interface: {}", interface_name);

        // If user explicitly allows backhaul mesh, skip detection
        if self.config.allow_backhaul_mesh {
            debug!("Backhaul mesh explicitly allowed, returning NotBackhaul");
            return Ok(BackhaulStatus::NotBackhaul);
        }

        // Platform-specific detection
        #[cfg(target_os = "linux")]
        {
            self.check_interface_linux(interface_name)
        }

        #[cfg(target_os = "macos")]
        {
            self.check_interface_macos(interface_name)
        }

        #[cfg(target_os = "windows")]
        {
            self.check_interface_windows(interface_name)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            warn!("Backhaul detection not implemented for this platform");
            Ok(BackhaulStatus::Unknown)
        }
    }

    /// Check if a specific IP address is on an interface being used as backhaul
    pub fn check_ip_address(&self, ip: IpAddr) -> Result<BackhaulStatus> {
        debug!("Checking backhaul status for IP: {}", ip);

        if self.config.allow_backhaul_mesh {
            return Ok(BackhaulStatus::NotBackhaul);
        }

        // Get interface name for this IP
        let interface_name = self.get_interface_for_ip(ip)?;

        if let Some(iface) = interface_name {
            self.check_interface(&iface)
        } else {
            Ok(BackhaulStatus::Unknown)
        }
    }

    #[cfg(target_os = "linux")]
    fn check_interface_linux(&self, interface_name: &str) -> Result<BackhaulStatus> {
        // Use `ip route` to check for default gateway on this interface
        let output = Command::new("ip")
            .args(["route", "show", "default"])
            .output()?;

        if !output.status.success() {
            warn!("Failed to execute 'ip route' command");
            return Ok(BackhaulStatus::Unknown);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if default route uses this interface
        // Format: "default via 192.168.1.1 dev wlan0 proto dhcp metric 600"
        for line in stdout.lines() {
            if line.contains("default") && line.contains(&format!("dev {}", interface_name)) {
                debug!(
                    "Interface {} has default route (is backhaul)",
                    interface_name
                );
                return Ok(BackhaulStatus::IsBackhaul);
            }
        }

        debug!(
            "Interface {} does not have default route (not backhaul)",
            interface_name
        );
        Ok(BackhaulStatus::NotBackhaul)
    }

    #[cfg(target_os = "macos")]
    fn check_interface_macos(&self, interface_name: &str) -> Result<BackhaulStatus> {
        // Use `netstat -rn` to check for default gateway
        let output = Command::new("netstat").args(["-rn"]).output()?;

        if !output.status.success() {
            warn!("Failed to execute 'netstat' command");
            return Ok(BackhaulStatus::Unknown);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if default route uses this interface
        // Format: "default            192.168.1.1        UGSc           en0"
        for line in stdout.lines() {
            if line.starts_with("default") && line.ends_with(interface_name) {
                debug!(
                    "Interface {} has default route (is backhaul)",
                    interface_name
                );
                return Ok(BackhaulStatus::IsBackhaul);
            }
        }

        debug!(
            "Interface {} does not have default route (not backhaul)",
            interface_name
        );
        Ok(BackhaulStatus::NotBackhaul)
    }

    #[cfg(target_os = "windows")]
    fn check_interface_windows(&self, interface_name: &str) -> Result<BackhaulStatus> {
        // Use `route print` to check for default gateway
        let output = Command::new("route").args(["print", "0.0.0.0"]).output()?;

        if !output.status.success() {
            warn!("Failed to execute 'route print' command");
            return Ok(BackhaulStatus::Unknown);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Look for default route (0.0.0.0) with this interface
        // This is a simplified check - Windows interface names are complex
        if stdout.contains(interface_name) && stdout.contains("0.0.0.0") {
            debug!(
                "Interface {} may be backhaul (Windows detection)",
                interface_name
            );
            return Ok(BackhaulStatus::IsBackhaul);
        }

        Ok(BackhaulStatus::NotBackhaul)
    }

    /// Get the interface name for a given IP address
    fn get_interface_for_ip(&self, ip: IpAddr) -> Result<Option<String>> {
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("ip").args(["addr", "show"]).output()?;

            if !output.status.success() {
                return Ok(None);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut current_interface: Option<String> = None;

            for line in stdout.lines() {
                // Interface line: "2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> ..."
                if let Some(iface) = line.split(':').nth(1) {
                    if !line.starts_with(' ') {
                        current_interface = Some(iface.trim().to_string());
                    }
                }

                // IP line: "    inet 192.168.1.100/24 ..."
                if line.contains(&ip.to_string()) {
                    return Ok(current_interface);
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            warn!("get_interface_for_ip not implemented for this platform");
        }

        Ok(None)
    }

    /// Check all interfaces and return a list of backhaul interfaces
    pub fn detect_all_backhauls(&self) -> Result<Vec<String>> {
        let mut backhauls = Vec::new();

        #[cfg(target_os = "linux")]
        {
            let output = Command::new("ip")
                .args(["route", "show", "default"])
                .output()?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                for line in stdout.lines() {
                    if let Some(dev_pos) = line.find("dev ") {
                        let after_dev = &line[dev_pos + 4..];
                        if let Some(iface_name) = after_dev.split_whitespace().next() {
                            backhauls.push(iface_name.to_string());
                            debug!("Detected backhaul interface: {}", iface_name);
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new("netstat").args(["-rn"]).output()?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                for line in stdout.lines() {
                    if line.starts_with("default") {
                        if let Some(iface_name) = line.split_whitespace().last() {
                            backhauls.push(iface_name.to_string());
                            debug!("Detected backhaul interface: {}", iface_name);
                        }
                    }
                }
            }
        }

        Ok(backhauls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backhaul_config_default() {
        let config = BackhaulConfig::default();
        assert!(!config.allow_backhaul_mesh);
        assert_eq!(config.check_interval_secs, 300);
    }

    #[test]
    fn test_backhaul_detector_creation() {
        let config = BackhaulConfig::default();
        let detector = BackhaulDetector::new(config);
        assert!(!detector.config.allow_backhaul_mesh);
    }

    #[test]
    fn test_allow_backhaul_mesh_override() {
        let config = BackhaulConfig {
            allow_backhaul_mesh: true,
            check_interval_secs: 60,
        };
        let detector = BackhaulDetector::new(config);

        // When allow_backhaul_mesh is true, should always return NotBackhaul
        let result = detector.check_interface("eth0").unwrap();
        assert_eq!(result, BackhaulStatus::NotBackhaul);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_detect_backhauls() {
        let config = BackhaulConfig::default();
        let detector = BackhaulDetector::new(config);

        // This test will only work on systems with network interfaces and `ip` command
        // Just check that it doesn't panic - may fail in restricted environments
        let result = detector.detect_all_backhauls();

        // Accept both success and failure (e.g., if `ip` command not available)
        // The important thing is that it doesn't panic
        match result {
            Ok(backhauls) => {
                // Success - log for debugging
                println!("Detected backhauls: {:?}", backhauls);
            }
            Err(e) => {
                // Failed (e.g., no `ip` command) - this is okay
                println!(
                    "Backhaul detection failed (expected in some environments): {}",
                    e
                );
            }
        }
    }
}
