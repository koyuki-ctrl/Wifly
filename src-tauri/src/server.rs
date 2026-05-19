use actix_cors::Cors;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix::{Actor, StreamHandler};
use actix_web_actors::ws;
use tauri::Emitter;
use futures_util::StreamExt;
use std::io::Write;

use crate::state::{AppState, SharedFile, ConnectedDevice};

/// Start the actix-web HTTP server in a background thread
/// Returns the server handle for graceful shutdown
pub async fn start_http_server(
    bind_addr: String,
    app_state: std::sync::Arc<std::sync::Mutex<AppState>>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    let server_state = web::Data::new(app_state);

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(server_state.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024 * 1024)) // Allow up to 10GB uploads
            .route("/", web::get().to(mobile_page))
            .route("/ws/", web::get().to(ws_index))
            .route("/api/files", web::get().to(list_files))
            .route("/api/download/{filename}", web::get().to(download_file))
            .route("/api/upload_chunk", web::post().to(upload_chunk))
            .default_service(web::to(|req: HttpRequest| async move {
                println!("  [?] Unhandled request: {} {}", req.method(), req.path());
                HttpResponse::NotFound().body("Not Found")
            }))
    })
    .bind(&bind_addr)
    .map_err(|e| format!("Failed to bind to {}: {}", bind_addr, e))?
    .disable_signals()
    .run();

    let server_handle = server.handle();

    // Spawn server
    let server_task = tokio::spawn(server);

    // Wait for shutdown signal
    let _ = shutdown_rx.await;
    server_handle.stop(true).await;
    let _ = server_task.await;

    Ok(())
}

/// Mobile-friendly landing page served to phones connecting to the hotspot
async fn mobile_page(
    req: HttpRequest,
    data: web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>,
) -> HttpResponse {
    track_client(&req, &data);

    let state = data.lock().unwrap();
    let files = &state.shared_files;
    let mut file_rows = String::new();

    if files.is_empty() {
        file_rows.push_str(r#"<div class="empty"><p>📂 No files shared yet</p><p class="sub">Files will appear here when shared from the desktop app</p></div>"#);
    } else {
        for f in files.iter() {
            let size_str = format_size(f.size);
            let emoji = get_file_emoji(&f.name);
            file_rows.push_str(&format!(
                r#"<a href="/api/download/{name}" class="file-card" download>
                    <div class="file-icon">{emoji}</div>
                    <div class="file-info">
                        <div class="file-name">{name}</div>
                        <div class="file-size">{size}</div>
                    </div>
                    <div class="dl-icon">⬇</div>
                </a>"#,
                name = html_escape(&f.name),
                emoji = emoji,
                size = size_str
            ));
        }
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no">
    <title>Wifly — File Share</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif; background: #0a0c10; color: #e6e9f0; min-height: 100vh; }}
        .header {{ background: linear-gradient(135deg, rgba(0,229,192,0.12) 0%, rgba(0,229,192,0.03) 100%); border-bottom: 1px solid rgba(0,229,192,0.15); padding: 28px 20px 24px; text-align: center; }}
        .logo {{ width: 48px; height: 48px; border-radius: 14px; border: 1.5px solid rgba(0,229,192,0.5); background: rgba(0,229,192,0.1); display: inline-flex; align-items: center; justify-content: center; font-size: 22px; margin-bottom: 12px; }}
        .brand {{ font-size: 22px; font-weight: 700; letter-spacing: -0.5px; }}
        .brand span {{ color: #00e5c0; }}
        .subtitle {{ font-size: 13px; color: #6e7888; margin-top: 6px; }}
        .count {{ display: inline-block; background: rgba(0,229,192,0.1); border: 1px solid rgba(0,229,192,0.2); color: #00e5c0; font-size: 12px; font-weight: 600; padding: 4px 14px; border-radius: 100px; margin-top: 14px; }}
        .refresh-link {{ display: block; margin-top: 10px; color: #00e5c0; font-size: 13px; cursor: pointer; text-decoration: underline; }}
        .files {{ padding: 16px; display: flex; flex-direction: column; gap: 8px; }}
        .file-card {{ display: flex; align-items: center; gap: 14px; padding: 16px; background: #13161d; border: 1px solid rgba(255,255,255,0.06); border-radius: 14px; text-decoration: none; color: inherit; transition: all 0.2s; -webkit-tap-highlight-color: transparent; }}
        .file-card:active {{ background: #1a1e28; border-color: rgba(0,229,192,0.2); }}
        .file-icon {{ width: 44px; height: 44px; background: rgba(0,229,192,0.08); border: 1px solid rgba(0,229,192,0.12); border-radius: 12px; display: flex; align-items: center; justify-content: center; font-size: 20px; flex-shrink: 0; }}
        .file-info {{ flex: 1; min-width: 0; }}
        .file-name {{ font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
        .file-size {{ font-size: 12px; color: #6e7888; margin-top: 3px; }}
        .dl-icon {{ width: 36px; height: 36px; background: rgba(0,229,192,0.1); border-radius: 10px; display: flex; align-items: center; justify-content: center; font-size: 16px; flex-shrink: 0; color: #00e5c0; }}
        .empty {{ text-align: center; padding: 48px 20px; color: #4e5666; }}
        .empty p {{ font-size: 16px; }}
        .empty .sub {{ font-size: 13px; margin-top: 8px; color: #3a3f4a; }}
        .upload-section {{ padding: 16px; border-top: 1px solid rgba(255,255,255,0.06); }}
        .upload-btn {{ width: 100%; padding: 16px; background: rgba(0,229,192,0.1); border: 1.5px solid rgba(0,229,192,0.3); border-radius: 14px; color: #00e5c0; font-size: 14px; font-weight: 600; cursor: pointer; text-align: center; transition: all 0.2s; -webkit-tap-highlight-color: transparent; }}
        .upload-btn:active {{ background: rgba(0,229,192,0.2); }}
        
        /* Progress UI */
        #progress-container {{ display: none; margin-bottom: 16px; padding: 16px; background: #13161d; border: 1px solid rgba(0,229,192,0.3); border-radius: 14px; }}
        .prog-label {{ font-size: 13px; font-weight: 600; color: #e6e9f0; margin-bottom: 8px; display: flex; justify-content: space-between; }}
        .prog-filename {{ color: #00e5c0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 70%; }}
        .prog-bar-bg {{ width: 100%; height: 6px; background: rgba(255,255,255,0.1); border-radius: 100px; overflow: hidden; }}
        .prog-bar-fill {{ height: 100%; background: #00e5c0; width: 0%; transition: width 0.1s linear; }}
        
        .footer {{ text-align: center; padding: 20px; font-size: 11px; color: #3a3f4a; }}
    </style>
</head>
<body>
    <div class="header">
        <div class="logo">📡</div>
        <div class="brand"><span>Wi</span>fly</div>
        <div class="subtitle">Local file sharing</div>
        <div class="count">{count} file{plural}</div>
        <div class="refresh-link" onclick="location.reload()">🔄 Refresh file list</div>
    </div>
    <div class="files">{files}</div>
    <div class="upload-section">
        <div id="progress-container">
            <div class="prog-label">
                <span class="prog-filename" id="prog-filename">Uploading...</span>
                <span id="prog-pct">0%</span>
            </div>
            <div class="prog-bar-bg">
                <div class="prog-bar-fill" id="prog-bar-fill"></div>
            </div>
        </div>
        <form id="upload-form" enctype="multipart/form-data">
            <input type="file" id="file-input" multiple style="display:none" onchange="uploadFiles(this.files)">
            <div class="upload-btn" id="upload-btn" onclick="document.getElementById('file-input').click()">
                📤 Send files to this computer
            </div>
        </form>
    </div>
    <div class="footer">Wifly v1.0.0 · Powered by Tauri</div>
    <script>
        window.isUploading = false;
        async function uploadFiles(files) {{
            if (!files || files.length === 0) return;
            window.isUploading = true;
            
            const btn = document.getElementById('upload-btn');
            const progCont = document.getElementById('progress-container');
            const progFill = document.getElementById('prog-bar-fill');
            const progPct = document.getElementById('prog-pct');
            const progName = document.getElementById('prog-filename');
            
            btn.style.display = 'none';
            progCont.style.display = 'block';
            
            let allSuccess = true;
            
            for (let i = 0; i < files.length; i++) {{
                const file = files[i];
                progName.textContent = file.name;
                
                // 5MB chunks
                const chunkSize = 5 * 1024 * 1024;
                const totalChunks = Math.ceil(file.size / chunkSize) || 1;
                
                for (let chunkIdx = 0; chunkIdx < totalChunks; chunkIdx++) {{
                    const start = chunkIdx * chunkSize;
                    const end = Math.min(start + chunkSize, file.size);
                    const chunk = file.slice(start, end);
                    
                    const url = `/api/upload_chunk?name=${{encodeURIComponent(file.name)}}&chunk=${{chunkIdx}}&total=${{totalChunks}}`;
                    
                    try {{
                        const res = await fetch(url, {{ method: 'POST', body: chunk }});
                        if (!res.ok) {{
                            const text = await res.text();
                            throw new Error(text);
                        }}
                    }} catch(e) {{
                        allSuccess = false;
                        alert('Upload error for ' + file.name + ': ' + e.message);
                        break;
                    }}
                    
                    const pct = Math.round(((chunkIdx + 1) / totalChunks) * 100);
                    progFill.style.width = pct + '%';
                    progPct.textContent = pct + '%';
                }}
            }}
            
            if (allSuccess) {{
                location.reload();
            }} else {{
                btn.style.display = 'block';
                progCont.style.display = 'none';
            }}
            window.isUploading = false;
        }}

        // WebSocket connection for presence
        const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = protocol + '//' + location.host + '/ws/';
        let ws;
        function connectWs() {{
            ws = new WebSocket(wsUrl);
            ws.onclose = () => {{
                setTimeout(connectWs, 2000); // Reconnect
            }};
        }}
        connectWs();
    </script>
</body>
</html>"#,
        count = files.len(),
        plural = if files.len() != 1 { "s" } else { "" },
        files = file_rows
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

/// API: List shared files as JSON
async fn list_files(
    req: HttpRequest,
    data: web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>,
) -> HttpResponse {
    track_client(&req, &data);
    let state = data.lock().unwrap();
    let files = &state.shared_files;
    HttpResponse::Ok().json(&*files)
}

/// API: Download a specific file
async fn download_file(
    req: HttpRequest,
    data: web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>,
    path: web::Path<String>,
) -> Result<actix_files::NamedFile, Error> {
    track_client(&req, &data);
    let filename = path.into_inner();
    
    // We need to clone the file path and size to avoid holding the MutexGuard across await/response
    let (file_path, file_size) = {
        let state = data.lock().unwrap();
        if let Some(file) = state.shared_files.iter().find(|f| f.name == filename) {
            (file.path.clone(), file.size)
        } else {
            return Err(actix_web::error::ErrorNotFound("File not in shared list"));
        }
    };

    if !file_path.exists() {
        return Err(actix_web::error::ErrorNotFound("File not found on disk"));
    }

    // Update transfer stats eagerly since we stream the file
    {
        let mut state = data.lock().unwrap();
        let ip = get_client_ip(&req);
        if let Some(client) = state.connected_devices.get_mut(&ip) {
            client.last_seen = chrono::Local::now().format("%H:%M").to_string();
            client.transfer += file_size;
        }
        state.total_transfer += file_size;
    }

    // NamedFile streams the file securely and efficiently without loading it into RAM
    let named_file = actix_files::NamedFile::open(file_path)?
        .set_content_disposition(actix_web::http::header::ContentDisposition {
            disposition: actix_web::http::header::DispositionType::Attachment,
            parameters: vec![actix_web::http::header::DispositionParam::Filename(filename)],
        });

    Ok(named_file)
}

#[derive(serde::Deserialize)]
pub struct ChunkUploadQuery {
    name: String,
    chunk: usize,
    total: usize,
}

/// API: Upload a file chunk from a mobile device
async fn upload_chunk(
    req: HttpRequest,
    data: web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>,
    query: web::Query<ChunkUploadQuery>,
    mut payload: web::Payload,
) -> HttpResponse {
    track_client(&req, &data);
    
    let filename = sanitize_filename::sanitize(&query.name);
    let chunk_idx = query.chunk;
    let total_chunks = query.total;
    
    let filepath = {
        let state = data.lock().unwrap();
        let dir = state.shared_dir.clone();
        let _ = std::fs::create_dir_all(&dir);
        dir.join(&filename)
    };

    // Open file in append mode. If it's the first chunk, truncate/create.
    let mut open_opts = std::fs::OpenOptions::new();
    open_opts.create(true).write(true);
    
    if chunk_idx == 0 {
        open_opts.truncate(true);
    } else {
        open_opts.append(true);
    }

    let mut file = match open_opts.open(&filepath) {
        Ok(f) => f,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Failed to open file: {}", e)),
    };

    while let Some(bytes) = payload.next().await {
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => return HttpResponse::BadRequest().body(format!("Payload error: {}", e)),
        };
        if let Err(e) = file.write_all(&bytes) {
            return HttpResponse::InternalServerError().body(format!("Write error: {}", e));
        }
    }

    // Only finalize and add to shared_files when the LAST chunk is received
    if chunk_idx + 1 >= total_chunks {
        let mut state = data.lock().unwrap();
        
        let final_size = std::fs::metadata(&filepath).map(|m| m.len()).unwrap_or(0);

        let entry = SharedFile {
            id: uuid::Uuid::new_v4().to_string(),
            name: filename.clone(),
            size: final_size,
            path: filepath.clone(),
            added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        };
        
        // Remove existing file with same name if it exists (overwrite)
        state.shared_files.retain(|f| f.name != filename);
        state.shared_files.push(entry);
        
        state.total_transfer += final_size;
        
        let ip = get_client_ip(&req);
        if let Some(client) = state.connected_devices.get_mut(&ip) {
            client.transfer += final_size;
        }

        if let Some(handle) = &state.app_handle {
            let _ = handle.emit("device-changed", ());
        }
        
        println!("  [OK] Assembled {} ({} bytes) to {}", filename, final_size, filepath.display());
    }

    HttpResponse::Ok().json(serde_json::json!({"status": "ok", "chunk": chunk_idx}))
}

/// Track client connections
fn track_client(req: &HttpRequest, data: &web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>) {
    let ip = get_client_ip(req);
    let ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Unknown")
        .to_string();

    let now = chrono::Local::now().format("%H:%M").to_string();

    let mut state = data.lock().unwrap();
    state
        .connected_devices
        .entry(ip.clone())
        .and_modify(|c| {
            c.last_seen = now.clone();
            c.user_agent = ua.clone();
        })
        .or_insert(ConnectedDevice {
            ip,
            user_agent: ua,
            connected_at: now.clone(),
            last_seen: now,
            transfer: 0,
        });
}

fn get_client_ip(req: &HttpRequest) -> String {
    req.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    }
}

fn get_file_emoji(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" => "📄",
        "doc" | "docx" => "📝",
        "xls" | "xlsx" => "📊",
        "ppt" | "pptx" => "📑",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => "🖼",
        "mp4" | "mov" | "avi" | "mkv" | "webm" => "🎬",
        "mp3" | "wav" | "flac" | "ogg" => "🎵",
        "zip" | "rar" | "7z" | "tar" | "gz" => "🗜",
        "js" | "ts" | "py" | "html" | "css" | "json" | "rs" => "💻",
        "txt" | "md" => "📄",
        "apk" => "📱",
        _ => "📦",
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}



/// WebSocket Actor for tracking presence
struct WsClient {
    ip: String,
    user_agent: String,
    app_state: std::sync::Arc<std::sync::Mutex<AppState>>,
}

impl Actor for WsClient {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let now = chrono::Local::now().format("%H:%M").to_string();
        let mut state = self.app_state.lock().unwrap();
        state
            .connected_devices
            .entry(self.ip.clone())
            .and_modify(|c| {
                c.last_seen = now.clone();
                c.user_agent = self.user_agent.clone();
            })
            .or_insert(ConnectedDevice {
                ip: self.ip.clone(),
                user_agent: self.user_agent.clone(),
                connected_at: now.clone(),
                last_seen: now,
                transfer: 0,
            });
        
        if let Some(handle) = &state.app_handle {
            let _ = handle.emit("device-changed", ());
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        let mut state = self.app_state.lock().unwrap();
        state.connected_devices.remove(&self.ip);
        if let Some(handle) = &state.app_handle {
            let _ = handle.emit("device-changed", ());
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsClient {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        if let Ok(ws::Message::Ping(msg)) = msg {
            ctx.pong(&msg);
        }
    }
}

/// Handle WebSocket connections
async fn ws_index(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<std::sync::Arc<std::sync::Mutex<AppState>>>,
) -> Result<HttpResponse, Error> {
    let ip = get_client_ip(&req);
    let ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Unknown")
        .to_string();

    let client = WsClient {
        ip,
        user_agent: ua,
        app_state: data.get_ref().clone(),
    };
    ws::start(client, &req, stream)
}
