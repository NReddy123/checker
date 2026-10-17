mod diff;

use diff::{compare_text, DiffKind, DiffLine};
use gpui_kit::component::{
    input::{Input, InputState},
    *,
};
use gpui_kit::*;
use std::fs;

struct Checker {
    left_path: Entity<InputState>,
    right_path: Entity<InputState>,
    rows: Vec<DiffLine>,
    status: String,
    compared: bool,
}

impl Checker {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            left_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Enter the first file path")
            }),
            right_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Enter the second file path")
            }),
            rows: Vec::new(),
            status: "Choose two text files to see what changed.".into(),
            compared: false,
        }
    }

    fn compare(&mut self, cx: &mut Context<Self>) {
        let left_path = self.left_path.read(cx).value().to_string();
        let right_path = self.right_path.read(cx).value().to_string();

        if left_path.trim().is_empty() || right_path.trim().is_empty() {
            self.status = "Enter a path for both files before comparing.".into();
            self.rows.clear();
            self.compared = false;
            cx.notify();
            return;
        }

        let left = match fs::read_to_string(&left_path) {
            Ok(contents) => contents,
            Err(error) => {
                self.set_error(format!("Could not read {}: {error}", left_path));
                cx.notify();
                return;
            }
        };
        let right = match fs::read_to_string(&right_path) {
            Ok(contents) => contents,
            Err(error) => {
                self.set_error(format!("Could not read {}: {error}", right_path));
                cx.notify();
                return;
            }
        };

        self.rows = compare_text(&left, &right);
        let added = self
            .rows
            .iter()
            .filter(|line| line.kind == DiffKind::Added)
            .count();
        let removed = self
            .rows
            .iter()
            .filter(|line| line.kind == DiffKind::Removed)
            .count();
        self.status = if added == 0 && removed == 0 {
            "The files are identical.".into()
        } else {
            format!("{added} additions  ·  {removed} removals")
        };
        self.compared = true;
        cx.notify();
    }

    fn set_error(&mut self, message: String) {
        self.rows.clear();
        self.status = message;
        self.compared = false;
    }
}

impl Render for Checker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let added = self
            .rows
            .iter()
            .filter(|line| line.kind == DiffKind::Added)
            .count();
        let removed = self
            .rows
            .iter()
            .filter(|line| line.kind == DiffKind::Removed)
            .count();

        div()
            .v_flex()
            .size_full()
            .bg(rgb(0x101318))
            .text_color(rgb(0xe7eaf0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px_8()
                    .py_5()
                    .border_b_1()
                    .border_color(rgb(0x272c34))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Checker"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x929baa))
                                    .child("A calm space to compare files"),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(rgb(0x1c222a))
                            .text_size(px(11.))
                            .text_color(rgb(0x9ea8b7))
                            .child("TEXT DIFF"),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_4()
                    .p_6()
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .child(
                                file_picker_card(
                                    "01",
                                    "Original",
                                    &self.left_path,
                                    rgb(0x73a7ff),
                                ),
                            )
                            .child(
                                file_picker_card(
                                    "02",
                                    "Modified",
                                    &self.right_path,
                                    rgb(0x56d6ad),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(if self.compared
                                        && (added > 0 || removed > 0)
                                    {
                                        rgb(0xd1a96b)
                                    } else {
                                        rgb(0x9ea8b7)
                                    })
                                    .child(self.status.clone()),
                            )
                            .child(
                                Button::new("compare")
                                    .primary()
                                    .label("Compare files")
                                    .on_click(cx.listener(|this, _, _, cx| this.compare(cx))),
                            ),
                    )
                    .child(self.render_diff()),
            )
    }
}

fn file_picker_card(
    number: &str,
    title: &str,
    input_state: &Entity<InputState>,
    accent: Rgba,
) -> impl IntoElement {
    div()
        .v_flex()
        .gap_2()
        .flex_1()
        .p_4()
        .rounded_lg()
        .bg(rgb(0x171b21))
        .border_1()
        .border_color(rgb(0x2b313a))
        .child(
            div()
                .h_flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(accent)
                        .child(number.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.to_owned()),
                ),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(0x929baa))
                .child("File path"),
        )
        .child(Input::new(input_state))
}

impl Checker {
    fn render_diff(&self) -> impl IntoElement {
        let mut rows = div().v_flex().min_h(px(200.)).gap(px(1.));

        if self.rows.is_empty() {
            let message = if self.compared {
                "No line changes to show."
            } else {
                "Your side-by-side comparison will appear here."
            };
            rows = rows.child(
                div()
                    .h_flex()
                    .min_h(px(200.))
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(rgb(0x171b21))
                    .text_size(px(13.))
                    .text_color(rgb(0x778190))
                    .child(message),
            );
            return div().child(rows);
        }

        for line in &self.rows {
            let (background, foreground, marker) = match line.kind {
                DiffKind::Equal => (0x171b21, 0xc8ced8, " "),
                DiffKind::Added => (0x183027, 0xa9ebcb, "+"),
                DiffKind::Removed => (0x382326, 0xf0b1b1, "−"),
            };
            let left_number = line
                .left_number
                .map(|number| number.to_string())
                .unwrap_or_default();
            let right_number = line
                .right_number
                .map(|number| number.to_string())
                .unwrap_or_default();
            let left_text = if line.left_number.is_some() {
                line.text.clone()
            } else {
                String::new()
            };
            let right_text = if line.right_number.is_some() {
                line.text.clone()
            } else {
                String::new()
            };

            rows = rows.child(
                div()
                    .h_flex()
                    .gap(px(1.))
                    .child(diff_cell(
                        &left_number,
                        &left_text,
                        marker,
                        background,
                        foreground,
                    ))
                    .child(diff_cell(
                        &right_number,
                        &right_text,
                        marker,
                        background,
                        foreground,
                    )),
            );
        }

        div()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .h_flex()
                    .px_3()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(0x929baa))
                    .child(div().flex_1().child("ORIGINAL"))
                    .child(div().flex_1().child("MODIFIED")),
            )
            .child(rows)
    }
}

fn diff_cell(
    number: &str,
    text: &str,
    marker: &str,
    background: u32,
    foreground: u32,
) -> impl IntoElement {
    div()
        .h_flex()
        .flex_1()
        .min_w(px(0.))
        .px_2()
        .py_1()
        .gap_2()
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .child(
            div()
                .w(px(34.))
                .text_color(rgb(0x778190))
                .text_size(px(11.))
                .child(number.to_owned()),
        )
        .child(
            div()
                .w(px(12.))
                .text_size(px(12.))
                .child(marker.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .child(if text.is_empty() {
                    " ".to_owned()
                } else {
                    text.to_owned()
                }),
        )
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1080.), px(760.)), cx)),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Checker::new(window, cx))
            })
            .expect("failed to open Checker window");
        });
}
