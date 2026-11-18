use nsh_core::prelude::*;

#[component]
fn Smoke() -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .child(text("NSH 原生窗口已启动").text_size(px(28.0)))
}
