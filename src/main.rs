use env_logger::Builder;
use log::LevelFilter;

mod types;
mod env;
mod resolve;
mod http;
mod workflow;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger with Info as default only if RUST_LOG is not set
    let mut builder = Builder::from_default_env();
    if std::env::var("RUST_LOG").is_err() {
        builder.filter_level(LevelFilter::Info);
    }
    builder.format_timestamp_millis().init();

    workflow::execute_workflow("workflow.yml").await?;
    Ok(())
}
