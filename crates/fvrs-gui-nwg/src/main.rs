use fvrs_core::init as init_core;
use fvrs_gui_nwg::{init as init_gui, run as run_gui};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize core runtime
    init_core().await?;
    
    // Initialize and run GUI
    init_gui()?;
    run_gui()?;
    
    Ok(())
} 