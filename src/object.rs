use std::collections::BTreeMap;
use std::fs::read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hash::hex_to_bytes;
use crate::index::Index;
use crate::storage::{read_object, store_generic_object, store_object};
use sha1::{Digest, Sha1};

/// 递归展开一个 tree 对象，返回 path -> sha1_hex 的扁平映射
/// prefix 为当前路径前缀，根调用时传 ""
pub fn flatten_tree(tree_hash: &str, prefix: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let (obj_type, content) = read_object(tree_hash)?;
    if obj_type != "tree" {
        anyhow::bail!("对象 {} 类型为 {}，不是 tree", tree_hash, obj_type);
    }

    let mut result = BTreeMap::new();
    let mut offset = 0;

    while offset < content.len() {
        // 格式: "<mode> <name>\0<20-byte-sha1>"
        let null_pos = content[offset..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| anyhow::anyhow!("tree 条目格式损坏"))?;

        let header = std::str::from_utf8(&content[offset..offset + null_pos])?;
        let (mode, name) = header
            .split_once(' ')
            .ok_or_else(|| anyhow::anyhow!("tree 条目 header 无效: {}", header))?;

        offset += null_pos + 1;

        if offset + 20 > content.len() {
            anyhow::bail!("tree 条目 sha1 截断");
        }
        let sha1_bytes = &content[offset..offset + 20];
        let sha1_hex = hex::encode(sha1_bytes);
        offset += 20;

        let full_path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", prefix, name)
        };

        if mode == "40000" {
            // 子目录，递归展开
            let sub = flatten_tree(&sha1_hex, &full_path)?;
            result.extend(sub);
        } else {
            result.insert(full_path, sha1_hex);
        }
    }

    Ok(result)
}

/// 解析后的 commit 对象
#[derive(Debug)]
pub struct CommitObject {
    pub hash: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub author_time: i64, // Unix 时间戳
    pub committer: String,
    pub committer_time: i64,
    pub message: String,
}

/// 从 object store 读取并解析一个 commit 对象
pub fn read_commit(hash: &str) -> anyhow::Result<CommitObject> {
    let (obj_type, content) = read_object(hash)?;
    if obj_type != "commit" {
        anyhow::bail!("对象 {} 类型为 {}，不是 commit", hash, obj_type);
    }
    let text = String::from_utf8(content)?;
    parse_commit(hash, &text)
}

/// 解析 commit 文本内容
/// 格式：
///   tree <hash>
///   parent <hash>   (0 或多行)
///   author <name> <email> <timestamp> <tz>
///   committer <name> <email> <timestamp> <tz>
///   (空行)
///   <message>
fn parse_commit(hash: &str, text: &str) -> anyhow::Result<CommitObject> {
    // 以第一个空行分割 header 区和 message 区
    let (header_part, message) = text
        .split_once("\n\n")
        .ok_or_else(|| anyhow::anyhow!("commit 格式损坏，缺少 header/message 分隔行: {}", hash))?;

    let mut tree = String::new();
    let mut parents = Vec::new();
    let mut author = String::new();
    let mut author_time = 0i64;
    let mut committer = String::new();
    let mut committer_time = 0i64;

    for line in header_part.lines() {
        if let Some(val) = line.strip_prefix("tree ") {
            tree = val.to_string();
        } else if let Some(val) = line.strip_prefix("parent ") {
            parents.push(val.to_string());
        } else if let Some(val) = line.strip_prefix("author ") {
            let (name_email, ts) = parse_identity(val)?;
            author = name_email;
            author_time = ts;
        } else if let Some(val) = line.strip_prefix("committer ") {
            let (name_email, ts) = parse_identity(val)?;
            committer = name_email;
            committer_time = ts;
        }
    }

    if tree.is_empty() {
        anyhow::bail!("commit {} 缺少 tree 字段", hash);
    }

    Ok(CommitObject {
        hash: hash.to_string(),
        tree,
        parents,
        author,
        author_time,
        committer,
        committer_time,
        message: message.to_string(),
    })
}

/// 从 identity 行解析出 "Name <email>" 和 Unix 时间戳
/// 格式: Name <email> <timestamp> <timezone>
fn parse_identity(line: &str) -> anyhow::Result<(String, i64)> {
    // 从右往左找时区（+0800），再找时间戳
    let parts: Vec<&str> = line.rsplitn(3, ' ').collect();
    // parts[0] = tz, parts[1] = timestamp, parts[2] = "Name <email>"
    if parts.len() < 3 {
        anyhow::bail!("identity 行格式无效: {}", line);
    }
    let timestamp: i64 = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("时间戳解析失败: {}", parts[1]))?;
    let name_email = parts[2].to_string();
    Ok((name_email, timestamp))
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

/// 从 Index 生成 tree 对象
pub fn write_tree_from_index(index: &Index) -> anyhow::Result<String> {
    write_tree_at_prefix(index, "")
}

/// 递归生成指定路径前缀下的 tree
fn write_tree_at_prefix(index: &Index, prefix: &str) -> anyhow::Result<String> {
    // 收集当前层级的条目和子目录
    let mut entries: Vec<(String, [u8; 20])> = Vec::new();
    let mut subtrees: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (path, entry) in &index.entries {
        let relative_path = if prefix.is_empty() {
            path.as_str()
        } else if path.starts_with(&format!("{}/", prefix)) {
            &path[prefix.len() + 1..]
        } else {
            continue;
        };

        if let Some(slash_pos) = relative_path.find('/') {
            let subdir_name = &relative_path[..slash_pos];
            subtrees
                .entry(subdir_name.to_string())
                .or_default()
                .push(path.clone());
        } else {
            let mode_str = format!("{}", entry.mode);
            entries.push((format!("{} {}", mode_str, relative_path), entry.sha1));
        }
    }

    for (subdir_name, _) in subtrees {
        let subdir_prefix = if prefix.is_empty() {
            subdir_name.clone()
        } else {
            format!("{}/{}", prefix, subdir_name)
        };
        let subtree_hash = write_tree_at_prefix(index, &subdir_prefix)?;
        let hash_bytes = hex_to_bytes(&subtree_hash)?;
        let mut hash_arr = [0u8; 20];
        hash_arr.copy_from_slice(&hash_bytes);
        entries.push((format!("40000 {}", subdir_name), hash_arr));
    }

    // 按照 git 的排序规则: 目录名加 / 后缀排序
    entries.sort_by(|a, b| {
        let (mode_a, name_a) = a.0.split_once(' ').unwrap();
        let (mode_b, name_b) = b.0.split_once(' ').unwrap();
        // 目录 (40000) 加 '/' 后缀排序
        let sort_key_a = if mode_a == "40000" {
            format!("{}/", name_a)
        } else {
            name_a.to_string()
        };
        let sort_key_b = if mode_b == "40000" {
            format!("{}/", name_b)
        } else {
            name_b.to_string()
        };
        sort_key_a.cmp(&sort_key_b)
    });

    // 构建 tree 内容
    let mut tree_content = Vec::new();
    for (mode_and_name, hash_bytes) in entries {
        tree_content.extend_from_slice(mode_and_name.as_bytes());
        tree_content.push(0);
        tree_content.extend_from_slice(&hash_bytes);
    }

    store_generic_object("tree", &tree_content)
}

pub fn commit_tree(
    tree_hash: &str,
    parent_hashes: &[String],
    author: &str,
    message: &str,
) -> anyhow::Result<String> {
    // 获取当前时间戳和时区
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let timezone = "+0800"; // 简化处理，使用固定时区

    // 构建 commit 内容
    let mut content = String::new();

    // tree 行
    content.push_str(&format!("tree {}\n", tree_hash));

    // parent 行 (可以有多个)
    for parent in parent_hashes {
        content.push_str(&format!("parent {}\n", parent));
    }

    // author 行
    content.push_str(&format!("author {} {} {}\n", author, now, timezone));

    // committer 行
    content.push_str(&format!("committer {} {} {}\n", author, now, timezone));

    // 空行 + 提交信息
    content.push('\n');
    content.push_str(message);

    store_generic_object("commit", content.as_bytes())
}
