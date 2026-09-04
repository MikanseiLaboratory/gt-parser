use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to read zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("XML attribute error: {0}")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),

    #[error("invalid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("invalid UTF-8: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("invalid UTF-16 in XML")]
    Utf16,

    #[error("unsupported input type: {path}")]
    UnsupportedInput { path: PathBuf },

    #[error("GT package is missing document.xml")]
    MissingDocumentXml,

    #[error("root element is not Composition")]
    MissingComposition,

    #[error("unexpected end of XML while parsing <{0}>")]
    UnexpectedEof(&'static str),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[cfg(feature = "fs")]
    #[error("tokio task join error: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
