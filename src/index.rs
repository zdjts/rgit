use anyhow::Result;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 简化的 Index 条目
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub mtime: u64,     // 修改时间 (秒级时间戳)
    pub mode: u32,      // 文件模式 (100644 或 100755)
    pub sha1: [u8; 20], // 对象 SHA-1
    pub path: String,   // 文件路径
}

/// 简化的 Index 结构
#[derive(Debug, Default)]
pub struct Index {
    pub entries: BTreeMap<String, IndexEntry>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// 从 .rgit/index 文件加载 Index
    pub fn load() -> Result<Self> {
        let path = Path::new(".rgit/index");
        if !path.exists() {
            return Ok(Self::new());
        }

        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        if data.len() < 12 {
            anyhow::bail!("Index 文件损坏: 太短");
        }

        // 读取 header
        let signature = &data[0..4];
        if signature != b"DIRC" {
            anyhow::bail!("Index 文件签名无效");
        }

        let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if version != 1 {
            anyhow::bail!("不支持的 Index 版本: {}", version);
        }

        let entry_count = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;

        let mut index = Self::new();
        let mut offset = 12;

        for _ in 0..entry_count {
            // mtime (8 bytes: seconds + nanoseconds, 我们只存储秒)
            let mtime = u64::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            offset += 8;

            // mode (4 bytes)
            let mode = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;

            // sha1 (20 bytes)
            let mut sha1 = [0u8; 20];
            sha1.copy_from_slice(&data[offset..offset + 20]);
            offset += 20;

            // path (变长, 以 null 结尾)
            let null_pos = data[offset..]
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| anyhow::anyhow!("Index 条目缺少路径结束符"))?;
            let path = String::from_utf8(data[offset..offset + null_pos].to_vec())?;
            offset += null_pos + 1;

            index.entries.insert(
                path.clone(),
                IndexEntry {
                    mtime,
                    mode,
                    sha1,
                    path,
                },
            );
        }

        Ok(index)
    }

    /// 保存 Index 到 .rgit/index
    pub fn save(&self) -> Result<()> {
        let path = Path::new(".rgit/index");
        let mut data = Vec::new();

        data.extend_from_slice(b"DIRC");
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&(self.entries.len() as u32).to_be_bytes());

        // 按路径排序写入条目
        for entry in self.entries.values() {
            // mtime (8 bytes: 存储秒数)
            data.extend_from_slice(&entry.mtime.to_be_bytes());

            // mode (4 bytes)
            data.extend_from_slice(&entry.mode.to_be_bytes());

            // sha1 (20 bytes)
            data.extend_from_slice(&entry.sha1);

            // path + null terminator
            data.extend_from_slice(entry.path.as_bytes());
            data.push(0);
        }

        let mut hasher = Sha1::new();
        hasher.update(&data);
        let checksum = hasher.finalize();
        data.extend_from_slice(&checksum);

        fs::write(path, data)?;
        Ok(())
    }

    /// 添加或更新条目
    pub fn add(&mut self, path: &str, mode: u32, sha1: [u8; 20]) {
        let mtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.entries.insert(
            path.to_string(),
            IndexEntry {
                mtime,
                mode,
                sha1,
                path: path.to_string(),
            },
        );
    }

    /// 移除条目
    pub fn remove(&mut self, path: &str) -> Option<IndexEntry> {
        self.entries.remove(path)
    }
}
