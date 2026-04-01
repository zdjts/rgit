use anyhow::Ok;
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::io::Write;
use std::{fs, path::Path};

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
