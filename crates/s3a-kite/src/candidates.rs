//! Candidate enumeration (§5.5): recomputed per request, no cache.
//!
//! Item 1: `{root}/papers/SRC-*.md` under the wiki root and the corpus
//! root (wiki wins on the same relative path). Front matter must be
//! present, parseable, and UTF-8; aliases/anchors are rejected; the
//! nesting limit applies; a present `source_id` must equal the stem, else
//! the dossier is omitted (AT-188). Item 2: live non-tombstone
//! `AcademicPaperRecord`s in every regular hyper-tile `*.s3a` under the
//! corpus root (marks applied; Quarantined files contribute none; Edge
//! files skipped; dedup by paper_id with bytewise-ascending canonical
//! path order, first record in tile order). An item-1 and an item-2 entry
//! with the same identity string merge into one list entry (page_path
//! from item 1, archive_path from item 2, title/source_url/on_disk from
//! item 1 — the SPEC's "no join" bars inventing a relationship between
//! *different* strings, not merging identical strings).

use std::collections::BTreeMap;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};

use s3a_pack_kite::{open_path, parse_front_matter_bytes, scholar_paper_records, KiteContext};

use crate::flags::{Canon, DirFds};
use crate::paths::{
    canonicalize, close_fd, fstat_fd, fstatat_nofollow, is_within, openat_child, read_dir_entries,
    read_fd_up_to,
};

/// One candidate as returned by GET /v1/candidates.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub title: Option<String>,
    pub source_url: Option<String>,
    pub on_disk: bool,
    pub page_path: Option<PathBuf>,
    pub archive_path: Option<PathBuf>,
    /// The raw front-matter `drive_path` (NOT exposed in the
    /// `/v1/candidates` JSON). The write path re-derives the DD-59
    /// resolution per request from this value.
    pub drive_path: Option<String>,
}

/// Enumerate candidates for a Scholar host. Files that cannot be read,
/// are not valid archives, or are Quarantined are silently skipped
/// (AT-218). The result is sorted by id (bytewise ascending).
pub fn list_candidates(
    canon: &Canon,
    dirfds: &DirFds,
    kite: &KiteContext,
    max_frame_pixels: u64,
    max_open_bytes: u64,
    pinned_keys: &[[u8; 32]],
) -> Vec<Candidate> {
    let mut by_id: BTreeMap<String, Candidate> = BTreeMap::new();
    let mut wiki_stems: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Item 1: wiki root first (it wins on the same relative path).
    if let (Some(wiki_root), Some(wiki_fd)) = (&canon.wiki_root, dirfds.wiki_root) {
        collect_dossiers(
            wiki_root,
            wiki_fd,
            canon,
            max_open_bytes,
            &mut by_id,
            &mut wiki_stems,
            true,
        );
    }
    if let (Some(corpus_root), Some(corpus_fd)) = (&canon.corpus_root, dirfds.corpus_root) {
        collect_dossiers(
            corpus_root,
            corpus_fd,
            canon,
            max_open_bytes,
            &mut by_id,
            &mut wiki_stems,
            false,
        );
    }
    // Item 2: live paper records in *.s3a under the corpus root.
    if let (Some(corpus_root), Some(corpus_fd)) = (&canon.corpus_root, dirfds.corpus_root) {
        let mut files: Vec<PathBuf> = Vec::new();
        walk_s3a_files(corpus_fd, &PathBuf::new(), &mut files);
        files.sort();
        for rel in files {
            let canonical = corpus_root.join(&rel);
            let fd = match open_relative(corpus_fd, &rel) {
                Some(fd) => fd,
                None => continue,
            };
            let handle = open_path(
                kite,
                fd,
                &canonical.to_string_lossy(),
                corpus_root,
                max_frame_pixels,
                max_open_bytes,
                pinned_keys.to_vec(),
                None,
                Some(rel.file_name().unwrap().to_string_lossy().into_owned()),
            );
            close_fd(fd);
            let Ok(handle) = handle else { continue };
            let Ok(ids) = scholar_paper_records(&handle) else {
                continue;
            };
            for id in ids {
                by_id
                    .entry(id.clone())
                    .and_modify(|c| {
                        if c.archive_path.is_none() {
                            c.archive_path = Some(canonical.clone());
                        }
                    })
                    .or_insert_with(|| Candidate {
                        id,
                        title: None,
                        source_url: None,
                        on_disk: false,
                        page_path: None,
                        archive_path: Some(canonical.clone()),
                        drive_path: None,
                    });
            }
        }
    }
    by_id.into_values().collect()
}

/// Collect `{root}/papers/SRC-*.md` dossiers into `by_id`. `is_wiki`
/// marks the wiki pass (its stems shadow the corpus pass).
fn collect_dossiers(
    root: &Path,
    root_fd: RawFd,
    canon: &Canon,
    max_open_bytes: u64,
    by_id: &mut BTreeMap<String, Candidate>,
    wiki_stems: &mut std::collections::HashSet<String>,
    is_wiki: bool,
) {
    let Ok(papers_fd) = openat_child(root_fd, "papers", false) else {
        return;
    };
    let entries = match read_dir_entries(papers_fd) {
        Ok(e) => e,
        Err(_) => {
            close_fd(papers_fd);
            return;
        }
    };
    for entry in entries {
        if entry.is_symlink || !entry.is_file {
            continue;
        }
        if !entry.name.starts_with("SRC-") || !entry.name.ends_with(".md") {
            continue;
        }
        let stem = entry.name.trim_end_matches(".md").to_string();
        if !is_wiki && wiki_stems.contains(&stem) {
            continue; // wiki wins on the same relative path
        }
        // lstat the file: regular only (the dir listing already said file,
        // but re-check through the dirfd).
        let st = match fstatat_nofollow(papers_fd, &entry.name) {
            Ok(st) => st,
            Err(_) => continue,
        };
        if st.is_symlink || !st.is_regular {
            continue;
        }
        let fd = match openat_child(papers_fd, &entry.name, false) {
            Ok(fd) => fd,
            Err(_) => continue,
        };
        let bytes = read_fd_up_to(fd, max_open_bytes);
        close_fd(fd);
        let Ok(bytes) = bytes else { continue };
        let fm = parse_front_matter_bytes(utf8_prefix(&bytes));
        let Some(fm) = fm else { continue };
        if let Some(source_id) = &fm.source_id {
            if source_id != &stem {
                continue; // AT-188: source_id must equal the stem
            }
        }
        let canonical = root.join("papers").join(&entry.name);
        let source_url = fm
            .source_url
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"));
        let on_disk = drive_path_on_disk(&fm.drive_path, &canonical, canon);
        if is_wiki {
            wiki_stems.insert(stem.clone());
        }
        by_id
            .entry(stem.clone())
            .and_modify(|c| {
                if c.page_path.is_none() {
                    c.page_path = Some(canonical.clone());
                    c.title = fm.title.clone();
                    c.source_url = source_url.clone();
                    c.on_disk = on_disk;
                    c.drive_path = fm.drive_path.clone();
                }
            })
            .or_insert_with(|| Candidate {
                id: stem,
                title: fm.title.clone(),
                source_url,
                on_disk,
                page_path: Some(canonical.clone()),
                archive_path: None,
                drive_path: fm.drive_path.clone(),
            });
    }
    close_fd(papers_fd);
}

/// AT-150: a relative drive_path is resolved against the dossier's parent
/// directory and then must canonicalize inside an allowed root (wiki or
/// corpus); a relative path that escapes is not an on-disk source. The
/// dossier `.md` itself is never the on-disk source. A drive_path landing
/// in --snapshot-dir or --download-dir is not usable.
fn drive_path_on_disk(drive_path: &Option<String>, dossier: &Path, canon: &Canon) -> bool {
    let Some(drive) = drive_path else {
        return false;
    };
    if drive.is_empty() {
        return false;
    }
    let resolved = if Path::new(drive).is_absolute() {
        PathBuf::from(drive)
    } else {
        let parent = dossier.parent().unwrap_or(Path::new("/"));
        parent.join(drive)
    };
    let Ok(canonical) = canonicalize(&resolved) else {
        return false;
    };
    if canonical == dossier {
        return false;
    }
    if !std::fs::metadata(&canonical).map(|m| m.is_file()).unwrap_or(false) {
        return false;
    }
    // Not usable inside snapshot/download dirs.
    for dir in [&canon.snapshot_dir, &canon.download_dir] {
        if let Some(d) = dir {
            if is_within(d, &canonical) || canonical == *d {
                return false;
            }
        }
    }
    // Must be inside an allowed root (wiki or corpus).
    let mut inside = false;
    for root in [&canon.wiki_root, &canon.corpus_root] {
        if let Some(r) = root {
            if is_within(r, &canonical) {
                inside = true;
            }
        }
    }
    inside
}

/// Read the front matter of a dossier at a relative path under a root
/// dirfd. Returns `None` when the file cannot be opened, is not regular,
/// or its front matter is missing/unparseable. (Used by the host's
/// `shadowed_by_wiki` check; the caller applies the `source_id` rule.)
pub fn dossier_front_matter_at(
    root_fd: RawFd,
    rel: &Path,
    max_open_bytes: u64,
) -> Option<s3a_pack_kite::FrontMatter> {
    let fd = open_relative(root_fd, rel)?;
    let st = fstat_fd(fd).ok()?;
    if !st.is_regular {
        close_fd(fd);
        return None;
    }
    let bytes = read_fd_up_to(fd, max_open_bytes).ok()?;
    close_fd(fd);
    parse_front_matter_bytes(utf8_prefix(&bytes))
}

/// Recursively walk `*.s3a` regular files under a dirfd, skipping
/// symlinks (files and directories). `rel` is the path relative to the
/// root fd, accumulated as we descend.
fn walk_s3a_files(dirfd: RawFd, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = read_dir_entries(dirfd) else {
        return;
    };
    for entry in entries {
        if entry.is_symlink {
            continue;
        }
        if entry.is_dir {
            let Ok(child) = openat_child(dirfd, &entry.name, false) else {
                continue;
            };
            let st = fstat_fd(child);
            if !matches!(st, Ok(ref s) if s.is_dir) {
                close_fd(child);
                continue;
            }
            walk_s3a_files(child, &rel.join(&entry.name), out);
            close_fd(child);
        } else if entry.is_file && entry.name.ends_with(".s3a") {
            out.push(rel.join(&entry.name));
        }
    }
}

/// Open a relative path (components separated by `/`) through a dirfd,
/// one component at a time with O_NOFOLLOW. Returns None on any failure.
pub fn open_relative(root_fd: RawFd, rel: &Path) -> Option<RawFd> {
    let mut fd = root_fd;
    let mut owned: Vec<RawFd> = Vec::new();
    for component in rel.components() {
        let name = component.as_os_str().to_string_lossy().into_owned();
        let next = openat_child(fd, &name, false).ok()?;
        if fd != root_fd {
            owned.push(fd);
        }
        fd = next;
    }
    for old in owned {
        close_fd(old);
    }
    Some(fd)
}

/// The DD-146 walk with the final component opened O_RDWR instead of
/// O_RDONLY (intermediate components stay O_RDONLY). Used only when the
/// `grow_drive_path_during_copy` hook is configured, so the host can
/// extend the already-opened `drive_path` itself (§5.2.7 line 523).
pub fn open_relative_rw(root_fd: RawFd, rel: &Path) -> Option<RawFd> {
    let mut fd = root_fd;
    let mut owned: Vec<RawFd> = Vec::new();
    let mut components = rel.components().peekable();
    while let Some(component) = components.next() {
        let name = component.as_os_str().to_string_lossy().into_owned();
        let next = if components.peek().is_none() {
            crate::paths::openat_child_rw(fd, &name, false).ok()?
        } else {
            openat_child(fd, &name, false).ok()?
        };
        if fd != root_fd {
            owned.push(fd);
        }
        fd = next;
    }
    for old in owned {
        close_fd(old);
    }
    Some(fd)
}

/// Trim a byte prefix to the last valid UTF-8 boundary (so a truncated
/// read that ends mid-character still parses its leading front matter).
fn utf8_prefix(bytes: &[u8]) -> &[u8] {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes,
        Err(e) => &bytes[..e.valid_up_to()],
    }
}
