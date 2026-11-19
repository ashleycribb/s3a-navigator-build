//! Path discipline: lexical normalization, component-prefix checks, and
//! the §5.2.2 shared safe-open rule (dirfd / openat O_NOFOLLOW).

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::RawFd;
use std::path::{Component, Path, PathBuf};

/// Lexically normalize an absolute path (resolve `.` and `..` only; no
/// symlink resolution). Returns `None` for a non-absolute path.
pub fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut out = PathBuf::from("/");
    for comp in path.components() {
        match comp {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

/// Is `path` equal to, or strictly inside, `root` (path-component prefix
/// after canonicalization, so `/x/a` and `/x/ab` are disjoint)?
pub fn is_within(root: &Path, path: &Path) -> bool {
    let mut r = root.components();
    let mut p = path.components();
    loop {
        match (r.next(), p.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(a), Some(b)) => {
                if a != b {
                    return false;
                }
            }
        }
    }
}

/// Are two paths disjoint (neither is a component-prefix of the other)?
pub fn are_disjoint(a: &Path, b: &Path) -> bool {
    !is_within(a, b) && !is_within(b, a)
}

/// `fstat` result for an open fd (or an `lstat`).
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub is_regular: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub uid: u32,
}

fn stat_from(raw: libc::stat) -> Stat {
    Stat {
        is_regular: (raw.st_mode & libc::S_IFMT) == libc::S_IFREG,
        is_dir: (raw.st_mode & libc::S_IFMT) == libc::S_IFDIR,
        is_symlink: (raw.st_mode & libc::S_IFMT) == libc::S_IFLNK,
        size: raw.st_size as u64,
        dev: raw.st_dev as u64,
        ino: raw.st_ino as u64,
        mode: raw.st_mode as u32,
        uid: raw.st_uid,
    }
}

/// `fstat` an open fd.
pub fn fstat_fd(fd: RawFd) -> std::io::Result<Stat> {
    let mut raw: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstat(fd, &mut raw) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(stat_from(raw))
}

/// `lstat` a path (does not follow symlinks).
pub fn lstat_path(path: &Path) -> std::io::Result<Stat> {
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let mut raw: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::lstat(c.as_ptr(), &mut raw) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(stat_from(raw))
}

/// `fstatat(dirfd, name, AT_SYMLINK_NOFOLLOW)`.
pub fn fstatat_nofollow(dirfd: RawFd, name: &str) -> std::io::Result<Stat> {
    let c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let mut raw: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::fstatat(
            dirfd,
            c.as_ptr(),
            &mut raw,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(stat_from(raw))
}

/// Open a directory by canonical path: `O_RDONLY|O_DIRECTORY|O_NOFOLLOW|
/// O_CLOEXEC`. Returns the raw fd (caller closes it).
pub fn open_dir_fd(canonical_dir: &Path) -> std::io::Result<RawFd> {
    let c = CString::new(canonical_dir.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in path"))?;
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd)
}

/// `openat(dirfd, name, O_RDONLY|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC)`, plus
/// `O_CREAT` mode `0600` when `create` is true. Returns the raw fd.
pub fn openat_child(dirfd: RawFd, name: &str, create: bool) -> std::io::Result<RawFd> {
    let c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
    if create {
        flags |= libc::O_CREAT;
    }
    let fd = unsafe { libc::openat(dirfd, c.as_ptr(), flags, 0o600) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd)
}

/// `openat(dirfd, name, O_RDWR|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC)` (+ `O_CREAT`
/// mode `0600` when `create` is true). For lock files and writable state.
pub fn openat_child_rw(dirfd: RawFd, name: &str, create: bool) -> std::io::Result<RawFd> {
    let c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let mut flags = libc::O_RDWR | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
    if create {
        flags |= libc::O_CREAT;
    }
    let fd = unsafe { libc::openat(dirfd, c.as_ptr(), flags, 0o600) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd)
}

/// Exclusive non-blocking `flock` (`LOCK_EX|LOCK_NB`) on an fd.
pub fn flock_ex_nb(fd: RawFd) -> std::io::Result<()> {
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Read at most `limit` bytes from an fd (stops at EOF or the limit;
/// unlike `read_fd_bounded` this does NOT error when the file is longer
/// than the limit). Uses `read` (advances the fd offset). Used for dossier
/// front-matter reads, where only the leading bytes matter.
pub fn read_fd_up_to(fd: RawFd, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 65536];
    while (out.len() as u64) < limit {
        let want = std::cmp::min(buf.len() as u64, limit - out.len() as u64) as usize;
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, want) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    Ok(out)
}

/// Read at most `cap + 1` bytes from an fd at offset 0 via `pread`.
/// The caller checks `len > cap` (a file that grows past the cap).
pub fn read_fd_bounded(fd: RawFd, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let mut offset: u64 = 0;
    let mut chunk = [0u8; 65536];
    loop {
        let n = unsafe {
            libc::pread(
                fd,
                chunk.as_mut_ptr() as *mut libc::c_void,
                chunk.len(),
                offset as libc::off_t,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            break;
        }
        let n = n as usize;
        offset += n as u64;
        out.extend_from_slice(&chunk[..n]);
        if out.len() as u64 > cap {
            // One more byte proves oversize; stop reading.
            break;
        }
    }
    Ok(out)
}

/// Close a raw fd (ignores errors).
pub fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

/// `unlinkat(dirfd, name, 0)`.
pub fn unlink_at(dirfd: RawFd, name: &str) -> std::io::Result<()> {
    let c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let rc = unsafe { libc::unlinkat(dirfd, c.as_ptr(), 0) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// `fsync` a dirfd (durability for renames inside it).
pub fn fsync_dir(dirfd: RawFd) -> std::io::Result<()> {
    let rc = unsafe { libc::fsync(dirfd) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// One entry of a directory listing via its fd (`/proc/self/fd/{fd}`).
/// `file_type` comes from `d_type`: symlinks are reported as symlinks,
/// never followed.
#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_file: bool,
}

/// List a directory through an already-open dirfd.
pub fn read_dir_entries(fd: RawFd) -> std::io::Result<Vec<DirEntryInfo>> {
    let path = format!("/proc/self/fd/{fd}");
    let rd = std::fs::read_dir(&path)?;
    let mut out = Vec::new();
    for e in rd {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        let ft = e.file_type()?;
        out.push(DirEntryInfo {
            name,
            is_dir: ft.is_dir(),
            is_symlink: ft.is_symlink(),
            is_file: ft.is_file(),
        });
    }
    Ok(out)
}

/// Canonicalize a path, returning the absolute canonical form.
/// (`std::fs::canonicalize` follows symlinks; callers use it only on paths
/// whose components they have already validated.)
pub fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(
            lexical_normalize(Path::new("/a/b/../c/./d")).unwrap(),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            lexical_normalize(Path::new("/../x")).unwrap(),
            PathBuf::from("/x")
        );
        assert!(lexical_normalize(Path::new("relative/x")).is_none());
    }

    #[test]
    fn within_is_component_prefix() {
        assert!(is_within(Path::new("/x/a"), Path::new("/x/a/b")));
        assert!(is_within(Path::new("/x/a"), Path::new("/x/a")));
        assert!(!is_within(Path::new("/x/a"), Path::new("/x/ab")));
        assert!(!is_within(Path::new("/x/a"), Path::new("/y/a")));
        assert!(are_disjoint(Path::new("/x/a"), Path::new("/x/ab")));
        assert!(!are_disjoint(Path::new("/x/a"), Path::new("/x/a/b")));
    }
}
