//! Convert vMix GT Title Designer packages into an intermediate document and HTML.

pub mod error;
pub mod model;
pub mod package;
pub mod parse;
pub mod render;
pub mod resolve;
pub mod warn;

use std::path::Path;

pub use error::{Error, Result};
pub use model::{GtDocument, InspectReport};
pub use package::Package;
pub use warn::Warning;

#[derive(Debug, Clone)]
pub struct Conversion {
    pub document: GtDocument,
    pub html: String,
    pub warnings: Vec<Warning>,
}

pub async fn convert_path(path: impl AsRef<Path>) -> Result<Conversion> {
    let package = Package::open(path).await?;
    convert_package(&package)
}

pub fn convert_package(package: &Package) -> Result<Conversion> {
    let mut document = parse::parse_document(&package.document_xml)?;
    document.asset_names = package.asset_names();
    let warnings = resolve::collect_warnings(&document);
    document.warnings = warnings.clone();
    let html = render::html::render(&document);
    Ok(Conversion {
        document,
        html,
        warnings,
    })
}

pub async fn inspect_path(path: impl AsRef<Path>) -> Result<InspectReport> {
    Ok(convert_path(path).await?.document.inspect_report())
}
