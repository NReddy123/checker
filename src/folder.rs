use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderStatus {
    LeftOnly,
    RightOnly,
    Identical,
    Different,
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub relative_path: String,
    pub status: FolderStatus,
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
    let paths: BTreeSet<_> = left_files.keys().chain(right_files.keys()).cloned().collect();

    Ok(paths
        .into_iter()
        .filter(|path| {
            include_patterns.is_empty()
                || include_patterns
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, path))
        })
        .map(|relative_path| {
            let status = match (left_files.get(&relative_path), right_files.get(&relative_path)) {
                (Some(_), None) => FolderStatus::LeftOnly,
                (None, Some(_)) => FolderStatus::RightOnly,
                (Some(left), Some(right)) => match (fs::read(left), fs::read(right)) {
                    (Ok(left), Ok(right)) if left == right => FolderStatus::Identical,
                    (Ok(_), Ok(_)) => FolderStatus::Different,
                    _ => FolderStatus::Unreadable,
                },
                (None, None) => unreachable!(),
            };
            FolderEntry {
                relative_path,
                status,
            }
        })
        .collect())
}

pub fn synchronize_folders(
    source: &Path,
    destination: &Path,
    entries: &[FolderEntry],
) -> io::Result<usize> {
    let destination_root = fs::canonicalize(destination)?;
    let mut copied = 0;
    for entry in entries {
        let relative = Path::new(&entry.relative_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            continue;
        }
        let source_file = source.join(relative);
        let destination_file = destination.join(relative);
        let source_metadata = match fs::symlink_metadata(&source_file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
            continue;
        }
        if fs::symlink_metadata(&destination_file)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to overwrite a symbolic link in the destination",
            ));
        }
        let source_bytes = fs::read(&source_file)?;
        if fs::read(&destination_file).is_ok_and(|destination_bytes| destination_bytes == source_bytes)
        {
            continue;
        }
        let parent = destination_file
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid destination"))?;
        fs::create_dir_all(parent)?;
        let canonical_parent = fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&destination_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "destination resolves outside the selected folder",
            ));
        }
        fs::copy(source_file, destination_file)?;
        copied += 1;
    }
    Ok(copied)
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
            || path
                .split('/')
                .enumerate()
                .any(|(index, _)| wildcard_matches(pattern, &path.split('/').take(index + 1).collect::<Vec<_>>().join("/")))
            || pattern.ends_with('/')
                && path.starts_with(pattern.trim_end_matches('/'))
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
        fs::write(left.join("skip.log"), "hidden").unwrap();
        fs::write(left.join(".gitignore"), "*.log\n").unwrap();

        let entries = compare_folders(&left, &right, &[], &[]).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].status, FolderStatus::Different);
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
}
