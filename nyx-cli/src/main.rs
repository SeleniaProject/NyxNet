#![forbid(unsafe_code)]

//! Nyx command line tool.
//!
//! Currently implements a single subcommand `errors` which fetches the Prometheus
//! metrics endpoint and outputs aggregated Nyx extended error codes in a
//! human-readable table.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroizing;
use nyx_crypto::keystore::{encrypt_and_store, KeystoreError};
use dirs::home_dir;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufRead, BufReader};
use std::path::PathBuf;
use rpassword::prompt_password;
mod i18n;
use i18n::tr;
use fluent_bundle::FluentArgs;

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
    /// Rotate long-term node secret key.
    Key {
        #[command(subcommand)]
        op: KeyCmd,
    },

    /// Add or manage quarantine list.
    Quarantine {
        /// Node ID (hex) to quarantine.
        node: String,
    },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Rotate secret key and overwrite keystore file.
    Rotate {
        /// Path to keystore file (.age). Default: ~/.nyx/keys.json.age
        #[arg(long)]
        path: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Errors { endpoint } => cmd_errors(&endpoint),
        Commands::Key { op } => match op {
            KeyCmd::Rotate { path } => cmd_key_rotate(path),
        },
        Commands::Quarantine { node } => cmd_quarantine(&node),
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

/// Implementation of `nyx key rotate`.
fn cmd_key_rotate(path_opt: Option<String>) -> Result<()> {
    // Determine path
    let default_path = || {
        let mut p = home_dir().expect("home dir");
        p.push(".nyx/keys.json.age");
        p.to_string_lossy().to_string()
    };
    let path_str = path_opt.unwrap_or_else(default_path);
    let path = PathBuf::from(&path_str);

    // Prompt passphrase
    let passphrase = prompt_password("Enter passphrase for keystore: ")?;
    if passphrase.trim().is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }

    // Generate new 32-byte secret key
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = Zeroizing::new(bytes.to_vec());

    encrypt_and_store(&secret, &path, &passphrase).map_err(|e| match e {
        KeystoreError::Io(ioe) => ioe.into(),
        other => anyhow::anyhow!(other),
    })?;

    println!("{}", tr("rotate-success", None));
    Ok(())
}

/// Implementation of `nyx quarantine <node>`.
fn cmd_quarantine(node: &str) -> Result<()> {
    let mut list_path = home_dir().expect("home");
    list_path.push(".nyx/quarantine.list");
    if let Some(parent) = list_path.parent() { fs::create_dir_all(parent)?; }

    // Load existing
    let mut existing = Vec::new();
    if list_path.exists() {
        let file = fs::File::open(&list_path)?;
        for line in BufReader::new(file).lines() {
            if let Ok(l) = line { existing.push(l); }
        }
    }

    if existing.iter().any(|e| e == node) {
        let mut args = FluentArgs::new();
        args.set("node", node);
        println!("{}", tr("quarantine-duplicate", Some(&args)));
        return Ok(());
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&list_path)?;
    writeln!(file, "{}", node)?;

    let mut args = FluentArgs::new();
    args.set("node", node);
    println!("{}", tr("quarantine-added", Some(&args)));
    Ok(())
} 