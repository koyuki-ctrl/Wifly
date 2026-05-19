use tokio::process::Command;

/// Start a Wi-Fi hotspot using NetworkManager (nmcli) with pkexec for GUI sudo
pub async fn start_hotspot(ssid: &str, password: &str) -> Result<String, String> {
    // First, try to find a suitable wireless interface
    let iface = find_wifi_interface().await?;

    // Use pkexec for graphical sudo prompt
    let output = Command::new("nmcli")
        .args([
            "device",
            "wifi",
            "hotspot",
            "ifname",
            &iface,
            "con-name",
            "wifly-hotspot",
            "ssid",
            ssid,
            "password",
            password,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to execute nmcli: {}", e))?;

    if output.status.success() {
        // Force 2.4 GHz band and WPA/WPA2 mixed mode for maximum device compatibility
        // Budget phones (e.g. Infinix X657) may fail to connect with WPA3 or WPA2-only
        let _ = Command::new("nmcli")
            .args([
                "connection", "modify", "wifly-hotspot",
                "802-11-wireless.band", "bg",
                "802-11-wireless-security.key-mgmt", "wpa-psk",
                "802-11-wireless-security.proto", "wpa rsn",
                "802-11-wireless-security.pairwise", "tkip ccmp",
                "802-11-wireless-security.group", "tkip ccmp",
                "802-11-wireless-security.pmf", "1", // 1 = disable PMF (prevents WPA3)
            ])
            .output()
            .await;

        // Restart the hotspot so the new security settings take effect
        // Without this, the hotspot keeps running with the original WPA3 config
        let _ = Command::new("nmcli")
            .args(["connection", "down", "wifly-hotspot"])
            .output()
            .await;
        let _ = Command::new("nmcli")
            .args(["connection", "up", "wifly-hotspot"])
            .output()
            .await;

        // Get the hotspot IP (usually 10.42.0.1 with NetworkManager)
        let ip = get_hotspot_ip(&iface).await.unwrap_or_else(|_| "10.42.0.1".to_string());
        Ok(ip)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Check if user cancelled the auth dialog
        if stderr.contains("dismissed") || stderr.contains("Not authorized") || stderr.contains("cancelled") {
            Err("Authorization cancelled by user".to_string())
        } else {
            Err(format!("Failed to start hotspot: {} {}", stderr.trim(), stdout.trim()))
        }
    }
}

/// Stop the Wi-Fi hotspot
pub async fn stop_hotspot() -> Result<(), String> {
    // Strategy 1: Try to bring down known hotspot connection names
    for name in &["wifly-hotspot", "Hotspot", "Hotspot-1", "Hotspot-2"] {
        let output = Command::new("nmcli")
            .args(["connection", "down", name])
            .output()
            .await;
        if let Ok(out) = &output {
            if out.status.success() {
                // Also delete the connection to clean up
                let _ = Command::new("nmcli")
                    .args(["connection", "delete", name])
                    .output()
                    .await;
                return Ok(());
            }
        }
    }

    // Strategy 2: Find any active AP-mode wifi connection by UUID
    if let Some(uuid) = find_active_hotspot_uuid().await {
        let output = Command::new("nmcli")
            .args(["connection", "down", &uuid])
            .output()
            .await
            .map_err(|e| format!("Failed to stop hotspot: {}", e))?;

        if !output.status.success() {
            let _ = Command::new("nmcli")
                .args(["connection", "delete", &uuid])
                .output()
                .await;
        } else {
            let _ = Command::new("nmcli")
                .args(["connection", "delete", &uuid])
                .output()
                .await;
        }

        return Ok(());
    }

    Ok(())
}

/// Find the first available wireless interface
async fn find_wifi_interface() -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(["--terse", "--fields", "DEVICE,TYPE", "device", "status"])
        .output()
        .await
        .map_err(|e| format!("Failed to query network devices: {}. Is NetworkManager installed?", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 && parts[1] == "wifi" {
            return Ok(parts[0].to_string());
        }
    }

    Err("No Wi-Fi interface found. Make sure your Wi-Fi adapter is enabled.".to_string())
}

/// Get the IP address assigned to the hotspot interface
async fn get_hotspot_ip(iface: &str) -> Result<String, String> {
    let output = Command::new("ip")
        .args(["addr", "show", iface])
        .output()
        .await
        .map_err(|e| format!("Failed to get interface IP: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("inet ") && !trimmed.contains("127.0.0.1") {
            // Parse "inet 10.42.0.1/24 ..."
            if let Some(addr) = trimmed.split_whitespace().nth(1) {
                if let Some(ip) = addr.split('/').next() {
                    return Ok(ip.to_string());
                }
            }
        }
    }

    Err("Could not determine hotspot IP".to_string())
}

/// Check if a hotspot is currently active
#[allow(dead_code)]
pub async fn is_hotspot_active() -> bool {
    find_active_hotspot_uuid().await.is_some()
}

/// Helper to find the UUID of an active hotspot (AP mode)
async fn find_active_hotspot_uuid() -> Option<String> {
    let output = Command::new("nmcli")
        .args(["--terse", "--fields", "UUID,TYPE,DEVICE", "connection", "show", "--active"])
        .output()
        .await
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        // nmcli --terse returns "802-11-wireless" for wifi TYPE
        if parts.len() >= 3 && (parts[1] == "wifi" || parts[1] == "802-11-wireless") && !parts[2].is_empty() {
            let uuid = parts[0];
            // Check if this connection is AP mode
            if let Ok(details) = Command::new("nmcli")
                .args(["--terse", "--fields", "802-11-wireless.mode", "connection", "show", uuid])
                .output()
                .await
            {
                let mode = String::from_utf8_lossy(&details.stdout);
                let mode_trimmed = mode.trim();
                // nmcli --terse returns "802-11-wireless.mode:ap"
                if mode_trimmed == "ap" || mode_trimmed.ends_with(":ap") {
                    return Some(uuid.to_string());
                }
            }
        }
    }
    None
}

/// Represents a device connected to the hotspot
#[derive(Debug, Clone, serde::Serialize)]
pub struct HotspotDevice {
    pub ip: String,
    pub mac: String,
    pub interface: String,
}

/// Get the list of devices currently connected to the hotspot
/// by reading the neighbor table (ip neigh) for the hotspot subnet (10.42.0.x)
pub async fn get_connected_devices() -> Vec<HotspotDevice> {
    let mut devices = Vec::new();

    // Use `ip neigh` instead of `/proc/net/arp` because the ARP cache keeps
    // disconnected devices for a long time. `ip neigh` shows the actual state.
    let output = Command::new("ip")
        .args(["neigh", "show"])
        .output()
        .await;

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            // Format: <IP> dev <device> lladdr <MAC> <STATE>
            // Example: 10.42.0.15 dev wlp3s0 lladdr a1:b2:c3:d4:e5:f6 REACHABLE
            let parts: Vec<&str> = line.split_whitespace().collect();
            
            if parts.len() >= 6 && parts[1] == "dev" && parts[3] == "lladdr" {
                let ip = parts[0];
                let device = parts[2];
                let mac = parts[4];
                let state = parts[5];

                // Only count devices on the hotspot subnet (10.42.0.x)
                // FAILED means the device is confirmed disconnected.
                // REACHABLE, DELAY, STALE, PROBE mean the device is or was recently connected.
                if ip.starts_with("10.42.0.") && state != "FAILED" {
                    let ip_string = ip.to_string();
                    devices.push(HotspotDevice {
                        ip: ip_string.clone(),
                        mac: mac.to_string(),
                        interface: device.to_string(),
                    });

                    // Proactively ping the device in the background.
                    // This forces the Linux kernel to refresh the ARP state.
                    // If the device has disconnected, the ping fails and the state 
                    // becomes FAILED almost instantly, updating our UI reactively.
                    // If it's a sleeping phone, it will reply and stay REACHABLE.
                    tokio::spawn(async move {
                        let _ = Command::new("ping")
                            .args(["-c", "1", "-w", "2", &ip_string])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .await;
                    });
                }
            }
        }
    }

    devices
}
