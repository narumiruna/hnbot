use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "hnbot", about = "Run the Hacker News summary service")]
#[command(arg_required_else_help = true, disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Continuously poll the feed and process unseen entries.
    Serve {
        /// Seconds to wait between completed feed batches; overrides configuration.
        #[arg(long, value_parser = parse_poll_interval)]
        poll_interval: Option<f64>,
    },
}

fn parse_poll_interval(raw: &str) -> Result<f64, String> {
    let value = raw.parse::<f64>().map_err(|error| error.to_string())?;
    if value.is_finite() && value >= 1.0 {
        Ok(value)
    } else {
        Err("poll interval must be finite and at least one second".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn serve_accepts_valid_override() {
        let cli = Cli::try_parse_from(["hnbot", "serve", "--poll-interval", "5"]).unwrap();
        let Command::Serve { poll_interval } = cli.command;
        assert_eq!(poll_interval, Some(5.0));
    }

    #[test]
    fn bare_command_and_main_are_rejected() {
        assert!(Cli::try_parse_from(["hnbot"]).is_err());
        assert!(Cli::try_parse_from(["hnbot", "main"]).is_err());
    }

    #[test]
    fn invalid_override_is_rejected() {
        assert!(Cli::try_parse_from(["hnbot", "serve", "--poll-interval", "0.5"]).is_err());
    }
}
