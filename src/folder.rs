use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderStatus {
    LeftOnly,
    RightOnly,
    Identical,
    Different,
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub relative_path: String,
    pub status: FolderStatus,
    pub left: Option<FileMetadata>,
    pub right: Option<FileMetadata>,
}

pub fn compare_folders(
    left: &Path,
    right: &Path,
    include_patterns: &[String],
    ignore_patterns: &[String],
) -> io::Result<Vec<FolderEntry>> {
    let mut ignored = ignore_patterns.to_vec();
    ignored.extend(read_ignore_file(left));
    ignored.extend(read_ignore_file(right));
    let mut left_files = BTreeMap::new();
    let mut right_files = BTreeMap::new();
    collect_files(left, left, &ignored, &mut left_files)?;
    collect_files(right, right, &ignored, &mut right_files)?;
    let paths: BTreeSet<_> = left_files
        .keys()
        .chain(right_files.keys())
        .cloned()
        .collect();

    Ok(paths
        .into_iter()
        .filter(|path| {
            include_patterns.is_empty()
                || include_patterns
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, path))
        })
        .map(|relative_path| {
            let left = left_files
                .get(&relative_path)
                .and_then(|path| file_metadata(path));
            let right = right_files
                .get(&relative_path)
                .and_then(|path| file_metadata(path));
            let status = match (
                left_files.get(&relative_path),
                right_files.get(&relative_path),
            ) {
                (Some(_), None) => FolderStatus::LeftOnly,
                (None, Some(_)) => FolderStatus::RightOnly,
                (Some(left), Some(right)) => match (file_metadata(left), file_metadata(right)) {
                    (Some(left_metadata), Some(right_metadata))
                        if left_metadata.size != right_metadata.size =>
                    {
                        FolderStatus::Different
                    }
                    (Some(_), Some(_)) => match files_equal(left, right) {
                        Ok(true) => FolderStatus::Identical,
                        Ok(false) => FolderStatus::Different,
                        Err(_) => FolderStatus::Unreadable,
                    },
                    _ => FolderStatus::Unreadable,
                },
                (None, None) => unreachable!(),
            };
            FolderEntry {
                relative_path,
                status,
                left,
                right,
            }
        })
        .collect())
}

fn files_equal(left: &Path, right: &Path) -> io::Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_buffer = [0; 64 * 1024];
    let mut right_buffer = [0; 64 * 1024];

    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn file_metadata(path: &Path) -> Option<FileMetadata> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileMetadata {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

pub fn synchronize_folders(
    source: &Path,
    destination: &Path,
    entries: &[FolderEntry],
) -> io::Result<usize> {
    let mut copied = 0;
    for entry in entries {
        let relative = Path::new(&entry.relative_path);
        let source_file = match checked_file_path(source, relative, false) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let source_metadata = match fs::symlink_metadata(&source_file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
            continue;
        }
        let destination_file = checked_file_path(destination, relative, true)?;
        let source_bytes = fs::read(&source_file)?;
        if fs::read(&destination_file)
            .is_ok_and(|destination_bytes| destination_bytes == source_bytes)
        {
            continue;
        }
        fs::copy(source_file, destination_file)?;
        copied += 1;
    }
    Ok(copied)
}

pub fn write_folder_file(root: &Path, relative: &str, contents: &str) -> io::Result<()> {
    let relative = Path::new(relative);
    let destination = checked_file_path(root, relative, true)?;
    fs::write(destination, contents)
}

fn checked_file_path(root: &Path, relative: &Path, create_parents: bool) -> io::Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid relative file path",
        ));
    }
    let canonical_root = fs::canonicalize(root)?;
    let mut parent = canonical_root.clone();
    let components: Vec<_> = relative.components().collect();
    for component in &components[..components.len() - 1] {
        let std::path::Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid relative file path",
            ));
        };
        parent.push(name);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "refusing to follow a symbolic link",
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a parent path is not a directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_parents => {
                fs::create_dir(&parent)?;
            }
            Err(error) => return Err(error),
        }
        parent = fs::canonicalize(parent)?;
        if !parent.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path resolves outside the selected folder",
            ));
        }
    }
    let std::path::Component::Normal(name) = components[components.len() - 1] else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid relative file path",
        ));
    };
    let file = parent.join(name);
    if fs::symlink_metadata(&file).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to follow a symbolic link",
        ));
    }
    Ok(file)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    ignored: &[String],
    output: &mut BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == ".git" || relative.starts_with(".git/") || is_ignored(&relative, ignored) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, ignored, output)?;
        } else if file_type.is_file() {
            output.insert(relative, path);
        }
    }
    Ok(())
}

fn read_ignore_file(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join(".gitignore"))
        .map(|contents| {
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn is_ignored(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let pattern = pattern.trim_start_matches('/');
        wildcard_matches(pattern, path)
            || path.split('/').enumerate().any(|(index, _)| {
                wildcard_matches(
                    pattern,
                    &path
                        .split('/')
                        .take(index + 1)
                        .collect::<Vec<_>>()
                        .join("/"),
                )
            })
            || pattern.ends_with('/')
                && (path == pattern.trim_end_matches('/')
                    || path
                        .strip_prefix(pattern.trim_end_matches('/'))
                        .is_some_and(|suffix| suffix.starts_with('/')))
    })
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    let (mut p, mut v, mut star, mut checkpoint) = (0, 0, None, 0);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            checkpoint = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            checkpoint += 1;
            v = checkpoint;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{FolderStatus, compare_folders, synchronize_folders};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "checker-folder-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn compares_recursive_files_and_honors_filters_and_gitignore() {
        let root = temp_root();
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("nested")).unwrap();
        fs::create_dir_all(right.join("nested")).unwrap();
        fs::write(left.join("same.txt"), "same").unwrap();
        fs::write(right.join("same.txt"), "same").unwrap();
        fs::write(left.join("nested/change.rs"), "left").unwrap();
        fs::write(right.join("nested/change.rs"), "right").unwrap();
        fs::write(left.join("same-size.txt"), "old").unwrap();
        fs::write(right.join("same-size.txt"), "new").unwrap();
        let left_large = vec![b'x'; 128 * 1024];
        let mut right_large = left_large.clone();
        right_large[127 * 1024] = b'y';
        fs::write(left.join("large.bin"), &left_large).unwrap();
        fs::write(right.join("large.bin"), &right_large).unwrap();
        fs::write(left.join("skip.log"), "hidden").unwrap();
        fs::write(left.join(".gitignore"), "*.log\n").unwrap();

        let entries = compare_folders(&left, &right, &[], &[]).unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.relative_path == "nested/change.rs")
                .unwrap()
                .status,
            FolderStatus::Different
        );
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.relative_path == "same-size.txt")
                .unwrap()
                .status,
            FolderStatus::Different
        );
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.relative_path == "large.bin")
                .unwrap()
                .status,
            FolderStatus::Different
        );
        let same = entries
            .iter()
            .find(|entry| entry.relative_path == "same.txt")
            .unwrap();
        assert_eq!(same.left.as_ref().unwrap().size, 4);
        assert_eq!(same.right.as_ref().unwrap().size, 4);
        let filtered = compare_folders(&left, &right, &["*.rs".into()], &[]).unwrap();
        assert_eq!(filtered.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn synchronizes_missing_or_different_files_to_destination() {
        let root = temp_root();
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("file.txt"), "new").unwrap();
        let entries = compare_folders(&left, &right, &[], &[]).unwrap();
        assert_eq!(synchronize_folders(&left, &right, &entries).unwrap(), 1);
        assert_eq!(fs::read_to_string(right.join("file.txt")).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn synchronization_does_not_follow_destination_symlinks() {
        use std::os::unix::fs::symlink;

        let root = temp_root();
        let left = root.join("left");
        let right = root.join("right");
        let outside = root.join("outside");
        fs::create_dir_all(left.join("linked")).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(left.join("linked/file.txt"), "contents").unwrap();
        symlink(&outside, right.join("linked")).unwrap();
        let entries = compare_folders(&left, &right, &[], &[]).unwrap();

        assert!(
            synchronize_folders(&left, &right, &entries)
                .unwrap_err()
                .to_string()
                .contains("symbolic link")
        );
        assert!(!outside.join("file.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
