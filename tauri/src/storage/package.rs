//! Unpacking an extension the user picked: a `.crx` or `.zip` file, or a
//! folder. See [Extensions § Adding and updating](../../../docs/specs/extensions.md#adding-and-updating).
//!
//! The archive is read here rather than through a zip library: an extension
//! needs only stored and deflated files, and [`miniz_oxide`] inflates those.

use std::path::Path;

use sha2::Digest;

use crate::error::{HakuError, Result};
use crate::model::data_url;

const MANIFEST: &str = "manifest.json";

/// The most a picked file, and what it unpacks to, may hold. Extensions are a
/// few megabytes; unpacking happens in memory, so a file that declares more
/// is refused before it is read or inflated.
pub const MAX_SIZE: usize = 512 * 1024 * 1024;

/// Reads a picked `.crx` or `.zip`.
///
/// # Errors
/// Fails when it cannot be read, or is larger than [`MAX_SIZE`].
pub fn read(path: &Path) -> Result<Vec<u8>> {
    let length = std::fs::metadata(path)?.len();
    if usize::try_from(length).map_or(true, |length| length > MAX_SIZE) {
        return Err(too_large());
    }
    Ok(std::fs::read(path)?)
}

fn too_large() -> HakuError {
    HakuError::Unsupported(format!(
        "it is larger than {} MB, far more than an extension needs",
        MAX_SIZE / (1024 * 1024)
    ))
}

/// Unpacks a `.crx` or `.zip` into `into`, creating it. A `.crx` signing key
/// is written into the manifest as `key` when it has none, so the extension
/// gets the id it has in the Chrome Web Store.
///
/// # Errors
/// Fails when `bytes` is neither, uses what an extension never needs, such
/// as encryption or ZIP64, names a file outside the folder, unpacks to more
/// than [`MAX_SIZE`], or holds no `manifest.json`; and when writing fails.
pub fn unpack(bytes: &[u8], into: &Path) -> Result<()> {
    let (key, archive) = split_crx(bytes)?;
    let files = top_folder_removed(read_zip(archive, MAX_SIZE)?);
    if !files.iter().any(|(path, _)| path == MANIFEST) {
        return Err(no_manifest());
    }
    for (path, contents) in files {
        let path = into.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
    }
    if let Some(key) = key {
        add_key(&into.join(MANIFEST), &key)?;
    }
    Ok(())
}

/// Copies an unpacked extension's folder into `into`, creating it.
///
/// # Errors
/// Fails when `from` has no `manifest.json`, or copying fails.
pub fn copy_folder(from: &Path, into: &Path) -> Result<()> {
    if !from.join(MANIFEST).is_file() {
        return Err(no_manifest());
    }
    copy_tree(from, into)
}

fn copy_tree(from: &Path, into: &Path) -> Result<()> {
    std::fs::create_dir_all(into)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = into.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The bytes a manifest `key` stands for, decoded from base64. Whitespace is
/// ignored, as Chrome ignores it.
pub fn decode_key(key: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut count = 0;
    let mut bytes = Vec::with_capacity(key.len() * 3 / 4);
    for c in key.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    Some(bytes)
}

fn no_manifest() -> HakuError {
    HakuError::Unsupported("not an extension: it has no manifest.json".into())
}

fn malformed(what: &str) -> HakuError {
    HakuError::Unsupported(format!("not a .crx or .zip extension: {what}"))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?)))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()
}

/// The signing key of a `.crx` and the archive after its header, or no key
/// and the bytes as they are for anything else.
fn split_crx(bytes: &[u8]) -> Result<(Option<Vec<u8>>, &[u8])> {
    if !bytes.starts_with(b"Cr24") {
        return Ok((None, bytes));
    }
    let bad = || malformed("its .crx header is cut short");
    match u32_at(bytes, 4).ok_or_else(bad)? {
        2 => {
            let key_len = u32_at(bytes, 8).ok_or_else(bad)?;
            let signature_len = u32_at(bytes, 12).ok_or_else(bad)?;
            let key = bytes.get(16..16 + key_len).ok_or_else(bad)?;
            let archive = bytes.get(16 + key_len + signature_len..).ok_or_else(bad)?;
            Ok((Some(key.to_vec()), archive))
        }
        3 => {
            let header_len = u32_at(bytes, 8).ok_or_else(bad)?;
            let header = bytes.get(12..12 + header_len).ok_or_else(bad)?;
            let archive = bytes.get(12 + header_len..).ok_or_else(bad)?;
            Ok((crx3_key(header), archive))
        }
        version => Err(malformed(&format!(".crx version {version}"))),
    }
}

/// The key a CRX3 header names the extension by. The header carries a proof
/// for each key that signed it, the store's own included; the extension's is
/// the one whose SHA-256 starts with the `crx_id` it declares.
fn crx3_key(header: &[u8]) -> Option<Vec<u8>> {
    // CrxFileHeader: sha256_with_rsa = 2, sha256_with_ecdsa = 3, each an
    // AsymmetricKeyProof whose public_key = 1; signed_header_data = 10000,
    // a SignedData whose crx_id = 1.
    let fields = protobuf_fields(header)?;
    let keys: Vec<&[u8]> = fields
        .iter()
        .filter(|(number, _)| *number == 2 || *number == 3)
        .filter_map(|(_, proof)| field(proof, 1))
        .collect();
    let crx_id = fields
        .iter()
        .find(|(number, _)| *number == 10000)
        .and_then(|(_, signed)| field(signed, 1))?;
    keys.into_iter()
        .find(|key| sha2::Sha256::digest(key).starts_with(crx_id))
        .map(<[u8]>::to_vec)
}

fn field(message: &[u8], number: u64) -> Option<&[u8]> {
    protobuf_fields(message)?
        .into_iter()
        .find(|(found, _)| *found == number)
        .map(|(_, value)| value)
}

/// The length-delimited fields of a protocol buffer message, by number. Other
/// wire types are skipped.
fn protobuf_fields(mut message: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    fn varint(bytes: &mut &[u8]) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = bytes.split_first()?;
            *bytes = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
    let mut fields = Vec::new();
    while !message.is_empty() {
        let tag = varint(&mut message)?;
        match tag & 7 {
            0 => {
                varint(&mut message)?;
            }
            1 => message = message.get(8..)?,
            2 => {
                let len = usize::try_from(varint(&mut message)?).ok()?;
                fields.push((tag >> 3, message.get(..len)?));
                message = message.get(len..)?;
            }
            5 => message = message.get(4..)?,
            _ => return None,
        }
    }
    Some(fields)
}

/// The files of a ZIP archive, by their path inside it with `/` between
/// folders. Folder entries are left out; writing a file creates its folders.
fn read_zip(archive: &[u8], limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
    const END: &[u8] = &[0x50, 0x4b, 0x05, 0x06];
    const CENTRAL: usize = 0x0201_4b50;
    const LOCAL: usize = 0x0403_4b50;

    // The end record is last, after a comment of up to 64 KiB, which may hold
    // the record's own signature: the record is the one whose comment ends
    // the archive.
    let search_from = archive.len().saturating_sub(22 + usize::from(u16::MAX));
    let end = (search_from..archive.len().saturating_sub(21))
        .rev()
        .find(|&at| archive[at..].starts_with(END) && u16_at(archive, at + 20) == Some(archive.len() - at - 22))
        .ok_or_else(|| malformed("it is not a ZIP archive"))?;
    let bad = || malformed("its ZIP directory is damaged");
    let count = u16_at(archive, end + 10).ok_or_else(bad)?;
    let mut at = u32_at(archive, end + 16).ok_or_else(bad)?;
    if count == usize::from(u16::MAX) || at == u32::MAX as usize {
        return Err(malformed("it is a ZIP64 archive"));
    }

    let mut files = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        if u32_at(archive, at) != Some(CENTRAL) {
            return Err(bad());
        }
        let flags = u16_at(archive, at + 8).ok_or_else(bad)?;
        let method = u16_at(archive, at + 10).ok_or_else(bad)?;
        let compressed = u32_at(archive, at + 20).ok_or_else(bad)?;
        let size = u32_at(archive, at + 24).ok_or_else(bad)?;
        let name_len = u16_at(archive, at + 28).ok_or_else(bad)?;
        let extra_len = u16_at(archive, at + 30).ok_or_else(bad)?;
        let comment_len = u16_at(archive, at + 32).ok_or_else(bad)?;
        let local = u32_at(archive, at + 42).ok_or_else(bad)?;
        let name = archive.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
        at += 46 + name_len + extra_len + comment_len;
        // Counted from what each file declares, before any is inflated.
        total = total.saturating_add(size);
        if total > limit {
            return Err(too_large());
        }

        let name = String::from_utf8_lossy(name).replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        let path = safe_path(&name).ok_or_else(|| malformed(&format!("it names a file outside itself, {name}")))?;
        if flags & 1 != 0 {
            return Err(malformed("it is encrypted"));
        }

        if u32_at(archive, local) != Some(LOCAL) {
            return Err(bad());
        }
        let start =
            local + 30 + u16_at(archive, local + 26).ok_or_else(bad)? + u16_at(archive, local + 28).ok_or_else(bad)?;
        let data = archive.get(start..start + compressed).ok_or_else(bad)?;
        let contents = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size)
                .map_err(|_| malformed(&format!("{path} is damaged")))?,
            _ => return Err(malformed(&format!("{path} is compressed in a way it does not support"))),
        };
        if contents.len() != size {
            return Err(malformed(&format!("{path} is damaged")));
        }
        files.push((path, contents));
    }
    Ok(files)
}

/// `name` when every part of it stays inside the folder it is unpacked into.
fn safe_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let safe =
        !name.starts_with('/') && !parts.is_empty() && parts.iter().all(|part| *part != ".." && !part.contains(':'));
    safe.then(|| parts.join("/"))
}

/// The files as they are, or, when they all sit in one top folder that holds
/// the manifest, as they are inside it.
fn top_folder_removed(files: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    if files.iter().any(|(path, _)| path == MANIFEST) {
        return files;
    }
    let Some((top, _)) = files.first().and_then(|(path, _)| path.split_once('/')) else {
        return files;
    };
    let prefix = format!("{top}/");
    let inside = files.iter().all(|(path, _)| path.starts_with(&prefix));
    if !inside || !files.iter().any(|(path, _)| *path == format!("{prefix}{MANIFEST}")) {
        return files;
    }
    files
        .into_iter()
        .map(|(path, contents)| (path[prefix.len()..].to_string(), contents))
        .collect()
}

/// Writes `key`, base64-encoded, into the manifest as `key` when it has none.
/// A manifest that does not parse is left for the engine to refuse.
fn add_key(manifest: &Path, key: &[u8]) -> Result<()> {
    let text = std::fs::read_to_string(manifest)?;
    let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else {
        return Ok(());
    };
    let Some(fields) = parsed.as_object_mut() else {
        return Ok(());
    };
    if fields.contains_key("key") {
        return Ok(());
    }
    let encoded = data_url("", key).split_off("data:;base64,".len());
    fields.insert("key".into(), serde_json::Value::String(encoded));
    let text = serde_json::to_string_pretty(&parsed).map_err(|error| HakuError::Storage(error.to_string()))?;
    std::fs::write(manifest, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ZIP archive of `files`, the first stored and the rest deflated.
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut directory = Vec::new();
        for (index, (name, contents)) in files.iter().enumerate() {
            let (method, data) = if index == 0 {
                (0u16, contents.to_vec())
            } else {
                (8u16, miniz_oxide::deflate::compress_to_vec(contents, 6))
            };
            let offset = archive.len() as u32;
            let sizes = |out: &mut Vec<u8>| {
                out.extend(0u32.to_le_bytes()); // crc, unchecked
                out.extend((data.len() as u32).to_le_bytes());
                out.extend((contents.len() as u32).to_le_bytes());
                out.extend((name.len() as u16).to_le_bytes());
                out.extend(0u16.to_le_bytes());
            };
            archive.extend(0x0403_4b50u32.to_le_bytes());
            archive.extend([20, 0, 0, 0]);
            archive.extend(method.to_le_bytes());
            archive.extend([0; 4]);
            sizes(&mut archive);
            archive.extend(name.as_bytes());
            archive.extend(&data);

            directory.extend(0x0201_4b50u32.to_le_bytes());
            directory.extend([20, 0, 20, 0, 0, 0]);
            directory.extend(method.to_le_bytes());
            directory.extend([0; 4]);
            sizes(&mut directory);
            directory.extend([0; 10]); // comment, disk, attributes
            directory.extend(offset.to_le_bytes());
            directory.extend(name.as_bytes());
        }
        let directory_at = archive.len() as u32;
        let directory_len = directory.len() as u32;
        archive.extend(directory);
        archive.extend(0x0605_4b50u32.to_le_bytes());
        archive.extend([0; 4]);
        archive.extend((files.len() as u16).to_le_bytes());
        archive.extend((files.len() as u16).to_le_bytes());
        archive.extend(directory_len.to_le_bytes());
        archive.extend(directory_at.to_le_bytes());
        archive.extend([0; 2]);
        archive
    }

    fn protobuf(number: u64, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut write = |mut n: u64| loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        };
        write(number << 3 | 2);
        write(value.len() as u64);
        out.extend(value);
        out
    }

    fn crx3(keys: &[&[u8]], id_of: &[u8], archive: &[u8]) -> Vec<u8> {
        let mut header = Vec::new();
        for key in keys {
            header.extend(protobuf(2, &[protobuf(1, key), protobuf(2, b"signature")].concat()));
        }
        let crx_id = &sha2::Sha256::digest(id_of)[..16];
        header.extend(protobuf(10000, &protobuf(1, crx_id)));
        let mut crx = b"Cr24".to_vec();
        crx.extend(3u32.to_le_bytes());
        crx.extend((header.len() as u32).to_le_bytes());
        crx.extend(header);
        crx.extend(archive);
        crx
    }

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("haku-package-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_zip_is_unpacked_with_its_folders() {
        let folder = temp_folder("zip");
        let archive = zip(&[
            ("manifest.json", br#"{"name": "A"}"#),
            ("popup/index.html", b"<p>hello</p>"),
        ]);
        unpack(&archive, &folder).unwrap();
        assert_eq!(std::fs::read(folder.join("popup/index.html")).unwrap(), b"<p>hello</p>");
        assert_eq!(
            std::fs::read_to_string(folder.join("manifest.json")).unwrap(),
            r#"{"name": "A"}"#
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_comment_holding_the_end_signature_is_not_the_end() {
        let mut archive = zip(&[("manifest.json", b"{}")]);
        let comment = [b"note PK".as_slice(), &[0; 30]].concat();
        let at = archive.len() - 2;
        archive[at..].copy_from_slice(&(comment.len() as u16).to_le_bytes());
        archive.extend(&comment);
        let files = read_zip(&archive, MAX_SIZE).unwrap();
        assert_eq!(files[0].0, "manifest.json");
    }

    #[test]
    fn an_archive_that_unpacks_past_the_limit_is_refused() {
        let archive = zip(&[("manifest.json", b"{}"), ("a.js", &[0; 64])]);
        assert!(read_zip(&archive, 66).is_ok());
        assert!(matches!(read_zip(&archive, 65), Err(HakuError::Unsupported(_))));
    }

    #[test]
    fn a_single_top_folder_is_unpacked_from_inside() {
        let files = vec![
            ("ubol/manifest.json".to_string(), b"{}".to_vec()),
            ("ubol/js/a.js".to_string(), b"".to_vec()),
        ];
        let paths: Vec<String> = top_folder_removed(files).into_iter().map(|(path, _)| path).collect();
        assert_eq!(paths, ["manifest.json", "js/a.js"]);
    }

    #[test]
    fn a_zip_without_a_manifest_is_not_an_extension() {
        let folder = temp_folder("no-manifest");
        assert!(unpack(&zip(&[("readme.txt", b"hi")]), &folder).is_err());
        assert!(!folder.exists());
    }

    #[test]
    fn a_path_leaving_the_folder_is_refused() {
        assert_eq!(safe_path("a/./b.js").as_deref(), Some("a/b.js"));
        assert_eq!(safe_path("../evil.js"), None);
        assert_eq!(safe_path("/abs.js"), None);
        assert_eq!(safe_path("C:/x.js"), None);
        let archive = zip(&[("manifest.json", b"{}"), ("..\\evil.js", b"x")]);
        assert!(read_zip(&archive, MAX_SIZE).is_err());
    }

    #[test]
    fn something_else_is_not_an_archive() {
        assert!(unpack(b"just text", &temp_folder("text")).is_err());
    }

    #[test]
    fn a_crx_writes_its_own_key_into_the_manifest() {
        let folder = temp_folder("crx");
        let archive = zip(&[("manifest.json", br#"{"name": "A", "version": "1"}"#)]);
        // The store's key signs it too; the extension's is the one its id names.
        let crx = crx3(&[b"store key", b"own key"], b"own key", &archive);
        unpack(&crx, &folder).unwrap();
        let manifest = std::fs::read_to_string(folder.join("manifest.json")).unwrap();
        let key = crate::model::key(&manifest).unwrap();
        assert_eq!(decode_key(&key).unwrap(), b"own key");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_manifest_key_is_kept() {
        let folder = temp_folder("kept-key");
        let archive = zip(&[("manifest.json", br#"{"key": "AAAA"}"#)]);
        unpack(&crx3(&[b"own key"], b"own key", &archive), &folder).unwrap();
        let manifest = std::fs::read_to_string(folder.join("manifest.json")).unwrap();
        assert_eq!(crate::model::key(&manifest).as_deref(), Some("AAAA"));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_key_decodes_from_base64() {
        assert_eq!(decode_key("aGFr dQ==").unwrap(), b"haku");
        assert_eq!(decode_key("not*base64"), None);
    }
}
