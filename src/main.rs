use std::path::Path;

use clap::{ CommandFactory, Parser };
use env_logger::Builder;
use log::LevelFilter;

mod types;
mod env;
mod resolve;
mod http;
mod workflow;

/// A tool for running REST workflows
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The workflow file to execute
    #[arg(value_name = "FILENAME")]
    workflow_file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger with Info as default only if RUST_LOG is not set
    let mut builder = Builder::from_default_env();
    if std::env::var("RUST_LOG").is_err() {
        builder.filter_level(LevelFilter::Info);
    }
    builder.format_timestamp_millis().init();

    let args = Args::parse();

    let workflow_file = args.workflow_file.unwrap_or_else(|| "workflow.yaml".to_string());
    if !Path::new(&workflow_file).exists() {
        println!("{}", Args::command().render_help());
        return Err(format!("Workflow file '{}' (default) not found", workflow_file).into());
    }
    workflow::execute_workflow(&workflow_file).await?;

    Ok(())
}
