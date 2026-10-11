use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

/// Fixed heap buffer: never allocate the whole media file or a large stack frame.
pub fn digest_file(path: &Path) -> Result<(u64, String), String> {
    digest_file_controlled(path, &|| false)
}

pub fn digest_file_controlled(
    path: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<(u64, String), String> {
    let mut source = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hash = Sha256::new();
    let mut size = 0;
    loop {
        if stop() {
            return Err("文件校验已停止".into());
        }
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hash.update(&buffer[..count]);
    }
    Ok((size, format!("{:x}", hash.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes_across_buffer_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video.bin");
        let bytes = vec![17_u8; 64 * 1024 * 3 + 71];
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            digest_file(&path).unwrap(),
            (bytes.len() as u64, format!("{:x}", Sha256::digest(&bytes)))
        );
    }
}
