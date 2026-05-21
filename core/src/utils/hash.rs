// utils/hash.rs - SHA256 / MD5 计算
use sha2::{Digest, Sha256};

/// 计算字节数组的 SHA256 哈希，返回十六进制字符串
pub fn sha256_string(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// 计算文件的 SHA256 哈希
pub fn sha256_file(path: &std::path::Path) -> anyhow::Result<String> {
    use std::fs::File;
    use std::io::{BufReader, Read};

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}
