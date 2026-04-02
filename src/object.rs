use std::collections::BTreeMap;
use std::fs::read;
use std::path::Path;

use crate::hash::hex_to_bytes;
use crate::index::Index;
use crate::storage::store_generic_object;
use crate::storage::store_object;
use sha1::{Digest, Sha1};

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
