mod audio;
mod config;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{
    image::Image,
    ipc::Response,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

use audio::{
    build_wav_bytes, list_input_devices, peaks, save_wav, start_capture, AudioCommand,
    AudioControl, DeviceInfo, RingBuffer, TARGET_SAMPLE_RATE,
};
use config::{Config, MAX_BUFFER_SECS, MIN_BUFFER_SECS};

struct AppState {
    buffer: Arc<Mutex<RingBuffer>>,
    control: AudioControl,
    config: Arc<Mutex<Config>>,
    config_path: PathBuf,
}

impl AppState {
    fn persist_config(&self) {
        let cfg = self.config.lock().unwrap().clone();
        if let Err(e) = config::save(&self.config_path, &cfg) {
            eprintln!("[config] save failed: {e}");
        }
    }
}

#[derive(Serialize)]
struct BufferInfo {
    sample_rate: u32,
    capacity_samples: usize,
    capacity_ms: u64,
    current_samples: usize,
    current_ms: u64,
}

#[tauri::command]
fn buffer_info(state: tauri::State<AppState>) -> BufferInfo {
    let buf = state.buffer.lock().unwrap();
    let cap = buf.capacity();
    let cur = buf.len();
    BufferInfo {
        sample_rate: TARGET_SAMPLE_RATE,
        capacity_samples: cap,
        capacity_ms: (cap as u64 * 1000) / TARGET_SAMPLE_RATE as u64,
        current_samples: cur,
        current_ms: (cur as u64 * 1000) / TARGET_SAMPLE_RATE as u64,
    }
}

#[tauri::command]
fn get_peaks(state: tauri::State<AppState>, width: usize) -> Vec<i16> {
    let snapshot = {
        let buf = state.buffer.lock().unwrap();
        buf.snapshot()
    };
    peaks(&snapshot, width)
}

#[tauri::command]
fn get_wav(
    state: tauri::State<AppState>,
    start_sample: usize,
    end_sample: usize,
) -> Result<Response, String> {
    let segment = clip_segment(&state, start_sample, end_sample);
    let bytes = build_wav_bytes(&segment).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

#[tauri::command]
fn save_clip(
    state: tauri::State<AppState>,
    start_sample: usize,
    end_sample: usize,
) -> Result<String, String> {
    let segment = clip_segment(&state, start_sample, end_sample);
    if segment.is_empty() {
        return Err("empty selection".into());
    }
    let dir = state.config.lock().unwrap().save_folder.clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!("{}.wav", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"));
    let path = dir.join(name);
    save_wav(&path, &segment).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn input_devices() -> Vec<DeviceInfo> {
    list_input_devices()
}

#[tauri::command]
fn current_input_device(state: tauri::State<AppState>) -> Option<String> {
    state.control.current()
}

#[tauri::command]
fn set_input_device(
    state: tauri::State<AppState>,
    name: Option<String>,
) -> Result<(), String> {
    state
        .control
        .command_tx
        .send(AudioCommand::SetDevice(name.clone()))
        .map_err(|e| e.to_string())?;
    state.config.lock().unwrap().input_device = name;
    state.persist_config();
    Ok(())
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn set_save_folder(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let p = PathBuf::from(path);
    if p.as_os_str().is_empty() {
        return Err("empty path".into());
    }
    std::fs::create_dir_all(&p).map_err(|e| format!("cannot create folder: {e}"))?;
    state.config.lock().unwrap().save_folder = p;
    state.persist_config();
    Ok(())
}

#[tauri::command]
fn set_buffer_secs(state: tauri::State<AppState>, secs: u32) -> Result<(), String> {
    if !(MIN_BUFFER_SECS..=MAX_BUFFER_SECS).contains(&secs) {
        return Err(format!(
            "buffer must be {MIN_BUFFER_SECS}-{MAX_BUFFER_SECS} seconds"
        ));
    }
    let new_samples = (TARGET_SAMPLE_RATE as usize) * (secs as usize);
    state.buffer.lock().unwrap().resize(new_samples);
    state.config.lock().unwrap().buffer_secs = secs;
    state.persist_config();
    Ok(())
}

#[tauri::command]
fn set_play_tail_secs(state: tauri::State<AppState>, secs: u32) -> Result<(), String> {
    if secs == 0 || secs > 600 {
        return Err("play tail must be 1-600 seconds".into());
    }
    state.config.lock().unwrap().play_tail_secs = secs;
    state.persist_config();
    Ok(())
}

#[tauri::command]
fn open_captures_dir(state: tauri::State<AppState>) -> Result<(), String> {
    let dir = state.config.lock().unwrap().save_folder.clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn clip_segment(state: &tauri::State<AppState>, start: usize, end: usize) -> Vec<i16> {
    let buf = state.buffer.lock().unwrap();
    let snapshot = buf.snapshot();
    let n = snapshot.len();
    let s = start.min(n);
    let e = end.min(n).max(s);
    snapshot[s..e].to_vec()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = app
                .path()
                .app_config_dir()
                .map_err(|e| format!("app_config_dir: {e}"))?
                .join("config.json");
            let cfg = config::load(&config_path);
            let initial_secs = cfg.buffer_secs;
            let initial_device = cfg.input_device.clone();

            let buffer = Arc::new(Mutex::new(RingBuffer::new(
                (TARGET_SAMPLE_RATE as usize) * (initial_secs as usize),
            )));
            let control = start_capture(buffer.clone());

            // Apply persisted device choice (if any).
            if initial_device.is_some() {
                let _ = control
                    .command_tx
                    .send(AudioCommand::SetDevice(initial_device));
            }

            app.manage(AppState {
                buffer,
                control,
                config: Arc::new(Mutex::new(cfg)),
                config_path,
            });

            // Tray.
            let show_item = MenuItem::with_id(app, "show", "Open", true, None::<&str>)?;
            let captures_item =
                MenuItem::with_id(app, "captures", "Open captures folder", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &captures_item, &quit_item])?;

            let icon = tray_icon();
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "captures" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let _ = open_captures_dir(state);
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            buffer_info,
            get_peaks,
            get_wav,
            save_clip,
            open_captures_dir,
            input_devices,
            current_input_device,
            set_input_device,
            get_config,
            set_save_folder,
            set_buffer_secs,
            set_play_tail_secs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn tray_icon() -> Image<'static> {
    static PNG: &[u8] = include_bytes!("../icons/tray.png");
    Image::from_bytes(PNG).expect("valid tray icon PNG")
}
