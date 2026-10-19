//! Store uploaded JDBC libraries independently of their original location.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_JAR_FILE_BYTES: usize = 256 * 1024 * 1024;

pub fn import(directory: &Path, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_JAR_FILE_BYTES {
        return Err(
            "This JAR exceeds the 256 MB upload limit. Enter its local file path instead.".into()
        );
    }
    // JARs are ZIP archives. The Java runtime validates their contents when loading the driver.
    if bytes.len() < 30 || !bytes.starts_with(b"PK\x03\x04") {
        return Err("Choose a JDBC driver JAR file. This file has no ZIP/JAR header.".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| format!("Cannot create driver storage: {e}"))?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = directory.join(format!(
        "driver-{stamp}-{}-{}.jar",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|e| format!("Cannot import JAR: {e}"))?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&path);
        return Err(format!("Cannot write JAR: {e}"));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_bytes_without_overwriting_and_rejects_non_archives() {
        let root = std::env::temp_dir().join(format!("rowcraft-jar-import-{}", std::process::id()));
        let mut bytes = vec![0; 30];
        bytes[..4].copy_from_slice(b"PK\x03\x04");
        let first = import(&root, &bytes).unwrap();
        let second = import(&root, &bytes).unwrap();
        assert_ne!(first, second);
        assert!(Path::new(&first).is_absolute());
        assert_eq!(std::fs::read(first).unwrap(), bytes);
        assert!(import(&root, b"not a jar").is_err());
        assert!(import(&root, b"PK\x03\x04").is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
