use std::io::Write;
use std::{fs, path::Path};

use flate2::{Compression, write::ZlibEncoder};
use sha1::{Digest, Sha1};

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
