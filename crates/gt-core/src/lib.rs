//! Convert vMix GT Title Designer packages into an intermediate document and HTML.

pub mod anim;
pub mod edit;
pub mod error;
pub mod fields;
pub mod model;
pub mod package;
pub mod parse;
pub mod render;
pub mod resolve;
pub mod schema;
pub mod warn;
#[cfg(feature = "write")]
pub mod write;

#[cfg(feature = "fs")]
use std::path::Path;

pub use error::{Error, Result};
pub use fields::DataField;
pub use model::{GtDocument, InspectReport};
pub use package::Package;
pub use warn::Warning;
#[cfg(feature = "write")]
pub use write::{WriteAssets, serialize_document_xml, write_gtzip_bytes};

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub embed_assets: bool,
    pub storyboard: String,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            embed_assets: false,
            storyboard: "TransitionIn".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputAsset {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Conversion {
    pub document: GtDocument,
    pub html: String,
    pub warnings: Vec<Warning>,
    pub assets: Vec<OutputAsset>,
}

#[cfg(feature = "fs")]
pub async fn convert_path(path: impl AsRef<Path>) -> Result<Conversion> {
    convert_path_with(path, ConvertOptions::default()).await
}

#[cfg(feature = "fs")]
pub async fn convert_path_with(
    path: impl AsRef<Path>,
    options: ConvertOptions,
) -> Result<Conversion> {
    let mut package = Package::open(path).await?;
    let document = parse::parse_document(&package.document_xml)?;
    package.load_external_images(&document).await?;
    convert_package_with(&package, options, Some(document))
}

pub fn convert_package(package: &Package) -> Result<Conversion> {
    convert_package_with(package, ConvertOptions::default(), None)
}

pub fn convert_package_with(
    package: &Package,
    options: ConvertOptions,
    parsed: Option<GtDocument>,
) -> Result<Conversion> {
    let mut document = match parsed {
        Some(document) => document,
        None => parse::parse_document(&package.document_xml)?,
    };
    document.asset_names = package.asset_names();
    resolve::resolve_bounding(&mut document);
    let rendered = render::html::render(&document, package, &options);
    let mut warnings = resolve::collect_warnings(&document, &options);
    warnings.extend(rendered.warnings);
    document.warnings = warnings.clone();
    Ok(Conversion {
        document,
        html: rendered.html,
        warnings,
        assets: rendered.assets,
    })
}

#[cfg(feature = "fs")]
pub async fn inspect_path(path: impl AsRef<Path>) -> Result<InspectReport> {
    Ok(convert_path(path).await?.document.inspect_report())
}
