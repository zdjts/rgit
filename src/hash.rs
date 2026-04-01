pub fn hex_to_bytes(hex: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = hex::decode(hex)?;
    Ok(bytes)
}
