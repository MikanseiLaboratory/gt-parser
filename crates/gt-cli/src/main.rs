use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use gt_core::{convert_path, inspect_path};

#[derive(Debug, Parser)]
#[command(
    name = "gt-parser",
    version,
    about = "Convert vMix GT Title Designer files (.gtzip / .gtxml) to HTML"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert a GT title into HTML
    Convert {
        /// Path to a .gtzip or .gtxml file
        input: PathBuf,
        /// Output directory (default: <input-stem>_html)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Html)]
        format: OutputFormat,
        /// Embed assets as data URIs (phase 2; accepted for CLI compatibility)
        #[arg(long)]
        embed_assets: bool,
    },
    /// Print a JSON summary of the parsed title
    Inspect {
        /// Path to a .gtzip or .gtxml file
        input: PathBuf,
        /// Always emit JSON (this is the default inspect format)
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Html,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Convert {
            input,
            output,
            format: OutputFormat::Html,
            embed_assets,
        } => convert(&input, output, embed_assets),
        Command::Inspect { input, json: _ } => inspect(&input),
    }
}

fn convert(input: &Path, output: Option<PathBuf>, embed_assets: bool) -> Result<()> {
    let mut conversion =
        convert_path(input).with_context(|| format!("failed to convert {}", input.display()))?;
    if embed_assets {
        conversion.warnings.push(gt_core::Warning::new(
            "unsupported.embed_assets",
            "--embed-assets is reserved for phase 2 image embedding",
        ));
    }
    let outdir = output.unwrap_or_else(|| default_output_dir(input));
    fs::create_dir_all(&outdir)
        .with_context(|| format!("failed to create {}", outdir.display()))?;
    fs::write(outdir.join("index.html"), conversion.html.as_bytes())
        .with_context(|| format!("failed to write {}", outdir.join("index.html").display()))?;
    fs::write(
        outdir.join("warnings.json"),
        serde_json::to_vec_pretty(&conversion.warnings)?,
    )
    .with_context(|| format!("failed to write {}", outdir.join("warnings.json").display()))?;
    println!("wrote {}", outdir.join("index.html").display());
    if !conversion.warnings.is_empty() {
        println!(
            "{} warning(s) written to warnings.json",
            conversion.warnings.len()
        );
    }
    Ok(())
}

fn inspect(input: &Path) -> Result<()> {
    let report =
        inspect_path(input).with_context(|| format!("failed to inspect {}", input.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn default_output_dir(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("gt-title");
    PathBuf::from(format!("{stem}_html"))
}
