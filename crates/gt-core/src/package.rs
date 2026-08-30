use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::error::{Error, Result};
use crate::model::{FillKind, GtDocument, flatten_objects};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Gtzip,
    Gtxml,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceIndex {
    /// Normalized source path -> zip entry name (usually a GUID).
    pub path_to_entry: BTreeMap<String, String>,
    /// Normalized source path -> all frame zip entries for an image sequence.
    pub path_to_sequence: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub kind: PackageKind,
    pub path: PathBuf,
    pub document_xml: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub resources: ResourceIndex,
}

impl Package {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let bytes = tokio::fs::read(&path).await?;
        match ext.as_str() {
            "gtzip" | "zip" => {
                tokio::task::spawn_blocking(move || Self::from_zip_bytes(path, &bytes)).await?
            }
            "gtxml" | "xml" => Self::from_xml_bytes(path, &bytes, PackageKind::Gtxml),
            _ => Err(Error::UnsupportedInput { path }),
        }
    }

    pub fn from_xml_bytes(path: PathBuf, bytes: &[u8], kind: PackageKind) -> Result<Self> {
        Ok(Self {
            kind,
            path,
            document_xml: decode_xml_bytes(bytes)?,
            files: BTreeMap::new(),
            resources: ResourceIndex::default(),
        })
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
        let resources = parse_resources(files.get("resources.xml").or_else(|| {
            files.iter().find_map(|(name, bytes)| {
                Path::new(name)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| name.eq_ignore_ascii_case("resources.xml"))
                    .map(|_| bytes)
            })
        }));
        Ok(Self {
            kind: PackageKind::Gtzip,
            path,
            document_xml,
            files,
            resources,
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

    pub async fn load_external_images(&mut self, document: &GtDocument) -> Result<()> {
        if self.kind != PackageKind::Gtxml {
            return Ok(());
        }
        let base = self.path.parent().unwrap_or(Path::new("."));
        for source in collect_image_sources(document) {
            if self.lookup_bytes(&source).is_some() {
                continue;
            }
            let candidates = [
                base.join(&source),
                base.join(source.replace('\\', "/")),
                base.join(
                    Path::new(&source)
                        .file_name()
                        .unwrap_or_else(|| source.as_ref()),
                ),
            ];
            for candidate in candidates {
                if let Ok(bytes) = tokio::fs::read(&candidate).await {
                    self.files.insert(source.clone(), bytes);
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn lookup_entry(&self, source: &str) -> Option<&str> {
        let key = normalize_source(source);
        self.resources
            .path_to_entry
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.files
                    .keys()
                    .find(|name| normalize_source(name) == key)
                    .map(String::as_str)
            })
            .or_else(|| {
                let file_name = Path::new(source)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(normalize_source);
                file_name.and_then(|name| {
                    self.files
                        .keys()
                        .find(|entry| {
                            Path::new(entry.as_str())
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|entry_name| normalize_source(entry_name) == name)
                        })
                        .map(String::as_str)
                })
            })
    }

    pub fn lookup_bytes(&self, source: &str) -> Option<&[u8]> {
        if let Some(entry) = self.lookup_entry(source)
            && let Some(bytes) = self.files.get(entry)
        {
            return Some(bytes.as_slice());
        }
        self.files.get(source).map(Vec::as_slice).or_else(|| {
            self.files
                .get(&source.replace('\\', "/"))
                .map(Vec::as_slice)
        })
    }

    pub fn sequence_entries(&self, source: &str) -> Vec<String> {
        let key = normalize_source(source);
        if let Some(seq) = self.resources.path_to_sequence.get(&key) {
            return seq.clone();
        }
        self.lookup_entry(source)
            .map(|entry| vec![entry.to_string()])
            .unwrap_or_default()
    }
}

fn collect_image_sources(document: &GtDocument) -> Vec<String> {
    let mut sources = Vec::new();
    for layer in &document.layers {
        for object in flatten_objects(layer) {
            if let Some(source) = &object.image_source {
                sources.push(source.clone());
            }
            if let FillKind::Picture { source, .. } = &object.fill.kind {
                sources.push(source.clone());
            }
            if let FillKind::Picture { source, .. } = &object.stroke.fill.kind {
                sources.push(source.clone());
            }
        }
    }
    sources
}

fn normalize_source(source: &str) -> String {
    source.replace('/', "\\").trim().to_ascii_lowercase()
}

fn parse_resources(bytes: Option<&Vec<u8>>) -> ResourceIndex {
    let Some(bytes) = bytes else {
        return ResourceIndex::default();
    };
    let Ok(xml) = decode_xml_bytes(bytes) else {
        return ResourceIndex::default();
    };
    let mut index = ResourceIndex::default();
    let mut current_filename = None;
    let mut current_frames: Vec<(String, String)> = Vec::new();
    let mut in_resource = false;

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(start) | quick_xml::events::Event::Empty(start)) => {
                let tag = start.local_name().as_ref().to_string();
                if tag.eq_ignore_ascii_case("resource") {
                    in_resource = true;
                    current_filename =
                        start
                            .attributes()
                            .filter_map(|attr| attr.ok())
                            .find_map(|attr| {
                                if attr.key.as_ref().eq_ignore_ascii_case("filename") {
                                    attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .ok()
                                        .map(|value| value.into_owned())
                                } else {
                                    None
                                }
                            });
                    current_frames.clear();
                } else if tag.eq_ignore_ascii_case("source") && in_resource {
                    let guid = start
                        .attributes()
                        .filter_map(|attr| attr.ok())
                        .find_map(|attr| {
                            if attr.key.as_ref().eq_ignore_ascii_case("guid") {
                                attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .ok()
                                    .map(|value| value.into_owned())
                            } else {
                                None
                            }
                        });
                    if let Some(guid) = guid {
                        current_frames.push((guid, String::new()));
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(text)) => {
                if let Some((_, path)) = current_frames.last_mut()
                    && path.is_empty()
                {
                    *path = text.xml10_content().into_owned();
                }
            }
            Ok(quick_xml::events::Event::End(end)) => {
                let tag = end.local_name().as_ref().to_string();
                if tag.eq_ignore_ascii_case("resource") {
                    let seq: Vec<String> = current_frames
                        .iter()
                        .map(|(guid, _)| guid.clone())
                        .collect();
                    if let Some(filename) = &current_filename {
                        index
                            .path_to_sequence
                            .insert(normalize_source(filename), seq.clone());
                        if let Some(first) = seq.first() {
                            index
                                .path_to_entry
                                .insert(normalize_source(filename), first.clone());
                        }
                    }
                    for (guid, path) in &current_frames {
                        let keys = [path.as_str(), guid.as_str()];
                        for key in keys {
                            if !key.is_empty() {
                                index
                                    .path_to_entry
                                    .insert(normalize_source(key), guid.clone());
                                index
                                    .path_to_sequence
                                    .insert(normalize_source(key), seq.clone());
                            }
                        }
                    }
                    in_resource = false;
                    current_filename = None;
                    current_frames.clear();
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    index
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

pub fn sniff_image(bytes: &[u8]) -> (&'static str, &'static str) {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        ("png", "image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        ("jpg", "image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        ("gif", "image/gif")
    } else if bytes.starts_with(b"BM") {
        ("bmp", "image/bmp")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        ("webp", "image/webp")
    } else {
        ("bin", "application/octet-stream")
    }
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
