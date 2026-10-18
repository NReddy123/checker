# Next-Gen Diff & Merge Architecture (Rust + GPUI)

Building a true, no-compromise **Beyond Compare** competitor in **Rust + GPUI** means targeting two structural bottlenecks that legacy diff tools (built on Delphi, Qt, or Electron) fail to solve: **blocking I/O during massive directory comparisons** and **$O(N^2)$ layout stalls on multi-megabyte or syntax-aware file diffs**. 

By pairing Rust’s zero-cost concurrency with GPUI’s hybrid immediate/retained-mode GPU rendering, you can deliver an application that launches in under **20ms**, maintains a locked **120fps** scroll across 10-million-line log files, and performs **AST-aware structural diffs** in real time.

---

## 1. Macro Architecture & Crate Topology

To keep compile times manageable and enforce strict separation between your diff engines and the GPU presentation layer, structure your workspace as a decoupled multi-crate monorepo. 

> **Crucial Ecosystem Note on GPUI:** Because upstream GPUI inside the Zed monorepo evolves rapidly for Zed's internal needs, pin your dependency to a known tag or evaluate community layers like `gpui-component` for baseline widgets, while writing your core diff viewports directly against GPUI’s low-level `Element` trait.

* **`core-vfs` (Virtual File System):** Unified async trait layer over local NVMe/SSD (`io_uring` on Linux, `IOCP` on Windows, `kqueue` on macOS), SFTP/SSH, S3, Git object databases (`gix`), and archive formats (`.zip`, `.tar.zst`) streamed in-memory without temp-file extraction.
* **`diff-engine`:** Pure computation crate. Houses line-based (`imara-diff`), byte-level (rolling hash/Rabin fingerprint for hex diffs), and structural (`tree-sitter`) diff algorithms. Zero UI dependencies.
* **`text-buffer`:** A custom Rope + `SumTree` implementation (heavily inspired by Zed’s `sum_tree` crate) designed specifically for **multi-buffer coordinate mapping** (Left $\leftrightarrow$ Base $\leftrightarrow$ Right $\leftrightarrow$ Output).
* **`app-ui`:** The GPUI application layer. Manages `Model<T>` state entities, `View<T>` declarative chrome, custom `Element` implementations for the diff canvas, and custom WGSL/Metal shaders for the Bezier alignment gutter.

---

## 2. The Data Foundation: Multi-Buffer `SumTree`

Standard text editors manage one coordinate space. A 3-way merge tool manages **four simultaneously synchronized coordinate spaces** (Left, Base, Right, Merged Output) where foldable unchanged regions, inserted lines, and phantom alignment padding ("ghost lines") must resolve in $O(\log N)$ time.

### Why a Rope + `SumTree` is Non-Negotiable
If a user diffs two 500MB files and clicks on scrollbar pixel $y = 420$, you cannot iterate over an array of lines to find which hunk is visible. 

Borrowing Zed's architectural secret weapon, implement a B+ Tree (`SumTree`) where every internal node summarizes metadata about its children:

```rust
#[derive(Clone, Default, Debug)]
pub struct DiffSummary {
    pub left_lines: u32,
    pub right_lines: u32,
    pub display_rows: u32,      // Accounts for folded regions & phantom alignment rows
    pub conflict_count: u32,
    pub byte_len_left: usize,
    pub byte_len_right: usize,
}
```

With this summary maintained incrementally on every edit:
* **Scroll-to-Row translation:** Converting a pixel offset to a synchronized `(LeftLine, RightLine)` pair takes **$O(\log N)$** tree traversal.
* **Next/Prev Difference Jump:** Finding the next diff hunk or unresolved merge conflict is an instant **$O(\log N)$** seek using `conflict_count` summaries, even in million-line files.
* **Zero-Copy File Loading:** For files over 50MB, back the leaves of your Rope with `memmap2` read-only memory maps rather than heap allocations, paging chunks into the GPU text atlas only as they enter the viewport.

---

## 3. The Diff Engine: Progressive 3-Tier Pipeline

Beyond Compare’s classic strength is its custom rules-based grammar diffing, which feels dated and requires manual setup. A **10x experience** uses a progressive, non-blocking 3-stage diff pipeline that refines accuracy within milliseconds:

| Stage | Target Latency | Algorithm & Rust Crate | Purpose in UI |
| :--- | :--- | :--- | :--- |
| **Tier 1: Coarse Line Diff** | $< 5\text{ms}$ | **Histogram / Myers** via `imara-diff` | Instant viewport layout, scrollbar minimap markers, and initial alignment blocks. |
| **Tier 2: Word / Grapheme Diff** | $< 15\text{ms}$ (Viewport First) | **SIMD Token Myers** + `unicode-segmentation` | Intra-line character highlighting inside modified hunks. Computed lazily for visible viewport first, then backgrounded. |
| **Tier 3: Structural AST Diff** | $30\text{ms}\text{--}150\text{ms}$ (Background) | **`tree-sitter`** + GumTree / Difftastic heuristics | Understands code moves, refactored arguments, and formatting-only changes (e.g., ignores Prettier/Rustfmt line wraps). |

### The "Killer Feature": Semantic Noise Suppression
In standard diff tools, formatting a block of Rust or TypeScript turns the entire screen red and green. Because you have `tree-sitter` running on GPUI's background executor, you can provide a **"Hide Formatting Changes" (`Cmd+Shift+F`)** toggle that matches AST nodes across panes and collapses pure whitespace/wrapping diffs into subtle grey indicators.

---

## 4. GPUI Rendering Architecture: Achieving Locked 120fps

GPUI gives you three levels of abstraction:
1. **`Entity` / `Model<T>`:** Application state owned by the GPUI context.
2. **`View` (`Render` trait):** Declarative, Tailwind-style UI for toolbars, tabs, modals, and status bars.
3. **`Element` trait:** Imperative layout and direct GPU quad/path painting for high-density surfaces.

### Rule #1: Never Render Diff Lines as Declarative `div()` Nodes
If you render 150 visible lines across 3 panes with inline word highlights using nested `div()` elements, Taffy (GPUI’s Flexbox layout engine) will compute thousands of layout nodes per frame. Instead, build a custom `DiffPaneElement` implementing GPUI’s `Element` trait:

* **`request_layout`:** Requests a single fixed-size bounding box from Taffy filling the pane.
* **`prepaint`:** Queries the `SumTree` for only the rows intersecting the current scroll viewport (`start_row..end_row`), shapes visible text runs using GPUI’s `TextSystem` (which caches shaped glyphs in a GPU texture atlas), and computes hitboxes (`Hitbox`) for inline "Accept Left / Accept Right" gutter buttons.
* **`paint`:** Emits batched background quads (`PaintQuad`) for added/deleted/modified line bands, intra-line word highlight quads with rounded corners, and shaped glyph runs in a single pass.

### Rule #2: The Central Bezier Gutter Connector
The visual hallmark of a world-class diff tool is the central connector column that draws smooth S-curves linking a 3-line block on the Left to a 25-line block on the Right—even when the two panes are scrolled independently or aligned dynamically.
* Use GPUI’s `PathBuilder` (powered by `lyon` under the hood) inside a dedicated `GutterConnectorElement`.
* Fill the cubic Bezier ribbon with a semi-transparent alpha gradient (`hsla`) and stroke the top and bottom edges with a `1.5px` high-contrast border so overlapping moves remain visually distinct.

---

## 5. Folder & Archive Comparison Engine

Beyond Compare is legendary for directory comparison, yet it still bottlenecks on deeply nested `node_modules` or `.git` trees. Here is how to make directory compare **10x faster**:

1. **Lock-Free Parallel Traversal:** Use `jwalk` or a custom work-stealing `rayon` pool paired with `ignore` (for optional `.gitignore` filtering). Emit batched discovery chunks over a `flume` channel every **8ms** (half a 60Hz frame) so the GPUI tree view populates live without jitter.
2. **3-Stage Verification Pipeline:**
   * **Phase 1 (Metadata):** Instant comparison by file size and `mtime` tolerance.
   * **Phase 2 (Fast Hash):** If sizes match and the user requests content verification, read the **first 4KB + last 4KB** of both files and hash with **`blake3`**. If they differ, mark as *Modified* immediately without reading the middle 2GB.
   * **Phase 3 (Full SIMD Hash):** Stream full contents through `blake3` (which saturates NVMe bandwidth at $>6\text{ GB/s}$ using AVX-512/NEON) on background threads.
3. **Inline Expandable File Diffs (The UX Breakthrough):** In Beyond Compare, double-clicking a file in Folder Compare opens a disconnected new tab. In your GPUI app, clicking the chevron next to a modified file in the Folder Tree should **accordion-expand the inline diff directly inside the folder tree**, or open a synchronized preview split at the bottom with zero tab switching.

---

## 6. End-to-End Execution Pipeline (8.33ms Frame Budget)

```
+-----------------------------------------------------------------------------+
| 1. VFS & Fast Ingestion (Async I/O & Compute Pool)                          |
|    - core-vfs streams via memmap2 (>50MB files) or blake3 SIMD (6+ GB/s)    |
+-----------------------------------------------------------------------------+
                                      |
                                      v (flume channel chunks)
+-----------------------------------------------------------------------------+
| 2. 3-Tier Progressive Diff (Background Executor)                            |
|    - Tier 1: imara-diff Histogram line diff (< 5ms)                         |
|    - Tier 2: Viewport-first SIMD token/grapheme diff (< 15ms)               |
|    - Tier 3: tree-sitter structural AST diff (30ms - 150ms)                 |
+-----------------------------------------------------------------------------+
                                      |
                                      v (Arc<SumTree> immutable snapshot)
+-----------------------------------------------------------------------------+
| 3. Multi-Buffer SumTree Sync (Shared Entity State)                          |
|    - B+ Tree indexing Left, Base, Right, and Output coordinate spaces       |
|    - O(log N) line/pixel translation and conflict seeking                   |
+-----------------------------------------------------------------------------+
                                      |
                                      v (GPUI Model<T> observation)
+-----------------------------------------------------------------------------+
| 4. GPUI Element Layout & Prepaint (Main Thread)                             |
|    - DiffPaneElement queries visible viewport rows (start_row..end_row)     |
|    - Shapes visible glyphs into GPU texture atlas & registers Hitboxes      |
+-----------------------------------------------------------------------------+
                                      |
                                      v (GPU Command Buffer)
+-----------------------------------------------------------------------------+
| 5. GPU Batch Paint at 120fps (< 8.33ms Total Frame Budget)                  |
|    - Emits batched PaintQuad instances for line/word highlights             |
|    - Renders lyon cubic Bezier connectors across central gutter             |
+-----------------------------------------------------------------------------+
```

---

## 7. What Makes It a "10x Experience" (Look, Feel & Workflows)

To make users abandon Beyond Compare, Kaleidoscope, and Araxis Merge immediately, build these five capabilities into the core UX:

### 1. Sub-Line "Ghost Alignment" Without Jarring Jumps
When comparing a 1-line expression to a 12-line refactored block, traditional tools insert 11 blank "phantom lines" on the shorter side, which ruins the natural reading flow of the file.
* **Two-Mode Scroll Coupling:** Offer **Strict Lock** (traditional phantom lines with diagonal subtle stripe shaders) *and* **Elastic Magnetic Scroll** (neither file has blank gaps; instead, the central Bezier gutter stretches smoothly and subtly decelerates the shorter side's scroll velocity so matching hunks pass the vertical center of the screen at the exact same moment).

### 2. First-Class Structured Table & Parquet/SQLite Diffing
Beyond Compare’s "Table Compare" only handles basic CSVs well.
* Integrate **`polars`** or **`arrow`** in Rust so users can drag-and-drop two `.parquet`, `.csv`, `.json`, or `.sqlite` files.
* Automatically infer primary keys, sort both datasets in memory in milliseconds, and render a virtualized GPU grid showing added rows, deleted rows, and schema drift.

### 3. Interactive 3-Way Conflict Resolution Sandbox
Resolving Git merge conflicts in VS Code or Beyond Compare feels stressful because editing the output pane feels permanent and brittle.
* Build a **Live Conflict AST Sandbox**: as the user clicks hunks or types directly into the bottom/center output buffer, run `tree-sitter` incrementally ($< 1\text{ms}$) and highlight syntax or bracket-matching errors *before* they save the merged file.
* Include a **1-Click "Take Both Smartly"** action that uses AST awareness to merge two independent imports, struct fields, or enum variants without duplicating surrounding braces.

### 4. Visual Image & PDF Diffing on the GPU
Because your entire app already renders through Metal/Vulkan/DirectX via GPUI, image diffing is practically free to make extraordinary:
* **Onion Skin Slider & Wipe:** Hardware-accelerated split curtain at 120fps.
* **Perceptual Pixel Diff Shader:** Write a custom fragment shader that highlights pixel deltas using $\Delta E$ (**CIEDE2000**) perceptual color distance rather than naive RGB subtraction, with a live threshold slider to filter out JPEG compression artifacts.

### 5. Keyboard-First Command Palette & Vim/Zed Ergonomics
Beyond Compare's UI is crammed with 1990s toolbar icons and deeply nested modal dialogs for session settings.
* Adopt Zed's **Command Palette (`Cmd+Shift+P`)** and **keymap architecture**. Every action—toggling whitespace, changing character encoding, swapping left/right panes, copying a hunk to the right (`dp` / `do` in Vim mode), or filtering folder trees by glob—should be instantaneous from the keyboard.

---

## 8. Recommended Implementation Roadmap (Zero to Alpha)

Don't start with folder comparison or 3-way merges—nail the **2-way text diff viewport** first, because it proves out your `SumTree` and custom GPUI `Element` pipeline:

1. **Milestone 1: The Synchronized Dual Viewport (Weeks 1–3)**
   * Build a minimal GPUI window hosting two side-by-side custom `Element` panes and a central `GutterConnectorElement`.
   * Integrate `ropey` (or extract Zed's `sum_tree` crate) + `imara-diff`. Achieve 120fps synchronized scrolling with smooth Bezier connectors across a 100,000-line file diff.
2. **Milestone 2: Intra-Line & `tree-sitter` Semantic Layer (Weeks 4–6)**
   * Add viewport-prioritized word diffing and `tree-sitter` syntax highlighting.
   * Implement gutter action hitboxes (1-click hunk push/pull between Left and Right with full undo/redo stack).
3. **Milestone 3: 3-Way Merge & Conflict Engine (Weeks 7–9)**
   * Add the Base ancestor buffer and the bottom editable Output buffer.
   * Register the tool as a drop-in `git mergetool` and `git difftool` CLI binary so developers can invoke it directly from the terminal (`bcrust fileA fileB`).
4. **Milestone 4: Parallel VFS & Folder Compare (Weeks 10–14)**
   * Build the `jwalk` + `blake3` directory scanner feeding a virtualized GPUI tree view with inline expandable file previews.