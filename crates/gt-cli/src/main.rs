use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use gt_core::fields::list_fields;
use gt_core::schema::{AUTHORING_SCHEMA_JSON, FORMAT_SUMMARY};
use gt_core::write::{WriteAssets, write_gtzip_path};
use gt_core::{ConvertOptions, Package, convert_package_with, convert_path_with, inspect_path};

#[derive(Debug, Parser)]
#[command(
    name = "gt-parser",
    version,
    about = "Parse, preview, and write vMix GT Title Designer files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert a GT title into HTML
    Convert {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Html)]
        format: OutputFormat,
        #[arg(long)]
        embed_assets: bool,
        #[arg(long, default_value = "TransitionIn")]
        storyboard: String,
    },
    /// Print a JSON summary of the parsed title
    Inspect {
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Write a .gtzip from a title or authoring IR JSON
    Pack {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Extra assets as name=path
        #[arg(long = "asset", value_name = "NAME=PATH")]
        assets: Vec<String>,
    },
    /// List vMix data fields
    Fields { input: PathBuf },
    /// Print the authoring JSON Schema
    Schema,
    /// Write a self-contained HTML preview
    Preview {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run the MCP server on stdio
    Mcp,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Html,
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Convert {
            input,
            output,
            format: OutputFormat::Html,
            embed_assets,
            storyboard,
        } => convert(&input, output, embed_assets, storyboard).await,
        Command::Inspect { input, json: _ } => inspect(&input).await,
        Command::Pack {
            input,
            output,
            assets,
        } => pack(&input, &output, &assets).await,
        Command::Fields { input } => fields(&input).await,
        Command::Schema => {
            println!("{FORMAT_SUMMARY}");
            println!("{AUTHORING_SCHEMA_JSON}");
            Ok(())
        }
        Command::Preview { input, output } => {
            convert(&input, output, true, "TransitionIn".to_string()).await
        }
        Command::Mcp => gt_mcp::run_stdio()
            .await
            .map_err(|error| anyhow::anyhow!("{error}")),
    }
}

async fn convert(
    input: &Path,
    output: Option<PathBuf>,
    embed_assets: bool,
    storyboard: String,
) -> Result<()> {
    let conversion = convert_path_with(
        input,
        ConvertOptions {
            embed_assets,
            storyboard,
        },
    )
    .await
    .with_context(|| format!("failed to convert {}", input.display()))?;
    let outdir = output.unwrap_or_else(|| default_output_dir(input));
    tokio::fs::create_dir_all(&outdir)
        .await
        .with_context(|| format!("failed to create {}", outdir.display()))?;
    for asset in &conversion.assets {
        let dest = outdir.join(&asset.relative_path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&dest, &asset.bytes)
            .await
            .with_context(|| format!("failed to write {}", dest.display()))?;
    }
    tokio::fs::write(outdir.join("index.html"), conversion.html.as_bytes())
        .await
        .with_context(|| format!("failed to write {}", outdir.join("index.html").display()))?;
    tokio::fs::write(
        outdir.join("warnings.json"),
        serde_json::to_vec_pretty(&conversion.warnings)?,
    )
    .await
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

async fn inspect(input: &Path) -> Result<()> {
    let report = inspect_path(input)
        .await
        .with_context(|| format!("failed to inspect {}", input.display()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn fields(input: &Path) -> Result<()> {
    let conversion = convert_path_with(input, ConvertOptions::default())
        .await
        .with_context(|| format!("failed to read {}", input.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&list_fields(&conversion.document))?
    );
    Ok(())
}

async fn pack(input: &Path, output: &Path, extra: &[String]) -> Result<()> {
    let (document, mut assets) = load_authoring(input).await?;
    for spec in extra {
        let (name, path) = spec
            .split_once('=')
            .with_context(|| format!("asset must be name=path, got {spec}"))?;
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("failed to read {path}"))?;
        assets.insert(name, bytes);
    }
    write_gtzip_path(output, &document, &assets)
        .await
        .with_context(|| format!("failed to write {}", output.display()))?;
    println!("wrote {}", output.display());
    Ok(())
}

async fn load_authoring(input: &Path) -> Result<(gt_core::GtDocument, WriteAssets)> {
    let ext = input
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "json" {
        let text = tokio::fs::read_to_string(input).await?;
        let document: gt_core::GtDocument = serde_json::from_str(&text)?;
        return Ok((document, WriteAssets::default()));
    }
    let mut package = Package::open(input).await?;
    let document = gt_core::parse::parse_document(&package.document_xml)?;
    package.load_external_images(&document).await?;
    let conversion = convert_package_with(&package, ConvertOptions::default(), Some(document))?;
    Ok((conversion.document, WriteAssets::from_package(&package)))
}

fn default_output_dir(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("gt-title");
    PathBuf::from(format!("{stem}_html"))
}
