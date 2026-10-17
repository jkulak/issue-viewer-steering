//! Reading and writing issue files on disk.
//!
//! Other programs edit the same files while the viewer runs, so a write goes to a temporary file
//! next to the target and replaces it in one rename: a reader sees the old file or the new one,
//! never half of it. The temporary name starts with a dot, so it never looks like an issue.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn digest(text: &str) -> String {
    let mut full = hex(&Sha256::digest(text.as_bytes()));
    full.truncate(16);
    full
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileText {
    /// Newlines normalised to `\n`.
    pub text: String,
    /// The newline the file uses on disk.
    pub newline: &'static str,
    /// Digest of `text`: what a form sends back to prove it saw this version.
    pub version: String,
    /// False when the bytes were not UTF-8 and had to be replaced.
    pub readable: bool,
}

impl FileText {
    pub fn new(text: &str) -> Self {
        decode(text.as_bytes().to_vec())
    }
}

pub fn decode(raw: Vec<u8>) -> FileText {
    let (text, readable) = match String::from_utf8(raw) {
        Ok(text) => (text, true),
        Err(error) => (
            String::from_utf8_lossy(error.as_bytes()).into_owned(),
            false,
        ),
    };
    let text = text
        .strip_prefix('\u{feff}')
        .map(str::to_string)
        .unwrap_or(text);
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let text = if newline == "\r\n" {
        text.replace("\r\n", "\n")
    } else {
        text
    };
    let version = digest(&text);
    FileText {
        text,
        newline,
        version,
        readable,
    }
}

pub fn read(path: &Path) -> io::Result<FileText> {
    Ok(decode(fs::read(path)?))
}

fn encode(text: &str, newline: &str) -> Vec<u8> {
    if newline == "\n" {
        text.as_bytes().to_vec()
    } else {
        text.replace('\n', newline).into_bytes()
    }
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the system has no source of randomness");
    hex(&buffer)
}

/// Replace `path` with `text` in one rename, keeping the newline style and the file mode. A
/// symbolic link is written through, so it stays a link.
pub fn write_atomic(path: &Path, text: &str, newline: &str) -> io::Result<()> {
    let target: PathBuf = match fs::canonicalize(path) {
        Ok(resolved) => resolved,
        Err(_) => path.to_path_buf(),
    };
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("issue");
    let temporary = target.with_file_name(format!(".{name}.{}.tmp", random_hex(4)));
    let mode = fs::metadata(&target)
        .ok()
        .map(|m| m.permissions().mode() & 0o7777);
    let result = (|| {
        let mut handle = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        handle.write_all(&encode(text, newline))?;
        handle.sync_all()?;
        if let Some(mode) = mode {
            fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
        }
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Write a new file, failing if one with that name appeared in the meantime.
pub fn create_exclusive(path: &Path, text: &str) -> io::Result<()> {
    let mut handle = OpenOptions::new().write(true).create_new(true).open(path)?;
    handle.write_all(&encode(text, "\n"))?;
    handle.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_normalises_newlines_and_remembers_them() {
        let file = decode(b"\xef\xbb\xbfa\r\nb\r\n".to_vec());
        assert_eq!(file.text, "a\nb\n");
        assert_eq!(file.newline, "\r\n");
        assert!(file.readable);
        assert_eq!(file.version, digest("a\nb\n"));
    }

    #[test]
    fn bytes_that_are_not_utf8_are_replaced_and_flagged() {
        let file = decode(b"caf\xe9\n".to_vec());
        assert_eq!(file.text, "caf\u{fffd}\n");
        assert!(!file.readable);
    }

    #[test]
    fn an_atomic_write_keeps_crlf_and_the_mode_and_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("001-x.md");
        fs::write(&path, "old\r\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&path, "new\nline\n", "\r\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new\r\nline\r\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn writing_through_a_symbolic_link_keeps_the_link() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real.md");
        let link = directory.path().join("001-link.md");
        fs::write(&real, "old\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, "new\n", "\n").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new\n");
    }

    #[test]
    fn creating_a_file_that_exists_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("001-x.md");
        create_exclusive(&path, "a\n").unwrap();
        assert_eq!(
            create_exclusive(&path, "b\n").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }
}
