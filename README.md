# Checker

Checker is a native Rust desktop app for comparing UTF-8 text files and folder
trees. It uses [GPUI Kit](https://github.com/longbridge/gpui-kit) and shows
line-numbered text changes and recursive folder differences.

## Run

```sh
cargo run
```

Enter two file paths or two folder paths and select **Compare**. For folders,
Checker lists files recursively and reports left-only, right-only, identical,
changed, or unreadable files. The include and ignore fields accept comma-
separated `*`/`?` path patterns; `.git` folders and patterns in either root's
`.gitignore` are also ignored. Folder synchronization copies files from the
chosen side to the other without deleting destination-only files.

For text comparisons, use the whitespace, case, and comment toggles to control
line matching. Search filters visible diff rows, and the star controls let you
bookmark and navigate between rows. Changed lines are paired and show
character-level differences; basic syntax cues are provided for comments and
common code constructs.

The optional base-file field enables a three-way merge. Non-overlapping edits
are combined, overlapping edits are marked with conflict markers, and the
editable result can be saved to the right-side file. The editor can also load
either file for manual changes. **Copy** and **Sync** actions write to disk;
review the selected paths before using them. Binary diff viewers, remote file
systems, and deletion-based mirroring are not supported.

## Validate

```sh
cargo test
cargo check
```