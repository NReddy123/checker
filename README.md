# Checker

Checker is a native Rust desktop app for comparing two UTF-8 text files. It uses
[GPUI Kit](https://github.com/longbridge/gpui-kit) for its polished desktop UI
and shows line-numbered additions and removals side by side.

## Run

```sh
cargo run
```

Enter the paths to two local text files and select **Compare files**. The first
version focuses on text files; binary and directory comparisons are not
supported yet.

## Validate

```sh
cargo test
cargo check
```