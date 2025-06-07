use fvrs_core::core::{FileSystem, MonitoringSettings, MonitoringFilter};
use std::path::PathBuf;
use std::env;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("FVRS - File System Monitoring Tool");
        println!("Usage:");
        println!("  {} list [path]        - List files in directory", args[0]);
        println!("  {} monitor [path]     - Monitor directory for changes", args[0]);
        println!("  {} search <pattern>   - Search for files matching pattern", args[0]);
        return Ok(());
    }
    
    let command = &args[1];
    let mut fs = FileSystem::new();
    
    match command.as_str() {
        "list" => {
            let path = if args.len() > 2 {
                PathBuf::from(&args[2])
            } else {
                std::env::current_dir()?
            };
            
            println!("Listing files in: {}", path.display());
            println!("{:<30} {:<15} {:<20} {}", "Name", "Size", "Modified", "Type");
            println!("{:-<75}", "");
            
            match fs.list_files(Some(path)).await {
                Ok(entries) => {
                    for entry in entries {
                        let size_str = if entry.is_dir {
                            "<DIR>".to_string()
                        } else {
                            format_size(entry.size)
                        };
                        
                        let type_str = if entry.is_dir { "Directory" } else { "File" };
                        
                        println!("{:<30} {:<15} {:<20} {}", 
                            entry.name, 
                            size_str,
                            entry.modified.format("%Y-%m-%d %H:%M:%S"),
                            type_str
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error listing files: {}", e);
                }
            }
        }
        
        "monitor" => {
            let path = if args.len() > 2 {
                PathBuf::from(&args[2])
            } else {
                std::env::current_dir()?
            };
            
            println!("Monitoring: {}", path.display());
            println!("Press Ctrl+C to stop...");
            println!("{:-<60}", "");
            
            let settings = MonitoringSettings {
                path: path.clone(),
                recursive: true,
                filter: MonitoringFilter::new(),
                max_history: 1000,
                debounce_ms: 100,
            };
            
            if let Err(e) = fs.start_monitoring_with_settings(settings).await {
                eprintln!("Error starting monitoring: {}", e);
                return Ok(());
            }
            
            // イベントを監視
            loop {
                if let Some(event) = fs.next_event() {
                    println!("[{}] {:?}: {}", 
                        event.timestamp.format("%H:%M:%S"),
                        event.event_type,
                        event.path.display()
                    );
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
        
        "search" => {
            if args.len() < 3 {
                eprintln!("Error: Pattern required for search command");
                return Ok(());
            }
            
            let pattern = &args[2];
            
            println!("Searching for files matching: {}", pattern);
            println!("{:-<50}", "");
            
            match fs.find_files(pattern).await {
                Ok(entries) => {
                    if entries.is_empty() {
                        println!("No files found matching pattern: {}", pattern);
                    } else {
                        for entry in entries {
                            let size_str = if entry.is_dir {
                                "<DIR>".to_string()
                            } else {
                                format_size(entry.size)
                            };
                            
                            println!("{} ({})", entry.path.display(), size_str);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error searching files: {}", e);
                }
            }
        }
        
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Use 'list', 'monitor', or 'search'");
        }
    }
    
    Ok(())
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