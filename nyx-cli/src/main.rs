#![forbid(unsafe_code)]

//! Nyx command line tool.
//!
//! Currently implements a single subcommand `errors` which fetches the Prometheus
//! metrics endpoint and outputs aggregated Nyx extended error codes in a
//! human-readable table.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;

#[derive(Parser)]
#[command(name = "nyx" )]
#[command(about = "Nyx command line interface", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetch error code statistics from Prometheus exporter.
    Errors {
        /// Base URL of exporter (e.g. http://localhost:9090)
        #[arg(long, default_value = "http://localhost:9090")] 
        endpoint: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Errors { endpoint } => cmd_errors(&endpoint),
    }
}

/// Implementation of `nyx errors`.
fn cmd_errors(base: &str) -> Result<()> {
    let url = format!("{}/metrics", base.trim_end_matches('/'));
    let body = reqwest::blocking::get(&url)
        .with_context(|| format!("request metrics from {url}"))?
        .text()?;

    // Regex capturing code label and value.
    let re = Regex::new(r#"nyx_error_total\{code="([0-9a-f]{4})"\}\s+(\d+)"#)?;
    println!("Error Code | Description | Count\n-----------|-------------|------");
    fn desc(code: &str) -> &str {
        match code {
            "0007" => "UNSUPPORTED_CAP",
            "0004" => "VERSION_MISMATCH",
            "0005" => "PATH_VALIDATION_FAILED",
            _ => "UNKNOWN",
        }
    }
    for cap in re.captures_iter(&body) {
        let code = &cap[1];
        let count = &cap[2];
        println!("0x{code}     | {:<15} | {count}", desc(code));
    }
    Ok(())
} 