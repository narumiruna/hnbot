use clap::Parser;
use hnbot::app::App;
use hnbot::cli::{Cli, Command};
use hnbot::config::Settings;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let cli = Cli::parse();
    let settings = Settings::from_env()?;

    match cli.command {
        Command::Serve { poll_interval } => {
            let interval = poll_interval.unwrap_or(settings.feed_poll_interval_seconds);
            App::production(settings).await?.serve(interval).await?;
        }
    }

    Ok(())
}
