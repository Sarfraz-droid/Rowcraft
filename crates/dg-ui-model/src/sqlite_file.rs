//! Import a browser-selected SQLite database into persistent application storage.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

pub const MAX_SQLITE_FILE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ImportedSqlite {
    pub path: String,
}

pub fn import(directory: &Path, bytes: &[u8]) -> Result<ImportedSqlite, String> {
    if bytes.len() > MAX_SQLITE_FILE_BYTES {
        return Err(
            "This database exceeds the 256 MB upload limit. Enter its local file path instead."
                .into(),
        );
    }
    if !bytes.starts_with(b"SQLite format 3\0") {
        return Err("Choose a SQLite database file (.db, .sqlite or .sqlite3). This file has no SQLite header.".into());
    }
    std::fs::create_dir_all(directory)
        .map_err(|e| format!("Cannot create database storage: {e}"))?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = directory.join(format!(
        "import-{stamp}-{}-{}.sqlite",
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
    let mut file = options.open(&path).map_err(|e| format!("Cannot import database: {e}"))?;
    let saved = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = saved.map_err(|e| format!("Cannot write database: {e}")).and_then(|()| {
        let conn = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| format!("Cannot open this SQLite database: {e}"))?;
        let check: String = conn
            .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))
            .map_err(|e| format!("This SQLite database is incomplete or damaged: {e}"))?;
        if check != "ok" {
            return Err(format!("This SQLite database is damaged: {check}"));
        }
        Ok(ImportedSqlite { path: path.to_string_lossy().into_owned() })
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_real_database_without_overwriting_and_cleans_up_invalid_files() {
        let root =
            std::env::temp_dir().join(format!("dglite-sqlite-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.db");
        let conn = rusqlite::Connection::open(&original).unwrap();
        conn.execute_batch("create table people(name text); insert into people values ('Ada');")
            .unwrap();
        drop(conn);
        let bytes = std::fs::read(&original).unwrap();
        let uploads = root.join("uploads");
        let first = import(&uploads, &bytes).unwrap();
        let second = import(&uploads, &bytes).unwrap();
        assert_ne!(first.path, second.path);
        let conn = rusqlite::Connection::open(&first.path).unwrap();
        assert_eq!(
            conn.query_row("select name from people", [], |r| r.get::<_, String>(0)).unwrap(),
            "Ada"
        );
        conn.execute("delete from people", []).unwrap();
        drop(conn);
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        assert!(import(&uploads, b"not a database").is_err());
        assert!(import(&uploads, &bytes[..100]).is_err());
        assert_eq!(std::fs::read_dir(&uploads).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
