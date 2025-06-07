use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use fvrs_core::core::{FileSystem, MonitoringSettings, MonitoringFilter};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    
    // 初期化
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ui.set_current_path(current_dir.to_string_lossy().to_string().into());
    
    // ファイル一覧のモデル
    let files_model = Rc::new(VecModel::<FileEntry>::default());
    ui.set_files(ModelRc::from(files_model.clone()));
    
    // イベント一覧のモデル
    let events_model = Rc::new(VecModel::<MonitoringEvent>::default());
    ui.set_events(ModelRc::from(events_model.clone()));
    
    // ファイルシステムインスタンス
    let mut fs = FileSystem::new();
    
    // ディレクトリ変更コールバック
    let ui_weak = ui.as_weak();
    let files_model_clone = files_model.clone();
    ui.on_change_directory(move |path| {
        let ui = ui_weak.unwrap();
        let path_str = path.to_string();
        ui.set_current_path(path.into());
        
        // 新しいディレクトリのファイル一覧を取得
        tokio::spawn(async move {
            let mut fs = FileSystem::new();
            if let Ok(entries) = fs.list_files(Some(PathBuf::from(&path_str))).await {
                let mut file_entries = Vec::new();
                for entry in entries {
                    file_entries.push(FileEntry {
                        name: entry.name.into(),
                        size: if entry.is_dir { 
                            "<DIR>".into() 
                        } else { 
                            format_size(entry.size).into() 
                        },
                        modified: entry.modified.format("%Y-%m-%d %H:%M").to_string().into(),
                        is_dir: entry.is_dir,
                    });
                }
                
                // UIスレッドでモデルを更新
                slint::invoke_from_event_loop(move || {
                    files_model_clone.set_vec(file_entries);
                }).unwrap();
            }
        });
    });
    
    // ファイル一覧更新コールバック
    let ui_weak = ui.as_weak();
    let files_model_clone = files_model.clone();
    ui.on_refresh_files(move || {
        let ui = ui_weak.unwrap();
        let path_str = ui.get_current_path().to_string();
        let files_model = files_model_clone.clone();
        
        tokio::spawn(async move {
            let mut fs = FileSystem::new();
            if let Ok(entries) = fs.list_files(Some(PathBuf::from(&path_str))).await {
                let mut file_entries = Vec::new();
                for entry in entries {
                    file_entries.push(FileEntry {
                        name: entry.name.into(),
                        size: if entry.is_dir { 
                            "<DIR>".into() 
                        } else { 
                            format_size(entry.size).into() 
                        },
                        modified: entry.modified.format("%Y-%m-%d %H:%M").to_string().into(),
                        is_dir: entry.is_dir,
                    });
                }
                
                slint::invoke_from_event_loop(move || {
                    files_model.set_vec(file_entries);
                }).unwrap();
            }
        });
    });
    
    // 監視開始コールバック
    let ui_weak = ui.as_weak();
    let events_model_clone = events_model.clone();
    ui.on_start_monitoring(move || {
        let ui = ui_weak.unwrap();
        let path_str = ui.get_current_path().to_string();
        let events_model = events_model_clone.clone();
        
        ui.set_monitoring_active(true);
        
        tokio::spawn(async move {
            let mut fs = FileSystem::new();
            let settings = MonitoringSettings {
                path: PathBuf::from(&path_str),
                recursive: true,
                filter: MonitoringFilter::new(),
                max_history: 1000,
                debounce_ms: 100,
            };
            
            if let Ok(_) = fs.start_monitoring_with_settings(settings).await {
                // 監視イベントを受信してUIに表示
                loop {
                    if let Some(event) = fs.next_event() {
                        let monitoring_event = MonitoringEvent {
                            event_type: format!("{:?}", event.event_type).into(),
                            path: event.path.to_string_lossy().to_string().into(),
                            timestamp: event.timestamp.format("%H:%M:%S").to_string().into(),
                        };
                        
                        let events_model = events_model.clone();
                        slint::invoke_from_event_loop(move || {
                            events_model.push(monitoring_event);
                            // 最新1000件まで保持
                            if events_model.row_count() > 1000 {
                                events_model.remove(0);
                            }
                        }).unwrap();
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        });
    });
    
    // 監視停止コールバック
    let ui_weak = ui.as_weak();
    ui.on_stop_monitoring(move || {
        let ui = ui_weak.unwrap();
        ui.set_monitoring_active(false);
        // TODO: 監視停止の実装
    });
    
    // 初回ファイル一覧読み込み
    ui.invoke_refresh_files();
    
    ui.run()
}

fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
} 