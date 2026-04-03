use std::io::{Read, Write};
use std::{fs, path::Path};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use sha1::{Digest, Sha1};

/// 从 object store 读取并解压一个对象
/// 返回 (type, content)，type 如 "blob" / "tree" / "commit"
pub fn read_object(hash: &str) -> anyhow::Result<(String, Vec<u8>)> {
    let (dir, file) = hash.split_at(2);
    let path = Path::new(".rgit/objects").join(dir).join(file);
    if !path.exists() {
        anyhow::bail!("对象不存在: {}", hash);
    }

    let f = fs::File::open(&path)?;
    let mut decoder = ZlibDecoder::new(f);
    let mut raw = Vec::new();
    decoder.read_to_end(&mut raw)?;

    // 格式: "<type> <size>\0<content>"
    let null_pos = raw
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| anyhow::anyhow!("对象格式损坏，缺少 header 结束符: {}", hash))?;

    let header = std::str::from_utf8(&raw[..null_pos])?;
    let obj_type = header
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("对象 header 格式无效: {}", header))?
        .0
        .to_string();

    let content = raw[null_pos + 1..].to_vec();
    Ok((obj_type, content))
}

pub fn store_generic_object(obj_type: &str, content: &[u8]) -> anyhow::Result<String> {
    let header = format!("{} {}\0", obj_type, content.len());
    let mut hasher = Sha1::new();
    hasher.update(header.as_bytes());
    hasher.update(content);
    let hash = hex::encode(hasher.finalize());
    let (dir, file) = hash.split_at(2);
    let object_path = Path::new(".rgit/objects").join(dir);
    fs::create_dir_all(&object_path)?;

    let f = fs::File::create(object_path.join(file))?;
    let mut encoder = ZlibEncoder::new(f, Compression::default());
    encoder.write_all(header.as_bytes())?;
    encoder.write_all(content)?;
    encoder.finish()?;

    Ok(hash)
}
pub fn store_object(hash: &str, header: &str, content: &[u8]) -> anyhow::Result<()> {
    let rgit_dir = Path::new(".rgit/objects");
    let (dir, file) = hash.split_at(2);
    let object_path = rgit_dir.join(dir);
    fs::create_dir_all(&object_path)?;
    let file_path = object_path.join(file);
    if file_path.exists() {
        return Ok(());
    }
    let f = fs::File::create(file_path)?;
    let mut encoder = ZlibEncoder::new(f, Compression::default());
    encoder.write_all(header.as_bytes())?;
    encoder.write_all(content)?;
    encoder.finish()?;
    Ok(())
}
