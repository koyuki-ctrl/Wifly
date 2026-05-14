use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::path::PathBuf;

/// Information about a shared file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFile {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub added_at: String,
}

/// Information about a connected device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedDevice {
    pub ip: String,
    pub user_agent: String,
    pub connected_at: String,
    pub last_seen: String,
    pub transfer: u64,
}

/// Server status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub ssid: Option<String>,
    pub hotspot_active: bool,
    pub connected_devices: usize,
    pub shared_files_count: usize,
    pub shared_files: Vec<SharedFile>,
    pub total_transfer: u64,
}

/// Shared application state managed by Tauri
pub struct AppState {
    pub running: bool,
    pub server_ip: Option<String>,
    pub server_port: Option<u16>,
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub hotspot_active: bool,
    pub shared_files: Vec<SharedFile>,
    pub connected_devices: HashMap<String, ConnectedDevice>,
    pub total_transfer: u64,
    pub server_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    pub shared_dir: PathBuf,
    pub app_handle: Option<tauri::AppHandle>,
}

impl AppState {
    pub fn new(shared_dir: PathBuf) -> Self {
        Self {
            running: false,
            server_ip: None,
            server_port: None,
            ssid: None,
            password: None,
            hotspot_active: false,
            shared_files: Vec::new(),
            connected_devices: HashMap::new(),
            total_transfer: 0,
            server_shutdown: None,
            shared_dir,
            app_handle: None,
        }
    }

    pub fn get_status(&self) -> ServerStatus {
        ServerStatus {
            running: self.running,
            ip: self.server_ip.clone(),
            port: self.server_port,
            ssid: self.ssid.clone(),
            hotspot_active: self.hotspot_active,
            connected_devices: self.connected_devices.len(),
            shared_files_count: self.shared_files.len(),
            shared_files: self.shared_files.clone(),
            total_transfer: self.total_transfer,
        }
    }
}

/// Wrapper for Tauri managed state
pub struct WiflyState(pub std::sync::Arc<Mutex<AppState>>);
