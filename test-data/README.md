# Comparison test data

For **File Compare**, select `sample-original.txt` and `sample-modified.txt`
from the repository root.

For **Folder Compare**, select `test-data/folder-left` and
`test-data/folder-right`. The results should include:

- `shared/identical.txt` and `nested/deep/identical.md` as identical
- `shared/same-size-different.txt` as different despite matching file sizes
- `shared/different-size.txt` as different with different file sizes
- `left-only.txt` as left-only and `right-only.txt` as right-only
- `ignored.log` omitted because both roots' `.gitignore` files exclude `*.log`

The `nested/deep` files also exercise recursive folder scanning. These are
fixtures only; folder synchronization will copy files if you choose **Sync**.
