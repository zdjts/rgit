use std::fs;
use std::{fs::read, path::Path};

use crate::hash::hex_to_bytes;
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
pub fn write_tree(dir: &Path) -> anyhow::Result<String> {
    dbg!(dir);
    let mut entries = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name().to_str().unwrap().to_string();
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let hash = write_tree(&path)?;
            entries.push((format!("40000 {}", file_name), hex_to_bytes(&hash)?));
        } else {
            let hash = hash_object(&path, true)?;
            entries.push((format!("100644 {}", file_name), hex_to_bytes(&hash)?));
        }
    }

    entries.sort_by(|a, b| {
        let (mode_a, name_a) = a.0.split_once(' ').unwrap();
        let (mode_b, name_b) = b.0.split_once(' ').unwrap();
        let final_a = if mode_a == "40000" {
            format!("{}/", name_a)
        } else {
            name_a.to_string()
        };
        let final_b = if mode_b == "40000" {
            format!("{}/", name_b)
        } else {
            name_b.to_string()
        };
        final_a.cmp(&final_b)
    });

    let mut tree_content = Vec::new();
    for (mode_and_name, hash_bytes) in entries {
        tree_content.extend_from_slice(mode_and_name.as_bytes());
        tree_content.push(0);
        tree_content.extend_from_slice(&hash_bytes);
    }
    store_generic_object("tree", &tree_content)
}
