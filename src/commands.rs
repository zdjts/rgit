use anyhow::Ok;
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::io::Write;
use std::{fs, path::Path};

use crate::object::{CommitObject, read_commit};

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
