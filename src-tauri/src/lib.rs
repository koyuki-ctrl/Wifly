mod hotspot;
mod server;
mod state;

use state::{SharedFile, WiflyState};
use std::path::PathBuf;

/// Start the hotspot + HTTP file server
#[tauri::command]
async fn start_server(
    ssid: String,
    password: String,
    port: u16,
    state: tauri::State<'_, WiflyState>,
) -> Result<serde_json::Value, String> {
    // Check if already running
    {
        let s = state.0.lock().map_err(|e| e.to_string())?;
        if s.running {
            return Err("Server is already running".to_string());
        }
    }

    // 1. Start Wi-Fi hotspot via pkexec (shows GUI dialog)
    let hotspot_ip = hotspot::start_hotspot(&ssid, &password).await?;

    // Give NetworkManager a moment to fully set up the hotspot interface
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 2. Get the actual IP
    let server_ip = hotspot_ip.clone();
    let bind_addr = format!("0.0.0.0:{}", port);

    let app_state_clone = state.0.clone();
    
    // 4. Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // 6. Start HTTP server in background
    let bind_addr_clone = bind_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_http_server(
            bind_addr_clone,
            app_state_clone,
            shutdown_rx,
        )
        .await
        {
            eprintln!("HTTP server error: {}", e);
        }
    });

    // 7. Update state
    {
        let mut s = state.0.lock().map_err(|e| e.to_string())?;
        s.running = true;
        s.server_ip = Some(server_ip.clone());
        s.server_port = Some(port);
        s.ssid = Some(ssid);
        s.password = Some(password);
        s.hotspot_active = true;
        s.server_shutdown = Some(shutdown_tx);
    }

    Ok(serde_json::json!({
        "ip": server_ip,
        "port": port,
        "hotspot": true
    }))
}

/// Stop the hotspot + HTTP server
#[tauri::command]
async fn stop_server(
    state: tauri::State<'_, WiflyState>,
) -> Result<(), String> {
    // 1. Send shutdown signal to HTTP server
    {
        let mut s = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(shutdown_tx) = s.server_shutdown.take() {
            let _ = shutdown_tx.send(());
        }
        s.running = false;
        s.server_ip = None;
        s.server_port = None;
        s.hotspot_active = false;
        s.connected_devices.clear();
    }

    // 2. Stop the Wi-Fi hotspot
    let _ = hotspot::stop_hotspot().await;

    Ok(())
}

/// Get current server status and connected devices
#[tauri::command]
fn get_status(
    state: tauri::State<'_, WiflyState>,
) -> Result<serde_json::Value, String> {
    let s = state.0.lock().map_err(|e| e.to_string())?;
    let mut status_json = serde_json::to_value(s.get_status()).unwrap();
    
    // Inject connected devices
    if let serde_json::Value::Object(ref mut map) = status_json {
        let devices: Vec<_> = s.connected_devices.values().cloned().collect();
        map.insert("devices_list".to_string(), serde_json::to_value(devices).unwrap());
    }
    
    Ok(status_json)
}

/// Add a file to the shared files list
#[tauri::command]
fn add_shared_file(
    path: String,
    state: tauri::State<'_, WiflyState>,
) -> Result<serde_json::Value, String> {
    let file_path = PathBuf::from(&path);
    if !file_path.exists() {
        return Err(format!("File does not exist: {}", path));
    }

    let metadata = std::fs::metadata(&file_path)
        .map_err(|e| format!("Cannot read file metadata: {}", e))?;

    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let file = SharedFile {
        id: uuid::Uuid::new_v4().to_string(),
        name: file_name.clone(),
        path: file_path.clone(),
        size: metadata.len(),
        added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
    };

    let mut s = state.0.lock().map_err(|e| e.to_string())?;

    // Check for duplicates
    if s.shared_files.iter().any(|f| f.path == file_path) {
        return Err("File already shared".to_string());
    }

    let response = serde_json::json!({
        "id": file.id,
        "name": file.name,
        "size": file.size,
        "path": file.path.to_string_lossy()
    });

    s.shared_files.push(file);

    Ok(response)
}

/// Remove a file from the shared files list
#[tauri::command]
fn remove_shared_file(
    file_id: String,
    state: tauri::State<'_, WiflyState>,
) -> Result<(), String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    
    if let Some(pos) = s.shared_files.iter().position(|f| f.id == file_id) {
        let file = s.shared_files.remove(pos);
        
        // If the file is in the shared directory (an upload), delete it from disk too
        if file.path.starts_with(&s.shared_dir) {
            let _ = std::fs::remove_file(&file.path);
        }
    }
    
    Ok(())
}

/// Get the current shared directory
#[tauri::command]
fn get_shared_dir(state: tauri::State<'_, WiflyState>) -> Result<String, String> {
    let s = state.0.lock().map_err(|e| e.to_string())?;
    Ok(s.shared_dir.to_string_lossy().to_string())
}

/// Change the shared directory
#[tauri::command]
fn set_shared_dir(
    path: String,
    state: tauri::State<'_, WiflyState>,
) -> Result<(), String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    let new_path = PathBuf::from(path);
    std::fs::create_dir_all(&new_path).map_err(|e| e.to_string())?;
    s.shared_dir = new_path;
    Ok(())
}

/// List all shared files
#[tauri::command]
fn list_shared_files(
    state: tauri::State<'_, WiflyState>,
) -> Result<serde_json::Value, String> {
    let s = state.0.lock().map_err(|e| e.to_string())?;
    let files: Vec<serde_json::Value> = s
        .shared_files
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "name": f.name,
                "size": f.size,
                "path": f.path.to_string_lossy()
            })
        })
        .collect();
    Ok(serde_json::json!(files))
}

/// Get the local IP address
#[tauri::command]
fn get_local_ip() -> Result<String, String> {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .map_err(|e| format!("Failed to get local IP: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Determine shared directory
    let shared_dir = dirs_next()
        .unwrap_or_else(|| PathBuf::from("/tmp/wifly-shared"));
    std::fs::create_dir_all(&shared_dir).ok();

    let app_state = WiflyState(std::sync::Arc::new(std::sync::Mutex::new(crate::state::AppState::new(shared_dir))));

    let app_state_clone = app_state.0.clone();
    
    tauri::Builder::default()
        .setup(move |app| {
            if let Ok(mut state) = app_state_clone.lock() {
                state.app_handle = Some(app.handle().clone());
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_status,
            add_shared_file,
            remove_shared_file,
            list_shared_files,
            get_local_ip,
            get_shared_dir,
            set_shared_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Get a reasonable directory for shared files
fn dirs_next() -> Option<PathBuf> {
    // Save files directly to the user's Downloads folder
    if let Ok(home) = std::env::var("HOME") {
        let dir = PathBuf::from(home).join("Downloads").join("Wifly");
        return Some(dir);
    }
    // Windows fallback
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let dir = PathBuf::from(userprofile).join("Downloads").join("Wifly");
        return Some(dir);
    }
    None
}
