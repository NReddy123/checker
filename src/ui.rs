use crate::folder::FileMetadata;
use crate::theme::ThemePalette;
use chrono::{DateTime, Local};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::SystemTime;

pub(crate) fn comparison_mode_card(
    title: &str,
    description: &str,
    button: impl IntoElement,
    palette: &ThemePalette,
) -> impl IntoElement {
    div()
        .v_flex()
        .flex_1()
        .min_w(px(0.))
        .gap_3()
        .p_4()
        .rounded_lg()
        .bg(rgb(palette.card))
        .border_1()
        .border_color(rgb(palette.border))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .child(title.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(10.))
                .text_color(rgb(palette.secondary_text))
                .child(description.to_owned()),
        )
        .child(button)
}

pub(crate) fn text_source(
    label: &str,
    state: &Entity<EditorState>,
    font_size: f32,
    palette: &ThemePalette,
) -> impl IntoElement {
    div()
        .v_flex()
        .flex_1()
        .min_w(px(0.))
        .min_h_0()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(9.))
                .text_color(rgb(palette.secondary_text))
                .child(label.to_owned()),
        )
        .child(
            Editor::new(state)
                .flex_1()
                .min_h_0()
                .font_family("Cascadia Code")
                .text_size(px(font_size)),
        )
}

pub(crate) fn folder_side_header(root: &str, palette: &ThemePalette) -> impl IntoElement {
    div()
        .h_flex()
        .flex_1()
        .min_w(px(0.))
        .gap_2()
        .px_2()
        .py_2()
        .text_size(px(9.))
        .text_color(rgb(palette.secondary_text))
        .child(div().flex_1().min_w(px(0.)).child(root.to_owned()))
        .child(div().w(px(64.)).child("SIZE"))
        .child(div().w(px(126.)).child("MODIFIED"))
}

pub(crate) fn folder_side_cell(
    path: Option<&str>,
    metadata: Option<&FileMetadata>,
    palette: &ThemePalette,
) -> impl IntoElement {
    let name = path.unwrap_or("—");
    let size = metadata
        .map(|metadata| format_size(metadata.size))
        .unwrap_or_default();
    let modified = metadata
        .and_then(|metadata| metadata.modified)
        .map(format_modified_time)
        .unwrap_or_default();

    div()
        .h_flex()
        .flex_1()
        .min_w(px(0.))
        .gap_2()
        .px_2()
        .py_2()
        .text_size(px(10.))
        .child(div().flex_1().min_w(px(0.)).child(name.to_owned()))
        .child(
            div()
                .w(px(64.))
                .text_color(rgb(palette.secondary_text))
                .child(size),
        )
        .child(
            div()
                .w(px(126.))
                .text_color(rgb(palette.muted_text))
                .child(modified),
        )
}

pub(crate) fn merge_preview(
    label: &str,
    content: &str,
    font_size: f32,
    palette: &ThemePalette,
) -> impl IntoElement {
    let mut lines = div()
        .v_flex()
        .text_size(px(font_size))
        .text_color(rgb(palette.foreground));
    if content.is_empty() {
        lines = lines.child(
            div()
                .px_2()
                .py_1()
                .text_color(rgb(palette.muted_text))
                .child("No content"),
        );
    } else {
        for (index, line) in content.lines().enumerate() {
            lines = lines.child(
                div()
                    .h_flex()
                    .gap_2()
                    .px_2()
                    .py(px(1.))
                    .child(
                        div()
                            .w(px(30.))
                            .text_color(rgb(palette.muted_text))
                            .child((index + 1).to_string()),
                    )
                    .child(line.to_owned()),
            );
        }
    }

    div()
        .v_flex()
        .flex_1()
        .min_w(px(0.))
        .min_h_0()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.panel))
        .child(
            div()
                .px_2()
                .py_1()
                .bg(rgb(palette.toolbar))
                .text_size(px(10.))
                .text_color(rgb(palette.secondary_text))
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .font_family("Cascadia Code")
                .child(lines),
        )
}

pub(crate) fn path_input(
    title: &str,
    state: &Entity<InputState>,
    palette: &ThemePalette,
) -> impl IntoElement {
    div()
        .h_flex()
        .items_center()
        .flex_1()
        .min_w(px(0.))
        .gap_1()
        .child(
            div()
                .w(px(42.))
                .text_size(px(9.))
                .text_color(rgb(palette.secondary_text))
                .child(title.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(Input::new(state).with_size(Size::Small)),
        )
}

pub(crate) fn source_label(path: &str, selected: Option<&str>, fallback: &str) -> String {
    if let Some(selected) = selected {
        return selected.to_owned();
    }
    PathBuf::from(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if path.trim().is_empty() {
                fallback.to_owned()
            } else {
                path.to_owned()
            }
        })
}

pub(crate) fn labeled_input(
    title: &str,
    state: &Entity<InputState>,
    palette: &ThemePalette,
) -> impl IntoElement {
    div()
        .v_flex()
        .flex_1()
        .min_w(px(0.))
        .gap(px(4.))
        .child(
            div()
                .text_size(px(9.))
                .text_color(rgb(palette.secondary_text))
                .child(title.to_owned()),
        )
        .child(Input::new(state).with_size(Size::Small))
}

pub(crate) fn diff_cell(
    number: &str,
    input_state: &Entity<InputState>,
    focused: bool,
    background: u32,
    foreground: u32,
    font_size: f32,
    palette: &ThemePalette,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let cell = div()
        .on_key_down(on_key_down)
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .h_flex()
        .flex_1()
        .min_w(px(0.))
        .px_2()
        .h(px((font_size + 8.).max(18.)))
        .gap_2()
        .bg(rgb(background))
        .when(focused, |this| {
            this.border_1().border_color(rgb(palette.accent))
        })
        .font_family("Cascadia Code")
        .child(
            div()
                .w(px(38.))
                .text_color(rgb(palette.muted_text))
                .text_size(px((font_size - 1.).max(8.)))
                .child(number.to_owned()),
        );
    // Render an actual GPUI text input in the aligned diff cell, not a painted
    // text span; native selection, clipboard, IME, and caret behavior stay active.
    cell.child(
        Input::new(input_state)
            .disabled(false)
            .readonly(false)
            .appearance(true)
            .bordered(true)
            .focus_bordered(true)
            .with_size(Size::XSmall)
            .h_full()
            .text_color(rgb(foreground))
            .text_size(px(font_size))
            .font_family("Cascadia Code"),
    )
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let units = ["KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024. && unit < units.len() - 1 {
        size /= 1024.;
        unit += 1;
    }
    format!("{size:.1} {}", units[unit])
}

fn format_modified_time(time: SystemTime) -> String {
    let time: DateTime<Local> = time.into();
    time.format("%Y-%m-%d %H:%M").to_string()
}
