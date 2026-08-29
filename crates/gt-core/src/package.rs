use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Gtzip,
    Gtxml,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub kind: PackageKind,
    pub path: PathBuf,
    pub document_xml: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Package {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "gtzip" | "zip" => Self::open_gtzip(path),
            "gtxml" | "xml" => Self::open_gtxml(path),
            _ => Err(Error::UnsupportedInput {
                path: path.to_path_buf(),
            }),
        }
    }

    pub fn from_xml_bytes(path: PathBuf, bytes: &[u8], kind: PackageKind) -> Result<Self> {
        Ok(Self {
            kind,
            path,
            document_xml: decode_xml_bytes(bytes)?,
            files: BTreeMap::new(),
        })
    }

    fn open_gtxml(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_xml_bytes(path.to_path_buf(), &bytes, PackageKind::Gtxml)
    }

    fn open_gtzip(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_zip_bytes(path.to_path_buf(), &bytes)
    }

    pub fn from_zip_bytes(path: PathBuf, bytes: &[u8]) -> Result<Self> {
        let cursor = Cursor::new(bytes.to_vec());
        let mut archive = ZipArchive::new(cursor)?;
        let mut files = BTreeMap::new();
        let mut document_xml = None;

        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            if file.is_dir() {
                continue;
            }
            let name = file.name().replace('\\', "/");
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            if is_document_xml(&name) {
                document_xml = Some(decode_xml_bytes(&buf)?);
            }
            files.insert(name, buf);
        }

        let document_xml = document_xml.ok_or(Error::MissingDocumentXml)?;
        Ok(Self {
            kind: PackageKind::Gtzip,
            path,
            document_xml,
            files,
        })
    }

    pub fn asset_names(&self) -> Vec<String> {
        self.files
            .keys()
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                !is_document_xml(name)
                    && lower != "resources.xml"
                    && lower != "[content_types].xml"
                    && !lower.ends_with('/')
            })
            .cloned()
            .collect()
    }
}

fn is_document_xml(name: &str) -> bool {
    Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("document.xml"))
}

pub fn decode_xml_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        decode_utf16(bytes, Endian::Little, true)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        decode_utf16(bytes, Endian::Big, true)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Ok(std::str::from_utf8(&bytes[3..])?.to_string())
    } else if looks_like_utf16_le(bytes) {
        decode_utf16(bytes, Endian::Little, false)
    } else if looks_like_utf16_be(bytes) {
        decode_utf16(bytes, Endian::Big, false)
    } else {
        Ok(std::str::from_utf8(bytes)?.to_string())
    }
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == b'<' && bytes[1] == 0
}

fn looks_like_utf16_be(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0 && bytes[1] == b'<'
}

fn decode_utf16(bytes: &[u8], endian: Endian, bom: bool) -> Result<String> {
    let rest = if bom { &bytes[2..] } else { bytes };
    if rest.len() % 2 != 0 {
        return Err(Error::Utf16);
    }
    let units: Vec<u16> = rest
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| match endian {
            Endian::Little => u16::from_le_bytes(*chunk),
            Endian::Big => u16::from_be_bytes(*chunk),
        })
        .collect();
    String::from_utf16(&units).map_err(|_| Error::Utf16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_and_utf16() {
        let xml = "<Composition Width=\"1\" Height=\"1\"/>";
        assert_eq!(decode_xml_bytes(xml.as_bytes()).unwrap(), xml);

        let mut utf16 = vec![0xFF, 0xFE];
        for unit in xml.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_xml_bytes(&utf16).unwrap(), xml);
    }
}
