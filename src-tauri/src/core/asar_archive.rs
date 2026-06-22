use serde_json::{Map, Value};
use std::path::Path;

/// Header offsets in the asar pickle prefix (all little-endian u32):
///   0  size of the next field slot (always 4)
///   4  headerSize (total bytes of the header pickle content)
///   8  headerDataSize (size of the inner pickle payload, 4-byte aligned)
///   12 jsonLen (length of the JSON tree string)
///   16 ...the JSON tree itself, padded so the file content section
///      starts at 8 + headerSize (4-byte aligned).
const PICKLE_SIZE_FIELD: usize = 4;

#[derive(Debug)]
pub struct AsarHeader {
    /// The 4-byte-aligned total size of the header pickle content
    /// (everything from the headerDataSize field through the end of
    /// the padded JSON).
    /// Offset where packed file content begins: `8 + header_size`.
    pub content_base: u64,
    /// Parsed `{"files": {...}}` tree.
    pub tree: Value,
}

#[derive(Debug)]
pub enum AsarError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Format(String),
}

impl std::fmt::Display for AsarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsarError::Io(err) => write!(f, "asar io error: {err}"),
            AsarError::Json(err) => write!(f, "asar json error: {err}"),
            AsarError::Format(msg) => write!(f, "asar format error: {msg}"),
        }
    }
}

impl std::error::Error for AsarError {}

impl From<std::io::Error> for AsarError {
    fn from(value: std::io::Error) -> Self {
        AsarError::Io(value)
    }
}

impl From<serde_json::Error> for AsarError {
    fn from(value: serde_json::Error) -> Self {
        AsarError::Json(value)
    }
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, AsarError> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
        .ok_or_else(|| AsarError::Format(format!("header truncated at offset {offset}")))
}

fn align_to_four(value: usize) -> usize {
    (value + 3) & !3
}

/// Parse the asar header from `bytes`, returning the header metadata and
/// the JSON tree. The packed file content is preserved verbatim in
/// `bytes[content_base..]` and is not touched by this function.
pub fn read_header(bytes: &[u8]) -> Result<AsarHeader, AsarError> {
    let pickle_size = read_u32_le(bytes, 0)? as usize;
    if pickle_size != PICKLE_SIZE_FIELD {
        return Err(AsarError::Format(format!(
            "unexpected asar pickle size field: {pickle_size} (expected {PICKLE_SIZE_FIELD})"
        )));
    }
    let header_size = read_u32_le(bytes, 4)? as usize;
    let header_data_size = read_u32_le(bytes, 8)? as usize;
    let json_len = read_u32_le(bytes, 12)? as usize;
    if header_data_size != align_to_four(json_len + PICKLE_SIZE_FIELD) {
        return Err(AsarError::Format(format!(
            "asar header data size {header_data_size} does not match aligned json length {json_len}"
        )));
    }
    if 8 + header_size > bytes.len() {
        return Err(AsarError::Format(format!(
            "asar header size {header_size} exceeds file length {}",
            bytes.len()
        )));
    }
    let json_start = 16;
    let json_end = json_start + json_len;
    let json_bytes = bytes
        .get(json_start..json_end)
        .ok_or_else(|| AsarError::Format("asar json region out of bounds".to_string()))?;
    let tree = serde_json::from_slice(json_bytes)?;
    Ok(AsarHeader {
        content_base: (8 + header_size) as u64,
        tree,
    })
}

/// Read the contents of a packed file entry from `content` (the bytes
/// starting at the asar content base). Returns the raw bytes.
pub fn read_packed_file(content: &[u8], offset: u64, size: usize) -> Result<Vec<u8>, AsarError> {
    let start = offset as usize;
    let end = start + size;
    content
        .get(start..end)
        .map(|slice| slice.to_vec())
        .ok_or_else(|| {
            AsarError::Format(format!("packed file region {start}..{end} out of bounds"))
        })
}

fn files_map_mut(tree: &mut Value) -> Result<&mut Map<String, Value>, AsarError> {
    tree.get_mut("files")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AsarError::Format("asar tree has no \"files\" object".to_string()))
}

fn entry_offset(entry: &Value) -> Option<u64> {
    entry
        .get("offset")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok())
}

/// Produce the bytes for a freshly repacked asar that appends two new
/// packed files (the rewritten `package.json` and the inspector shim) to
/// the end of the original packed content, leaving every original file
/// byte-for-byte unchanged at its original offset.
///
/// `original` is the full original asar file bytes. `shim_name` is the
/// filename of the inspector entry shim (also becomes `package.json`'s
/// new `main`). `shim_body` is that shim's source. `new_package_json` is
/// the rewritten package.json body.
pub fn build_patched_asar(
    original: &[u8],
    new_package_json: &[u8],
    shim_name: &str,
    shim_body: &[u8],
) -> Result<Vec<u8>, AsarError> {
    let header = read_header(original)?;
    let content_base = header.content_base as usize;
    let packed_content_len = original.len() - content_base;
    let packed_content = original
        .get(content_base..)
        .ok_or_else(|| AsarError::Format("asar content base exceeds file length".to_string()))?;

    let mut tree = header.tree;
    let files = files_map_mut(&mut tree)?;
    // Original package.json is replaced in-place in the tree: its bytes are
    // appended at the end of the packed content, and its offset/size point
    // to the new copy. The original bytes stay in place but are no longer
    // referenced, which is harmless.
    let new_pkg_offset = packed_content_len as u64;
    let new_pkg_size = new_package_json.len();
    if let Some(pkg_entry) = files.get_mut("package.json") {
        let map = pkg_entry.as_object_mut().ok_or_else(|| {
            AsarError::Format("\"package.json\" asar entry is not an object".to_string())
        })?;
        map.insert("size".to_string(), Value::from(new_pkg_size));
        map.insert(
            "offset".to_string(),
            Value::from(new_pkg_offset.to_string()),
        );
        // Drop any integrity block: our appended copy is not the signed one.
        map.remove("integrity");
    } else {
        return Err(AsarError::Format(
            "asar tree has no \"package.json\" entry".to_string(),
        ));
    }
    let shim_offset = new_pkg_offset + new_pkg_size as u64;
    let mut shim_entry = Map::new();
    shim_entry.insert("size".to_string(), Value::from(shim_body.len()));
    shim_entry.insert("offset".to_string(), Value::from(shim_offset.to_string()));
    files.insert(shim_name.to_string(), Value::Object(shim_entry));

    let new_json = serde_json::to_vec(&tree)?;
    let new_json_len = new_json.len();
    let header_data_size = align_to_four(new_json_len + PICKLE_SIZE_FIELD);
    let new_header_size = (PICKLE_SIZE_FIELD + header_data_size) as u32;
    let new_content_base = 8 + new_header_size as usize;

    let total = new_content_base + packed_content_len + new_package_json.len() + shim_body.len();
    let mut out = vec![0u8; total];
    out[0..4].copy_from_slice(&(PICKLE_SIZE_FIELD as u32).to_le_bytes());
    out[4..8].copy_from_slice(&new_header_size.to_le_bytes());
    out[8..12].copy_from_slice(&(header_data_size as u32).to_le_bytes());
    out[12..16].copy_from_slice(&(new_json_len as u32).to_le_bytes());
    out[16..16 + new_json_len].copy_from_slice(&new_json);
    // bytes 16+new_json_len .. new_content_base stay zero (padding).
    out[new_content_base..new_content_base + packed_content_len].copy_from_slice(packed_content);
    let pkg_pos = new_content_base + packed_content_len;
    out[pkg_pos..pkg_pos + new_package_json.len()].copy_from_slice(new_package_json);
    let shim_pos = pkg_pos + new_package_json.len();
    out[shim_pos..shim_pos + shim_body.len()].copy_from_slice(shim_body);
    Ok(out)
}

/// Read and parse the `package.json` entry from an asar file's bytes,
/// returning its raw text and the value of its `main` field.
pub fn read_package_json(asar_bytes: &[u8]) -> Result<(String, String), AsarError> {
    let header = read_header(asar_bytes)?;
    let content_base = header.content_base as usize;
    let files = header
        .tree
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| AsarError::Format("asar tree has no \"files\" object".to_string()))?;
    let pkg_entry = files
        .get("package.json")
        .ok_or_else(|| AsarError::Format("asar tree has no \"package.json\" entry".to_string()))?;
    let offset = entry_offset(pkg_entry).ok_or_else(|| {
        AsarError::Format("\"package.json\" asar entry has no offset".to_string())
    })?;
    let size = pkg_entry
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| AsarError::Format("\"package.json\" asar entry has no size".to_string()))?
        as usize;
    let content = asar_bytes
        .get(content_base..)
        .ok_or_else(|| AsarError::Format("asar content base exceeds file length".to_string()))?;
    let bytes = read_packed_file(content, offset, size)?;
    let text = String::from_utf8(bytes)
        .map_err(|err| AsarError::Format(format!("package.json is not utf-8: {err}")))?;
    let value: Value = serde_json::from_str(&text)?;
    let main = value
        .get("main")
        .and_then(Value::as_str)
        .unwrap_or("index.js")
        .to_string();
    Ok((text, main))
}

/// Convenience: read an asar file from disk and return its bytes.
pub fn read_asar_file(path: &Path) -> Result<Vec<u8>, AsarError> {
    std::fs::read(path).map_err(AsarError::Io)
}

/// Read a top-level packed file from the asar bytes by name (e.g. the
/// inspector shim or package.json), returning its raw bytes.
pub fn read_named_file(asar_bytes: &[u8], name: &str) -> Result<Vec<u8>, AsarError> {
    let header = read_header(asar_bytes)?;
    let content_base = header.content_base as usize;
    let files = header
        .tree
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| AsarError::Format("asar tree has no \"files\" object".to_string()))?;
    let entry = files
        .get(name)
        .ok_or_else(|| AsarError::Format(format!("asar tree has no \"{name}\" entry")))?;
    let offset = entry_offset(entry)
        .ok_or_else(|| AsarError::Format(format!("\"{name}\" asar entry has no offset")))?;
    let size = entry
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| AsarError::Format(format!("\"{name}\" asar entry has no size")))?
        as usize;
    let content = asar_bytes
        .get(content_base..)
        .ok_or_else(|| AsarError::Format("asar content base exceeds file length".to_string()))?;
    read_packed_file(content, offset, size)
}

/// Test helper: build an asar whose files (slash-separated paths) are laid
/// out in the given order, returning the full asar bytes. Used by the
/// claude_desktop_patch re-patch self-reference tests to build an asar that
/// contains a nested Claude main entry such as ".vite/build/index.pre.js".
#[cfg(test)]
pub(crate) fn build_test_asar_with_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    use serde_json::{Map, Value};
    let mut content = Vec::new();
    let mut offsets: Vec<u64> = Vec::new();
    for (_path, body) in files {
        offsets.push(content.len() as u64);
        content.extend_from_slice(body);
    }
    fn insert(tree: &mut Map<String, Value>, path: &str, size: usize, offset: u64) {
        let parts: Vec<&str> = path.split('/').collect();
        let mut node = tree;
        for part in &parts[..parts.len() - 1] {
            let entry = node
                .entry(part.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            let obj = entry.as_object_mut().expect("dir entry must be object");
            node = obj
                .entry("files".to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .expect("files must be object");
        }
        let mut leaf = Map::new();
        leaf.insert("size".to_string(), Value::from(size));
        leaf.insert("offset".to_string(), Value::from(offset.to_string()));
        node.insert(parts[parts.len() - 1].to_string(), Value::Object(leaf));
    }
    let mut root = Map::new();
    let mut files_map = Map::new();
    for ((path, body), offset) in files.iter().zip(offsets.iter()) {
        insert(&mut files_map, path, body.len(), *offset);
    }
    root.insert("files".to_string(), Value::Object(files_map));
    let tree = Value::Object(root);
    let json_bytes = serde_json::to_vec(&tree).expect("serialize asar tree");
    let json_len = json_bytes.len();
    let header_data_size = align_to_four(json_len + PICKLE_SIZE_FIELD);
    let header_size = PICKLE_SIZE_FIELD + header_data_size;
    let content_base = 8 + header_size;
    let total = content_base + content.len();
    let mut out = vec![0u8; total];
    out[0..4].copy_from_slice(&(PICKLE_SIZE_FIELD as u32).to_le_bytes());
    out[4..8].copy_from_slice(&(header_size as u32).to_le_bytes());
    out[8..12].copy_from_slice(&(header_data_size as u32).to_le_bytes());
    out[12..16].copy_from_slice(&(json_len as u32).to_le_bytes());
    out[16..16 + json_len].copy_from_slice(&json_bytes);
    out[content_base..content_base + content.len()].copy_from_slice(&content);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sample_asar(main: &str, app_body: &str) -> Vec<u8> {
        // A minimal asar: package.json + app.js, with proper 4-byte padding.
        let pkg = format!("{{\"name\":\"sample\",\"main\":\"{main}\"}}");
        let pkg_bytes = pkg.as_bytes();
        let app_bytes = app_body.as_bytes();
        let files_json = format!(
            "{{\"files\":{{\"app.js\":{{\"size\":{},\"offset\":\"0\"}},\"package.json\":{{\"size\":{},\"offset\":\"{}\"}}}}}}",
            app_bytes.len(),
            pkg_bytes.len(),
            app_bytes.len(),
        );
        let json_bytes = files_json.as_bytes();
        let json_len = json_bytes.len();
        let header_data_size = align_to_four(json_len + PICKLE_SIZE_FIELD);
        let header_size = PICKLE_SIZE_FIELD + header_data_size;
        let content_base = 8 + header_size;
        let total = content_base + app_bytes.len() + pkg_bytes.len();
        let mut out = vec![0u8; total];
        out[0..4].copy_from_slice(&(PICKLE_SIZE_FIELD as u32).to_le_bytes());
        out[4..8].copy_from_slice(&(header_size as u32).to_le_bytes());
        out[8..12].copy_from_slice(&(header_data_size as u32).to_le_bytes());
        out[12..16].copy_from_slice(&(json_len as u32).to_le_bytes());
        out[16..16 + json_len].copy_from_slice(json_bytes);
        out[content_base..content_base + app_bytes.len()].copy_from_slice(app_bytes);
        let pkg_pos = content_base + app_bytes.len();
        out[pkg_pos..pkg_pos + pkg_bytes.len()].copy_from_slice(pkg_bytes);
        out
    }

    #[test]
    fn reads_package_json_main_and_body() {
        let asar = build_sample_asar("app.js", "console.log('hi');");
        let (text, main) = read_package_json(&asar).unwrap();
        assert_eq!(main, "app.js");
        assert!(text.contains("\"main\":\"app.js\""));
        assert!(text.contains("\"name\":\"sample\""));
    }

    #[test]
    fn patched_asar_preserves_original_content_and_swaps_main() {
        let asar = build_sample_asar("app.js", "console.log('hi');");
        let new_pkg = b"{\"name\":\"sample\",\"main\":\"_shim.js\",\"originalMain\":\"app.js\"}";
        let shim = b"require('./app.js');\n";
        let patched = build_patched_asar(&asar, new_pkg, "_shim.js", shim).unwrap();

        // Original app.js bytes must still be readable at offset 0.
        let header = read_header(&patched).unwrap();
        let content_base = header.content_base as usize;
        let app_entry = header.tree.get("files").unwrap().get("app.js").unwrap();
        let off = entry_offset(app_entry).unwrap();
        let size = app_entry.get("size").and_then(Value::as_u64).unwrap() as usize;
        let app_bytes = &patched[content_base + off as usize..content_base + off as usize + size];
        assert_eq!(app_bytes, b"console.log('hi');");

        // package.json now points to the appended copy with new main.
        let (text, main) = read_package_json(&patched).unwrap();
        assert_eq!(main, "_shim.js");
        assert!(text.contains("\"originalMain\":\"app.js\""));

        // shim entry exists and its bytes are correct.
        let shim_entry = header.tree.get("files").unwrap().get("_shim.js").unwrap();
        let shim_off = entry_offset(shim_entry).unwrap();
        let shim_size = shim_entry.get("size").and_then(Value::as_u64).unwrap() as usize;
        let shim_bytes = &patched
            [content_base + shim_off as usize..content_base + shim_off as usize + shim_size];
        assert_eq!(shim_bytes, shim);
    }

    #[test]
    fn patched_asar_header_is_four_byte_aligned() {
        let asar = build_sample_asar("app.js", "x");
        let patched = build_patched_asar(&asar, b"{\"main\":\"_s.js\"}", "_s.js", b"y").unwrap();
        let header_data_size = u32::from_le_bytes(patched[8..12].try_into().unwrap()) as usize;
        let json_len = u32::from_le_bytes(patched[12..16].try_into().unwrap()) as usize;
        assert_eq!(
            header_data_size,
            align_to_four(json_len + PICKLE_SIZE_FIELD)
        );
    }
}
