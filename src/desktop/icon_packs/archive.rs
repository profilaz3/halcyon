//! Safe extraction of downloaded icon pack archives.
//!
//! Entries are written one by one, never with a library's own unpack: every
//! path must be relative and stay inside the destination, nothing is written
//! through a link, links may only point inside the archive, device files are
//! refused, and the unpacked size and file count are capped. After the walk
//! every link is resolved once more, so a chain of links that ends outside
//! the destination fails the whole archive.
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

/// The largest download accepted.
pub const DOWNLOAD_LIMIT: u64 = 300 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Total bytes written, files and link copies together.
    pub bytes: u64,
    /// Entries of any kind.
    pub entries: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 2 * 1024 * 1024 * 1024,
            entries: 400_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    TarGz,
    TarXz,
    TarBz2,
    Tar,
    Zip,
    Unknown,
}

/// The format from the first bytes of a file, whatever its name says.
pub fn sniff(head: &[u8]) -> Format {
    if head.starts_with(&[0x1f, 0x8b]) {
        Format::TarGz
    } else if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
        Format::TarXz
    } else if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Format::Zip
    } else if head.starts_with(b"BZh") {
        Format::TarBz2
    } else if head.len() >= 262 && &head[257..262] == b"ustar" {
        Format::Tar
    } else {
        Format::Unknown
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: u64,
    pub bytes: u64,
}

struct Budget {
    limits: Limits,
    stats: Stats,
    entries: u64,
}

impl Budget {
    fn entry(&mut self) -> Result<(), String> {
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(format!(
                "The pack has more than {} files",
                self.limits.entries
            ));
        }
        Ok(())
    }
    fn room(&self) -> u64 {
        self.limits.bytes.saturating_sub(self.stats.bytes)
    }
    fn too_large(&self) -> String {
        format!(
            "The pack unpacks to more than {} MB",
            self.limits.bytes / 1024 / 1024
        )
    }
}

/// Unpacks `archive` into the empty directory `dest`. A compressed tar is
/// first expanded next to `dest`, in a temporary file under the same parent.
pub fn extract(archive: &Path, dest: &Path, limits: Limits) -> Result<Stats, String> {
    let mut file =
        fs::File::open(archive).map_err(|error| format!("Can’t open the download: {error}"))?;
    let mut head = [0u8; 512];
    let read = read_head(&mut file, &mut head).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    let mut budget = Budget {
        limits,
        stats: Stats::default(),
        entries: 0,
    };
    match sniff(&head[..read]) {
        Format::TarGz => {
            let reader = flate2::read::MultiGzDecoder::new(BufReader::new(file));
            tar_entries(reader, dest, &mut budget)?;
        }
        Format::TarXz => {
            let parent = dest.parent().unwrap_or(Path::new("."));
            let temp = parent.join(format!(".halcyon-xz-{}.tar", super::unique()));
            let result = (|| {
                let out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)
                    .map_err(|error| error.to_string())?;
                let mut out = Capped {
                    inner: io::BufWriter::new(out),
                    left: limits.bytes.saturating_add(64 * 1024 * 1024),
                };
                lzma_rs::xz_decompress(&mut BufReader::new(file), &mut out).map_err(|error| {
                    if out.left == 0 {
                        budget.too_large()
                    } else {
                        format!("The pack is damaged: {error}")
                    }
                })?;
                out.inner.flush().map_err(|error| error.to_string())?;
                drop(out);
                let tar = fs::File::open(&temp).map_err(|error| error.to_string())?;
                tar_entries(BufReader::new(tar), dest, &mut budget)
            })();
            let _ = fs::remove_file(&temp);
            result?;
        }
        Format::Tar => tar_entries(BufReader::new(file), dest, &mut budget)?,
        Format::Zip => zip_entries(file, dest, &mut budget)?,
        Format::TarBz2 => {
            return Err("This pack is a bzip2 archive, which Halcyon can’t open yet".into());
        }
        Format::Unknown => return Err("The download isn’t an archive Halcyon can open".into()),
    }
    check_links(dest)?;
    Ok(budget.stats)
}

fn read_head(file: &mut fs::File, head: &mut [u8]) -> io::Result<usize> {
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..])? {
            0 => break,
            n => read += n,
        }
    }
    Ok(read)
}

/// A writer that fails once `left` bytes are used.
struct Capped<W> {
    inner: W,
    left: u64,
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() as u64 > self.left {
            self.left = 0;
            return Err(io::Error::other("too large"));
        }
        let written = self.inner.write(buf)?;
        self.left -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// An archive path as a relative path of plain components, `None` for the
/// archive root itself. Absolute paths and `..` are refused.
pub fn relative(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut out = PathBuf::new();
    let mut depth = 0;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                if part.len() > 255 {
                    return Err("The pack has a name that is too long".into());
                }
                out.push(part);
                depth += 1;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "The pack has a path that leaves its folder ({})",
                    path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "The pack has an absolute path ({})",
                    path.display()
                ));
            }
        }
    }
    if depth > 48 {
        return Err("The pack nests folders too deeply".into());
    }
    Ok((depth > 0).then_some(out))
}

/// Creates the parents of `dest/rel`, refusing to pass through a link or a
/// file.
fn parents(dest: &Path, rel: &Path) -> Result<(), String> {
    let mut path = dest.to_path_buf();
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    for part in parent.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "The pack writes through a link ({})",
                    rel.display()
                ));
            }
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "The pack uses a file as a folder ({})",
                    rel.display()
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

/// Clears the way for a new file or link at `path`: a later entry replaces
/// an earlier file or link, never a folder.
fn clear(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Err(format!(
            "The pack replaces a folder with a file ({})",
            path.display()
        )),
        Ok(_) => fs::remove_file(path).map_err(|error| error.to_string()),
        Err(_) => Ok(()),
    }
}

fn make_dir(dest: &Path, rel: &Path) -> Result<(), String> {
    parents(dest, rel)?;
    let path = dest.join(rel);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "The pack replaces a file with a folder ({})",
            rel.display()
        )),
        Err(_) => fs::create_dir(&path).map_err(|error| error.to_string()),
    }
}

fn make_file(
    dest: &Path,
    rel: &Path,
    reader: &mut dyn Read,
    executable: bool,
    budget: &mut Budget,
) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    parents(dest, rel)?;
    let path = dest.join(rel);
    clear(&path)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(if executable { 0o755 } else { 0o644 })
        .open(&path)
        .map_err(|error| error.to_string())?;
    let room = budget.room();
    let mut out = io::BufWriter::new(file);
    let copied = io::copy(&mut reader.take(room + 1), &mut out)
        .map_err(|error| format!("The pack is damaged: {error}"))?;
    if copied > room {
        return Err(budget.too_large());
    }
    out.flush().map_err(|error| error.to_string())?;
    budget.stats.bytes += copied;
    budget.stats.files += 1;
    Ok(())
}

/// The link target's lexical place inside the archive, or an error when it
/// is absolute, leaves the archive or names the archive root.
pub fn link_inside(rel: &Path, target: &Path) -> Result<PathBuf, String> {
    let mut place: Vec<&std::ffi::OsStr> = rel
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|part| match part {
                    Component::Normal(part) => Some(part),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    for component in target.components() {
        match component {
            Component::Normal(part) => place.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if place.pop().is_none() {
                    return Err(format!(
                        "The pack has a link that leaves its folder ({})",
                        rel.display()
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "The pack has a link to an absolute path ({})",
                    rel.display()
                ));
            }
        }
    }
    if place.is_empty() {
        return Err(format!(
            "The pack has a link to its own folder ({})",
            rel.display()
        ));
    }
    Ok(place.iter().collect())
}

fn make_symlink(dest: &Path, rel: &Path, target: &Path, budget: &mut Budget) -> Result<(), String> {
    if target.as_os_str().is_empty() || target.as_os_str().len() > 1024 {
        return Err(format!("The pack has a broken link ({})", rel.display()));
    }
    link_inside(rel, target)?;
    parents(dest, rel)?;
    let path = dest.join(rel);
    clear(&path)?;
    std::os::unix::fs::symlink(target, &path).map_err(|error| error.to_string())?;
    budget.stats.files += 1;
    Ok(())
}

/// A hard link becomes a copy of a regular file already unpacked.
fn make_hardlink(
    dest: &Path,
    rel: &Path,
    source: &Path,
    budget: &mut Budget,
) -> Result<(), String> {
    let source = relative(source)?
        .ok_or_else(|| format!("The pack has a broken link ({})", rel.display()))?;
    let root = dest.canonicalize().map_err(|error| error.to_string())?;
    let source = dest
        .join(source)
        .canonicalize()
        .map_err(|_| format!("The pack has a broken link ({})", rel.display()))?;
    if !source.starts_with(&root) || !source.is_file() {
        return Err(format!(
            "The pack has a link that leaves its folder ({})",
            rel.display()
        ));
    }
    let mut file = fs::File::open(&source).map_err(|error| error.to_string())?;
    make_file(dest, rel, &mut file, false, budget)
}

fn tar_entries<R: Read>(reader: R, dest: &Path, budget: &mut Budget) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|error| format!("The pack is damaged: {error}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("The pack is damaged: {error}"))?;
        budget.entry()?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
            || kind.is_gnu_longname()
            || kind.is_gnu_longlink()
        {
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| format!("The pack is damaged: {error}"))?
            .into_owned();
        let Some(rel) = relative(&path)? else {
            continue;
        };
        if kind.is_character_special() || kind.is_block_special() || kind.is_fifo() {
            return Err(format!(
                "The pack contains a device file ({})",
                rel.display()
            ));
        }
        if kind.is_dir() {
            make_dir(dest, &rel)?;
        } else if kind.is_symlink() || kind.is_hard_link() {
            let target = entry
                .link_name()
                .map_err(|error| format!("The pack is damaged: {error}"))?
                .ok_or_else(|| format!("The pack has a broken link ({})", rel.display()))?
                .into_owned();
            if kind.is_symlink() {
                make_symlink(dest, &rel, &target, budget)?;
            } else {
                make_hardlink(dest, &rel, &target, budget)?;
            }
        } else if kind.is_file() || kind.is_contiguous() {
            let executable = entry.header().mode().is_ok_and(|mode| mode & 0o111 != 0);
            make_file(dest, &rel, &mut entry, executable, budget)?;
        } else if kind.is_gnu_sparse() {
            return Err(format!(
                "The pack has a sparse file Halcyon can’t unpack ({})",
                rel.display()
            ));
        }
        // Other kinds (volume labels, vendor extensions) carry no files.
    }
    Ok(())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A small zip reader: the central directory, then each entry's local
/// header. Stored and deflated entries only; zip64 and encryption are
/// refused.
fn zip_entries(mut file: fs::File, dest: &Path, budget: &mut Budget) -> Result<(), String> {
    let damaged = |what: &str| format!("The pack is damaged ({what})");
    let len = file.metadata().map_err(|error| error.to_string())?.len();
    let tail_len = len.min(22 + 65_535);
    let mut tail = vec![0; tail_len as usize];
    file.seek(SeekFrom::Start(len - tail_len))
        .and_then(|_| file.read_exact(&mut tail))
        .map_err(|error| error.to_string())?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&at| tail[at..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| damaged("no zip directory"))?;
    let record = &tail[end..];
    let count = u16_at(record, 10);
    let size = u32_at(record, 12);
    let offset = u32_at(record, 16);
    if count == u16::MAX || size == u32::MAX || offset == u32::MAX {
        return Err("This pack is a zip64 archive, which Halcyon can’t open yet".into());
    }
    if u64::from(offset) + u64::from(size) > len || size > 128 * 1024 * 1024 {
        return Err(damaged("zip directory out of range"));
    }
    let mut directory = vec![0; size as usize];
    file.seek(SeekFrom::Start(offset.into()))
        .and_then(|_| file.read_exact(&mut directory))
        .map_err(|_| damaged("zip directory"))?;
    let mut at = 0;
    for _ in 0..count {
        if at + 46 > directory.len() || !directory[at..].starts_with(b"PK\x01\x02") {
            return Err(damaged("zip entry"));
        }
        let entry = &directory[at..];
        let host = u16_at(entry, 4) >> 8;
        let flags = u16_at(entry, 8);
        let method = u16_at(entry, 10);
        let crc = u32_at(entry, 16);
        let compressed = u32_at(entry, 20);
        let plain = u32_at(entry, 24);
        let name_len = usize::from(u16_at(entry, 28));
        let extra_len = usize::from(u16_at(entry, 30));
        let comment_len = usize::from(u16_at(entry, 32));
        let attributes = u32_at(entry, 38);
        let local = u32_at(entry, 42);
        if 46 + name_len > entry.len() {
            return Err(damaged("zip name"));
        }
        let raw = &entry[46..46 + name_len];
        at += 46 + name_len + extra_len + comment_len;
        budget.entry()?;
        if compressed == u32::MAX || plain == u32::MAX || local == u32::MAX {
            return Err("This pack is a zip64 archive, which Halcyon can’t open yet".into());
        }
        if flags & 1 != 0 {
            return Err("This pack is encrypted".into());
        }
        if raw.contains(&0) {
            return Err(damaged("zip name"));
        }
        let name = String::from_utf8_lossy(raw).replace('\\', "/");
        let Some(rel) = relative(Path::new(&name))? else {
            continue;
        };
        let mode = if host == 3 { attributes >> 16 } else { 0 };
        let kind = mode & 0o170_000;
        if matches!(kind, 0o020_000 | 0o060_000 | 0o010_000 | 0o140_000) {
            return Err(format!(
                "The pack contains a device file ({})",
                rel.display()
            ));
        }
        if name.ends_with('/') || kind == 0o040_000 {
            make_dir(dest, &rel)?;
            continue;
        }
        // The data starts after the local header, whose name and extra
        // field lengths may differ from the directory's.
        let mut header = [0u8; 30];
        file.seek(SeekFrom::Start(local.into()))
            .and_then(|_| file.read_exact(&mut header))
            .map_err(|_| damaged("zip entry header"))?;
        if !header.starts_with(b"PK\x03\x04") {
            return Err(damaged("zip entry header"));
        }
        let data =
            u64::from(local) + 30 + u64::from(u16_at(&header, 26)) + u64::from(u16_at(&header, 28));
        if data + u64::from(compressed) > len {
            return Err(damaged("zip entry out of range"));
        }
        file.seek(SeekFrom::Start(data))
            .map_err(|error| error.to_string())?;
        let raw_data = (&mut file).take(compressed.into());
        let mut reader: Box<dyn Read + '_> = match method {
            0 => Box::new(raw_data),
            8 => Box::new(flate2::read::DeflateDecoder::new(raw_data)),
            _ => {
                return Err(format!(
                    "The pack uses a zip compression Halcyon can’t open ({})",
                    rel.display()
                ));
            }
        };
        let mut checked = Checked {
            inner: &mut reader,
            crc: flate2::Crc::new(),
        };
        if kind == 0o120_000 {
            let mut target = Vec::new();
            (&mut checked)
                .take(1025)
                .read_to_end(&mut target)
                .map_err(|_| damaged("zip link"))?;
            if checked.crc.sum() != crc || target.contains(&0) {
                return Err(damaged("zip link"));
            }
            let target = PathBuf::from(String::from_utf8_lossy(&target).into_owned());
            make_symlink(dest, &rel, &target, budget)?;
        } else {
            make_file(dest, &rel, &mut checked, mode & 0o111 != 0, budget)?;
            if checked.crc.sum() != crc {
                return Err(damaged(&format!("checksum of {}", rel.display())));
            }
        }
    }
    Ok(())
}

/// Computes the CRC-32 of what passes through.
struct Checked<'a> {
    inner: &'a mut dyn Read,
    crc: flate2::Crc,
}

impl Read for Checked<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.crc.update(&buf[..read]);
        Ok(read)
    }
}

/// Resolves every link under `dest`: a dangling one is removed, and one
/// that ends outside `dest` (through a chain of links) fails the archive.
pub fn check_links(dest: &Path) -> Result<(), String> {
    let root = dest.canonicalize().map_err(|error| error.to_string())?;
    let mut stack = vec![dest.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|error| error.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_symlink() {
                match path.canonicalize() {
                    Ok(target) if target.starts_with(&root) && target != root => {}
                    Ok(_) => {
                        let rel = path.strip_prefix(dest).unwrap_or(&path);
                        return Err(format!(
                            "The pack has a link that leaves its folder ({})",
                            rel.display()
                        ));
                    }
                    Err(_) => {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }
    Ok(())
}
