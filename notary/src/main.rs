use anyhow::{bail, Context, Result};
use clap::Parser;
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Compute default host notary dir (current working directory).
fn default_host_notary_dir() -> String {
    env!("CARGO_MANIFEST_DIR").to_string()
}

/// CLI surface (env-aware). Long flags + .env supported.
#[derive(Parser, Debug)]
#[command(
    name = "notary-runner",
    about = "Run TLSNotary notary-server via Docker"
)]
struct Args {
    /// Host TCP port for the notary (binds 127.0.0.1:<port> -> 7047)
    #[arg(long = "port", env = "NOTARY_PORT", default_value = "7047")]
    host_port: u16,

    /// TLSN version tag (e.g., "v0.8.1"); empty -> "latest"
    #[arg(long = "version", env = "NOTARY_VERSION", default_value = "")]
    version: String,

    /// Host directory to mount as /root/.notary (must contain config.yaml)
    /// Defaults to current directory.
    #[arg(long = "host-notary-dir", env = "NOTARY_HOST_DIR", default_value_t = default_host_notary_dir())]
    host_notary_dir: String,

    /// Print the docker command and exit
    #[arg(long = "dry-run", hide = true)]
    dry_run: bool,

    /// Make the mount read-only
    #[arg(long = "readonly", default_value_t = true)]
    readonly: bool,
}

fn main() -> Result<()> {
    // Load .env first so clap's env defaults see it.
    let _ = dotenvy::dotenv();

    let args = Args::parse();

    let version = if args.version.trim().is_empty() {
        "latest".to_string()
    } else {
        args.version.trim().to_string()
    };

    // Validate config.yaml presence to avoid "Is a directory" or missing-file errors.
    let cfg_file = PathBuf::from(args.host_notary_dir);
    validate_config_file(&cfg_file.join("config.yaml"))?;

    // Compose image from version
    let image = format!("ghcr.io/tlsnotary/tlsn/notary-server:{version}");

    // Build docker command
    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("--init")
        .arg("--rm")
        .args(["-p", &format!("{}:7047", args.host_port)]);

    let mut vol = format!("{}:/root/.notary", display_path(&cfg_file));
    if args.readonly {
        vol.push_str(":ro");
    }
    cmd.args(["-v", &vol]);

    cmd.arg(&image)
        .args(["--config", "/root/.notary/config.yaml"]);

    if args.dry_run {
        println!("(dry-run) would exec: {:?}", cmd);
        return Ok(());
    }

    let status = cmd
        .status()
        .with_context(|| "failed to execute docker; is Docker installed and running?")?;

    if !status.success() {
        bail!("docker exited with non-zero status {}", status);
    }

    Ok(())
}

fn validate_config_file(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!(
            "Missing config file: {}\nExpected at: {}",
            "config.yaml",
            display_path(path)
        );
    }
    if !path.is_file() {
        bail!(
            "Expected a file at {}, but found a directory or special node",
            display_path(path)
        );
    }
    Ok(())
}

fn display_path<P: AsRef<Path>>(p: P) -> String {
    p.as_ref().to_string_lossy().into_owned()
}
