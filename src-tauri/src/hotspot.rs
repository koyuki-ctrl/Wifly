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
    if let Some(uuid) = find_active_hotspot_uuid().await {
        // Try to bring down the hotspot connection
        let output = Command::new("nmcli")
            .args(["connection", "down", &uuid])
            .output()
            .await
            .map_err(|e| format!("Failed to stop hotspot: {}", e))?;

        if !output.status.success() {
            // Try alternative: just delete the connection
            let _ = Command::new("nmcli")
                .args(["connection", "delete", &uuid])
                .output()
                .await;
        }

        // Also try deleting the connection to clean up
        let _ = Command::new("nmcli")
            .args(["connection", "delete", &uuid])
            .output()
            .await;
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
        if parts.len() >= 3 && parts[1] == "wifi" && !parts[2].is_empty() {
            let uuid = parts[0];
            // Check if this connection is AP mode
            if let Ok(details) = Command::new("nmcli")
                .args(["--terse", "--fields", "802-11-wireless.mode", "connection", "show", uuid])
                .output()
                .await
            {
                let mode = String::from_utf8_lossy(&details.stdout);
                if mode.trim() == "ap" {
                    return Some(uuid.to_string());
                }
            }
        }
    }
    None
}
