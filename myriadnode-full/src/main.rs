use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Use library modules
use myriadnode::{Config, Node};

/// MyriadNode - Multi-Network Communication Node
#[derive(Parser, Debug)]
#[command(name = "myriadnode")]
#[command(author = "MyriadMesh Contributors")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Multi-network communication aggregation daemon", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON log output
    #[arg(long)]
    json_logs: bool,

    /// Data directory
    #[arg(short, long, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Initialize node (generate keys, create config)
    #[arg(long)]
    init: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level, cli.json_logs)?;

    info!("MyriadNode v{} starting", env!("CARGO_PKG_VERSION"));

    // Load or create configuration
    let config = if cli.init {
        info!("Initializing new node...");
        let config = Config::create_default(cli.config, cli.data_dir)?;
        info!("Node initialized successfully!");
        info!("Node ID: {}", hex::encode(&config.node.id));
        info!("Config saved to: {}", config.config_path().display());
        return Ok(());
    } else {
        Config::load(cli.config, cli.data_dir)?
    };

    info!(
        "Loaded configuration from: {}",
        config.config_path().display()
    );
    info!("Node ID: {}", hex::encode(&config.node.id));
    info!("Node name: {}", config.node.name);

    // Create and start node
    let mut node = Node::new(config).await?;

    info!("MyriadNode initialized, starting services...");

    // Run the node (blocks until shutdown signal)
    if let Err(e) = node.run().await {
        error!("Node error: {}", e);
        return Err(e);
    }

    info!("MyriadNode shutdown complete");
    Ok(())
}

fn init_logging(level: &str, json: bool) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    if json {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    Ok(())
}
