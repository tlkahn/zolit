mod cli;
mod dsl;
mod error;
mod html;
mod matching;
mod zotero;

use clap::Parser;
use cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

fn main() {
    let cli = Cli::parse();

    let filter = match cli.verbose {
        0 => "zolit=warn",
        1 => "zolit=info",
        2 => "zolit=debug",
        _ => "zolit=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .init();

    let result = match cli.command {
        Command::Sync => cmd_sync(&cli),
        Command::List => cmd_list(&cli),
        Command::Status => cmd_status(&cli),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_sync(_cli: &Cli) -> error::Result<()> {
    eprintln!("sync: not yet implemented");
    Ok(())
}

fn cmd_list(_cli: &Cli) -> error::Result<()> {
    eprintln!("list: not yet implemented");
    Ok(())
}

fn cmd_status(_cli: &Cli) -> error::Result<()> {
    eprintln!("status: not yet implemented");
    Ok(())
}
