use std::cell::Cell;

use nsh_core::prelude::*;

#[cfg(target_arch = "wasm32")]
#[path = "terminal/browser.rs"]
mod browser;
#[cfg(windows)]
#[path = "terminal/windows.rs"]
mod windows;

/// Mount in the workspace detail column. `backend_origin` is the trusted HTTP(S)
/// API origin. The caller can share `open` with an action or keyboard shortcut.
/// `obscured` must include any modal or command palette covering this pane.
/// Hiding for an overlay preserves the running shell; closing or changing
/// workspace drops the old WebView and its session.
pub(crate) fn terminal_panel(
    backend_origin: String,
    workspace_id: impl Fn() -> Option<String> + 'static,
    open: Signal<bool>,
    obscured: impl Fn() -> bool + 'static,
) -> impl IntoElement {
    let backend_origin = backend_origin.trim_end_matches('/').to_owned();
    let workspace = computed(workspace_id);
    let obscured = computed(obscured);
    let expanded = computed(move || if open.get() { workspace.get() } else { None });
    let toggle = element_ref();

    view()
        .w_full()
        .min_w(px(0.0))
        .child(when(
            move || workspace.with(Option::is_some),
            move || {
                view()
                    .flex_row()
                    .px_3()
                    .h(px(34.0))
                    .gap_3()
                    .items_center()
                    .bg(rgb(0x193e36))
                    .child(
                        text("Workspace terminal")
                            .text_size(px(13.0))
                            .text_color(rgb(0xe3f2e9)),
                    )
                    .child(view().flex_1())
                    .child(
                        view()
                            .element_ref(&toggle)
                            .role(AccessibilityRole::Button)
                            .h(px(28.0))
                            .px_2()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(rgb(0x2a5146))
                            .on_press(move || open.update(|is_open| *is_open = !*is_open))
                            .child(
                                text(move || {
                                    if expanded.with(Option::is_some) {
                                        "Close terminal"
                                    } else {
                                        "Open terminal"
                                    }
                                })
                                .text_size(px(12.0))
                                .text_color(rgb(0xe3f2e9)),
                            ),
                    )
            },
        ))
        .child(dynamic(move || {
            expanded
                .get()
                .map(|id| shell_view(&backend_origin, id, open, obscured, toggle))
        }))
}

fn shell_view(
    origin: &str,
    workspace_id: String,
    open: Signal<bool>,
    obscured: Memo<bool>,
    toggle: ElementRef,
) -> impl IntoElement + use<> {
    let web = element_ref();
    let ready = signal(false);
    let error = signal(None::<String>);
    let first_show = Cell::new(true);
    effect(move || {
        if ready.get() {
            web.post_message(if obscured.get() {
                "obscure"
            } else if first_show.replace(false) {
                "focus"
            } else {
                "reveal"
            });
        }
    });

    let assets = format!("{origin}/api/terminal/");
    #[cfg(windows)]
    let content = web_view(format!("{assets}view?workspace_id={workspace_id}"));
    #[cfg(target_arch = "wasm32")]
    let content = web_view_html(
        include_str!("terminal/assets/index.html").to_owned(),
        Some(&assets),
    );
    #[cfg(target_arch = "wasm32")]
    let bridge = browser::bridge(origin, &workspace_id, web);
    #[cfg(windows)]
    let bridge = windows::bridge(origin, &workspace_id, web);

    view()
        .key(workspace_id)
        .h(px(256.0))
        .min_w(px(0.0))
        .overflow_hidden()
        .bg(rgb(0x102d2a))
        .style_when(move || obscured.get(), |style| style.invisible())
        .child(
            content
                .element_ref(&web)
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .scrollable(false)
                .on_error(move |message: &str| error.set(Some(message.to_owned())))
                .on_message(move |message: &str| match message {
                    "ready" => ready.set(true),
                    "exited" => {
                        open.set(false);
                        if !obscured.get() {
                            toggle.focus();
                        }
                    }
                    _ => bridge(message),
                }),
        )
        .child(dynamic(move || {
            error.get().map(|message| {
                text(format!("Could not load terminal: {message}"))
                    .px_3()
                    .text_size(px(12.0))
                    .text_color(rgb(0xffcfb3))
            })
        }))
}
