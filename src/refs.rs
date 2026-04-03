use std::fs;
use std::path::{Path, PathBuf};

/// 解析 ref 名称为文件路径
fn ref_path(refname: &str) -> PathBuf {
    Path::new(".rgit").join(refname)
}

/// 读取一个 ref 的值（原始内容，可能是符号引用或 commit hash）
fn read_ref_raw(refname: &str) -> anyhow::Result<String> {
    let path = ref_path(refname);
    if !path.exists() {
        anyhow::bail!("ref 不存在: {}", refname);
    }
    let content = fs::read_to_string(&path)?;
    Ok(content.trim().to_string())
}

/// 解析 ref，递归跟随符号引用 (symbolic ref) 直到得到 commit hash
pub fn resolve_ref(refname: &str) -> anyhow::Result<Option<String>> {
    let path = ref_path(refname);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?.trim().to_string();
    if let Some(target) = content.strip_prefix("ref: ") {
        // 符号引用，递归解析
        resolve_ref(target)
    } else {
        // 直接是 hash
        Ok(Some(content))
    }
}

/// 如果 HEAD 是 detached 状态则返回 None
pub fn head_ref() -> anyhow::Result<Option<String>> {
    let content = read_ref_raw("HEAD")?;
    if let Some(target) = content.strip_prefix("ref: ") {
        Ok(Some(target.to_string()))
    } else {
        Ok(None) // detached HEAD
    }
}

/// 会自动创建父目录
pub fn update_ref(refname: &str, hash: &str) -> anyhow::Result<()> {
    validate_hash(hash)?;
    let path = ref_path(refname);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, format!("{}\n", hash))?;
    Ok(())
}

pub fn set_head(target: &str) -> anyhow::Result<()> {
    let content = if target.starts_with("refs/") {
        format!("ref: {}\n", target)
    } else {
        validate_hash(target)?;
        format!("{}\n", target)
    };
    fs::write(Path::new(".rgit/HEAD"), content)?;
    Ok(())
}

fn validate_hash(hash: &str) -> anyhow::Result<()> {
    if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("无效的对象哈希: {}", hash);
    }
    Ok(())
}
