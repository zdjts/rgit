use std::io::Read;
use std::io::Write;
use std::{
    fs::{self, read},
    path::Path,
};

use flate2::read::ZlibDecoder;
use flate2::{Compression, write::ZlibEncoder};
use sha1::{Digest, Sha1};

pub fn init() -> anyhow::Result<()> {
    let root = Path::new(".rgit");
    if root.exists() {
        println!(
            "Reinitialized existing rgit repository in {:?}",
            fs::canonicalize("root")?
        );
    }
    fs::create_dir_all(root.join("objects"))?;
    fs::create_dir_all(root.join("refs"))?;
    fs::write(root.join("HEAD"), "ref: refs/heads/master\n")?;
    println!(
        "Initialized empty rgit repository in {:?}",
        fs::canonicalize(root)?
    );

    Ok(())
}
pub fn hash_object(file_path: &Path, write: bool) -> anyhow::Result<String> {
    let content = read(file_path)?;
    let size = content.len();

    let header = format!("blob {}\0", size);
    let mut hasher = Sha1::new();
    hasher.update(&header);
    hasher.update(&content);
    // let bytes: [u8; 20] = hasher.finalize().into();
    let hash = <[u8; 20]>::from(hasher.finalize())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    if write {
        store_object(&hash, &header, &content)?;
    }
    Ok(hash)
}
fn store_object(hash: &str, header: &str, content: &[u8]) -> anyhow::Result<()> {
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

pub fn cat_file(hash: &str, pretty_print: bool) -> anyhow::Result<()> {
    if !pretty_print {
        anyhow::bail!("缺少参数-p 打印");
    }
    let (dir, file) = hash.split_at(2);
    let path = Path::new(".rgit/objects").join(dir).join(file);
    if !path.exists() {
        anyhow::bail!("不存在的对象哈希:{}", hash);
    }
    let f = fs::File::open(path)?;
    let mut decoder = ZlibDecoder::new(f);
    let mut decoded_data = Vec::new();
    decoder.read_to_end(&mut decoded_data)?;

    if let Some(null_pos) = decoded_data.iter().position(|&b| b == 0) {
        let content = &decoded_data[null_pos + 1..];
        std::io::stdout().write_all(content)?;
    } else {
        anyhow::bail!("损坏的对象格式,无法找到 Header 结束标志");
    }

    Ok(())
}
