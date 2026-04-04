use anyhow::Ok;
use flate2::read::ZlibDecoder;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::{fs, path::Path};

use crate::index::Index;
use crate::object::{CommitObject, flatten_tree, read_commit};
use crate::refs::resolve_ref;

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

/// 从给定的 commit hash 出发，向上遍历 parent 链，打印提交历史
pub fn log(start_hash: &str) -> anyhow::Result<()> {
    let mut current = start_hash.to_string();
    loop {
        let commit = read_commit(&current)?;
        print_commit(&commit);

        // 沿第一个 parent 向上（主线）
        match commit.parents.into_iter().next() {
            Some(parent) => current = parent,
            None => break, // 到达根提交
        }
    }
    Ok(())
}

/// 格式化并打印单个 commit，模仿 git log 输出风格
fn print_commit(commit: &CommitObject) {
    println!("commit {}", commit.hash);
    println!("Author: {}", commit.author);
    println!("Date:   {}", format_timestamp(commit.author_time));
    println!();
    // message 每行缩进 4 空格
    for line in commit.message.lines() {
        println!("    {}", line);
    }
    println!();
}

/// 将 Unix 时间戳格式化为可读字符串
fn format_timestamp(ts: i64) -> String {
    // 不引入 chrono，手动转换为 "YYYY-MM-DD HH:MM:SS" UTC
    let secs = ts as u64;
    let (date, time) = unix_to_datetime(secs);
    format!("{} {}", date, time)
}

/// 将 Unix 时间戳（UTC）分解为日期和时间字符串
fn unix_to_datetime(secs: u64) -> (String, String) {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;

    // 从 1970-01-01 推算年月日
    let mut year = 1970u32;
    let mut remaining_days = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let months = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for &days_in_month in &months {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }
    let day = remaining_days + 1;

    (
        format!("{:04}-{:02}-{:02}", year, month, day),
        format!("{:02}:{:02}:{:02} +0000", h, m, s),
    )
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// 递归扫描工作目录，收集所有文件路径（相对路径，跳过 .rgit/）
fn scan_workdir(dir: &Path, prefix: &str, out: &mut BTreeMap<String, u64>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // 跳过 .rgit 目录
        if prefix.is_empty() && name == ".rgit" {
            continue;
        }

        let rel_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };

        let meta = entry.metadata()?;
        if meta.is_dir() {
            scan_workdir(&entry.path(), &rel_path, out)?;
        } else {
            // 纳秒级 mtime，与 Index 中存储精度一致
            let mtime_ns = meta.mtime() as u64 * 1_000_000_000 + meta.mtime_nsec() as u64;
            out.insert(rel_path, mtime_ns);
        }
    }
    Ok(())
}

/// status：对比三层状态
///
///   HEAD tree  vs  Index    → staged changes（待提交的改动）
///   Index      vs  workdir  → unstaged changes（已修改但未暂存）
///   workdir    -   Index    → untracked files（未跟踪的新文件）
pub fn status() -> anyhow::Result<()> {
    let index = Index::load()?;

    // ── 1. 获取 HEAD tree 的扁平映射 ─────────────────────────────────────
    // path -> sha1_hex，HEAD 不存在（初始仓库）时为空
    let head_tree: BTreeMap<String, String> = match resolve_ref("HEAD")? {
        Some(commit_hash) => {
            let commit = read_commit(&commit_hash)?;
            flatten_tree(&commit.tree, "")?
        }
        None => BTreeMap::new(),
    };

    // ── 2. 将 Index 转为 path -> sha1_hex 映射 ───────────────────────────
    let index_map: BTreeMap<String, String> = index
        .entries
        .values()
        .map(|e| (e.path.clone(), hex::encode(e.sha1)))
        .collect();

    // ── 3. 扫描工作目录，得到 path -> mtime ─────────────────────────────
    let mut workdir: BTreeMap<String, u64> = BTreeMap::new();
    scan_workdir(Path::new("."), "", &mut workdir)?;

    // ── 4. 计算 staged changes（HEAD vs Index）───────────────────────────
    let mut staged_new: Vec<String> = Vec::new();
    let mut staged_modified: Vec<String> = Vec::new();
    let mut staged_deleted: Vec<String> = Vec::new();

    // Index 中有、HEAD 中没有 → new file
    // Index 中有、HEAD 中也有但 sha1 不同 → modified
    for (path, idx_sha1) in &index_map {
        match head_tree.get(path) {
            None => staged_new.push(path.clone()),
            Some(head_sha1) if head_sha1 != idx_sha1 => staged_modified.push(path.clone()),
            _ => {}
        }
    }
    // HEAD 中有、Index 中没有 → deleted
    for path in head_tree.keys() {
        if !index_map.contains_key(path) {
            staged_deleted.push(path.clone());
        }
    }

    // ── 5. 计算 unstaged changes（Index vs workdir）──────────────────────
    // 这里利用 mtime 做快速筛选：
    //   mtime 与 Index 记录一致 → 文件未变，跳过哈希计算
    //   mtime 不同 → 需要重新哈希确认是否真正修改
    let mut unstaged_modified: Vec<String> = Vec::new();
    let mut unstaged_deleted: Vec<String> = Vec::new();

    for entry in index.entries.values() {
        match workdir.get(&entry.path) {
            None => {
                // Index 中有记录但文件不存在 → deleted
                unstaged_deleted.push(entry.path.clone());
            }
            Some(&disk_mtime) => {
                if disk_mtime != entry.mtime {
                    // mtime 变化，做精确哈希比对
                    if file_sha1_differs(&entry.path, &entry.sha1)? {
                        unstaged_modified.push(entry.path.clone());
                    }
                }
                // mtime 相同 → 跳过，视为未变
            }
        }
    }

    // ── 6. 未跟踪文件（workdir 有、Index 无）────────────────────────────
    let mut untracked: Vec<String> = Vec::new();
    for path in workdir.keys() {
        if !index_map.contains_key(path) {
            untracked.push(path.clone());
        }
    }

    // ── 7. 输出 ──────────────────────────────────────────────────────────
    let has_staged =
        !staged_new.is_empty() || !staged_modified.is_empty() || !staged_deleted.is_empty();
    let has_unstaged = !unstaged_modified.is_empty() || !unstaged_deleted.is_empty();

    if has_staged {
        println!("Changes to be committed:");
        for p in &staged_new {
            println!("\tnew file:   {}", p);
        }
        for p in &staged_modified {
            println!("\tmodified:   {}", p);
        }
        for p in &staged_deleted {
            println!("\tdeleted:    {}", p);
        }
        println!();
    }

    if has_unstaged {
        println!("Changes not staged for commit:");
        for p in &unstaged_modified {
            println!("\tmodified:   {}", p);
        }
        for p in &unstaged_deleted {
            println!("\tdeleted:    {}", p);
        }
        println!();
    }

    if !untracked.is_empty() {
        println!("Untracked files:");
        for p in &untracked {
            println!("\t{}", p);
        }
        println!();
    }

    if !has_staged && !has_unstaged && untracked.is_empty() {
        println!("nothing to commit, working tree clean");
    }

    Ok(())
}

/// 读取磁盘文件并计算 blob sha1，与 Index 中记录比较
/// 只在 mtime 变化时调用，避免不必要的 IO
fn file_sha1_differs(path: &str, index_sha1: &[u8; 20]) -> anyhow::Result<bool> {
    use sha1::{Digest, Sha1};

    let content = match fs::read(path) {
        std::result::Result::Ok(c) => c,
        Err(_) => return Ok(true), // 读取失败视为变化
    };
    let header = format!("blob {}\0", content.len());
    let mut hasher = Sha1::new();
    hasher.update(header.as_bytes());
    hasher.update(&content);
    let disk_sha1: [u8; 20] = hasher.finalize().into();
    Ok(disk_sha1 != *index_sha1)
}

// ─────────────────────────────────────────────────────────────────────────────
// checkout
// ─────────────────────────────────────────────────────────────────────────────

/// 将 commit 或 tree 对象还原到工作目录，同步更新 Index。
/// target 可以是 commit hash、tree hash 或 ref 名称（如 refs/heads/master）。
pub fn checkout(target: &str) -> anyhow::Result<()> {
    use crate::storage::read_object;

    // 解析 target → tree hash
    let tree_hash = resolve_target_to_tree(target)?;

    // 展开 tree 为扁平 path→sha1 映射
    let tree_files = flatten_tree(&tree_hash, "")?;

    // 加载当前 Index，用于判断哪些文件需要更新
    let mut index = Index::load()?;

    // ── 1. 写出 tree 中的所有文件 ────────────────────────────────────────
    for (path, sha1_hex) in &tree_files {
        let (obj_type, content) = read_object(sha1_hex)?;
        if obj_type != "blob" {
            anyhow::bail!("checkout: 期望 blob，得到 {} ({})", obj_type, sha1_hex);
        }

        // 创建父目录
        let disk_path = Path::new(path);
        if let Some(parent) = disk_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        // 写入文件内容
        fs::write(disk_path, &content)?;
        println!("restored: {}", path);

        // 更新 Index 条目（mode 固定为 100644；checkout 不保留可执行位，后续可扩展）
        let mut sha1_arr = [0u8; 20];
        sha1_arr.copy_from_slice(&hex::decode(sha1_hex)?);
        index.add(path, 100644, sha1_arr);
    }

    // ── 2. 清理 Index 中有但 tree 中没有的条目（并删除对应磁盘文件）────
    let paths_to_remove: Vec<String> = index
        .entries
        .keys()
        .filter(|p| !tree_files.contains_key(*p))
        .cloned()
        .collect();

    for path in paths_to_remove {
        let disk_path = Path::new(&path);
        if disk_path.exists() {
            fs::remove_file(disk_path)?;
            println!("removed:  {}", path);
        }
        index.remove(&path);
    }

    index.save()?;
    Ok(())
}

/// 将 target（commit hash / tree hash / ref）解析为 tree hash
fn resolve_target_to_tree(target: &str) -> anyhow::Result<String> {
    use crate::storage::read_object;

    // 先尝试作为 ref 解析（如 HEAD、refs/heads/master）
    let hash = if target.len() == 40 && target.chars().all(|c| c.is_ascii_hexdigit()) {
        target.to_string()
    } else {
        resolve_ref(target)?.ok_or_else(|| anyhow::anyhow!("无法解析 ref: {}", target))?
    };

    // 判断对象类型
    let (obj_type, _) = read_object(&hash)?;
    match obj_type.as_str() {
        "tree" => Ok(hash),
        "commit" => {
            let commit = read_commit(&hash)?;
            Ok(commit.tree)
        }
        other => anyhow::bail!("checkout 不支持的对象类型: {}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// add
// ─────────────────────────────────────────────────────────────────────────────

/// 将一组路径加入 Index（支持文件和目录；目录会递归暂存所有子文件）。
pub fn add(paths: &[String]) -> anyhow::Result<()> {
    let mut index = Index::load()?;
    let mut staged = 0usize;

    for raw_path in paths {
        let path = Path::new(raw_path);
        if !path.exists() {
            anyhow::bail!("路径不存在: {}", raw_path);
        }
        collect_and_stage(path, raw_path, &mut index, &mut staged)?;
    }

    index.save()?;
    println!("已暂存 {} 个文件", staged);
    Ok(())
}

/// 递归收集路径下所有文件并写入 Index
fn collect_and_stage(
    path: &Path,
    rel: &str,
    index: &mut Index,
    count: &mut usize,
) -> anyhow::Result<()> {
    use crate::storage::store_object;
    use sha1::{Digest, Sha1};

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过 .rgit 和隐藏目录
            if name.starts_with('.') {
                continue;
            }
            let child_rel = format!("{}/{}", rel, name);
            collect_and_stage(&entry.path(), &child_rel, index, count)?;
        }
    } else {
        // 读取文件，写入 object store，更新 Index
        let content = fs::read(path)?;
        let header = format!("blob {}\0", content.len());
        let mut hasher = Sha1::new();
        hasher.update(header.as_bytes());
        hasher.update(&content);
        let sha1_hex = hex::encode(hasher.finalize());

        store_object(&sha1_hex, &header, &content)?;

        let mut sha1_arr = [0u8; 20];
        sha1_arr.copy_from_slice(&hex::decode(&sha1_hex)?);

        // 保留已有条目的 mode；新文件默认 100644
        let mode = index.entries.get(rel).map(|e| e.mode).unwrap_or(100644);

        index.add(rel, mode, sha1_arr);
        *count += 1;
    }
    Ok(())
}

/// 从 Index 中移除文件。
/// cached=true  → 只移除 Index 条目，保留磁盘文件（等同 git rm --cached）
/// cached=false → 同时删除磁盘文件
pub fn rm(paths: &[String], cached: bool) -> anyhow::Result<()> {
    let mut index = Index::load()?;
    let mut removed = 0usize;

    for raw_path in paths {
        // 支持目录：移除所有以 raw_path/ 开头的条目
        let prefix = format!("{}/", raw_path);
        let to_remove: Vec<String> = index
            .entries
            .keys()
            .filter(|p| *p == raw_path || p.starts_with(&prefix))
            .cloned()
            .collect();

        if to_remove.is_empty() {
            anyhow::bail!("路径未被跟踪: {}", raw_path);
        }

        for path in to_remove {
            index.remove(&path);
            if !cached {
                let disk = Path::new(&path);
                if disk.exists() {
                    fs::remove_file(disk)?;
                }
            }
            println!("rm '{}'", path);
            removed += 1;
        }
    }

    index.save()?;
    println!("已移除 {} 个条目", removed);
    Ok(())
}
