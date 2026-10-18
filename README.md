# Checker

Checker is a native Rust desktop app for comparing UTF-8 text files and folder
trees. It uses [GPUI Kit](https://github.com/longbridge/gpui-kit) and shows
line-numbered text changes and recursive folder differences.

## Run

```sh
cargo run
```

Choose **Folder Compare**, **File Compare**, or **Text Compare** from the home
screen. Folder and file comparisons have **Browse** buttons that open the
system file/folder picker; paths can also be entered directly. Text Compare
starts with left and right input panes; after **Compare**, editing happens in
the aligned diff. Use **+ New** to start another comparison session; switching
tabs preserves each session's sources, results, and options.

Folder Compare lists files recursively and reports left-only, right-only,
identical, changed, or unreadable files. Results use aligned left/right columns
with file sizes and modification times, plus All, Diffs, and Same filters.
Include and ignore fields accept comma-separated `*`/`?` path patterns; `.git`
folders and simple patterns in either root's `.gitignore` are also ignored.
Select **Open** beside a listed file to inspect its line diff. Folder
synchronization copies files from the chosen side to the other without
deleting destination-only files. Folder scans and file/text diff computation
run in the background so large comparisons do not block the interface. Folder
comparison skips content reads when file sizes differ and compares same-sized
files in bounded chunks. The diff pane uses GPUI's virtual list and creates
native editing inputs only for visible or focused rows; immutable row and
projection snapshots are shared across the editor and comparison sessions.

For text comparisons, use the whitespace, case, and comment toggles to control
line matching. Search filters visible diff rows, and the star controls let you
bookmark and navigate between rows. Both sides are directly editable in the
aligned diff. Enter splits a line; Backspace/Delete at line boundaries joins
adjacent lines. Hunk and whole-file directional copies are staged in memory.
The compact editor toolbar provides **Undo**, **Redo**, **Save Left/Right**,
**Discard**, and font-size controls so editing stays in the comparison surface.
Editor font size can also be changed or reset from the **View** menu.

Use **View > Theme** to switch between dark and light themes; the choice is
saved for the next launch. Saving a side checks that its file has not changed on
disk since comparison. **Discard** restores the loaded contents without changing
either file. Unsaved file edits are marked in the session tab.

The optional base-file field enables a three-way merge. Non-overlapping edits
are combined and conflicts are marked in the generated result, which appears in
the right side of the same inline diff editor. **Merge inputs** reveals the
Base, Left, and Right source previews. Edit the result inline, then **Save
result** writes it to the right-side file; **Discard** restores the loaded
files. Folder **Sync** remains an explicit immediate disk operation and does
not delete destination-only files. Binary diff viewers, remote file systems,
and deletion-based mirroring are not supported.

## Validate

```sh
cargo test
cargo check
```