use std::{net::IpAddr, rc::Rc};

use futures_util::StreamExt;
use nsh_core::{element::input, prelude::*};
use serde::Deserialize;

use super::{Api, State, get};

const LOG_LIMIT: usize = 16 * 1024;

#[derive(Deserialize)]
struct PreviewInfo {
    preview_proxy_port: Option<u16>,
}

pub(super) fn panel(api: Rc<Api>, state: State) -> impl IntoElement {
    let process = computed(move || {
        state
            .processes
            .with(|processes| processes.as_ref()?.latest_dev.clone())
    });
    let proxy = signal(None::<u16>);
    let detected = signal(None::<u16>);
    let manual = signal(String::new());
    let error = signal(None::<String>);
    let api_port = api
        .websocket_url("/api/info")
        .split_once("://")
        .and_then(|(_, suffix)| suffix.split('/').next())
        .and_then(authority_port);
    #[cfg(target_arch = "wasm32")]
    let page_port = web_sys::window().and_then(|window| {
        let location = window.location();
        location
            .port()
            .ok()
            .and_then(|port| port.parse().ok())
            .or_else(|| match location.protocol().ok()?.as_str() {
                "http:" => Some(80),
                "https:" => Some(443),
                _ => None,
            })
    });
    #[cfg(not(target_arch = "wasm32"))]
    let page_port = None;
    let blocked =
        move |port| Some(port) == api_port || Some(port) == page_port || Some(port) == proxy.get();
    effect(move || {
        let current = process.get();
        proxy.set(None);
        detected.set(None);
        manual.set(String::new());
        error.set(None);
        let Some(id) = current else { return };
        let api = api.clone();
        let task = spawn(async move {
            let result = async {
                let info: PreviewInfo = get(&api, "/api/info").await?;
                let port = info
                    .preview_proxy_port
                    .ok_or("本地服务未启用独立预览端口。")?;
                proxy.set(Some(port));
                watch_port(&api, &id, [api_port, page_port, Some(port)], detected).await
            }
            .await;
            if let Err(message) = result {
                error.set(Some(message));
            }
        });
        cleanup(move || task.cancel());
    });
    let selected = computed(move || {
        let chosen = manual.with(|value| {
            let value = value.trim();
            if value.is_empty() {
                detected.get()
            } else {
                value.parse::<u16>().ok()
            }
        });
        chosen.filter(|port| *port != 0 && !blocked(*port))
    });
    when(
        move || process.with(Option::is_some),
        move || {
            view().app_class(Some("gap-2 border border-border rounded-card p-3".into()))
            .child(text("开发预览").app_class(Some("text-sm font-semibold".into())))
            .child(view().app_class(Some("flex-row flex-wrap items-center gap-2".into()))
                .child(input().app_class(Some("flex-1 min-w-24 border border-input rounded-md px-2 min-h-9 text-sm".into()))
                    .label("开发服务器端口")
                    .placeholder(move || detected.get().map(|port| port.to_string()).unwrap_or_else(|| "输入开发服务器端口".into()))
                    .value(manual).on_change(move |value: &str| manual.set(value.to_owned())))
                .child(super::ui::action("在浏览器预览", false)
                    .disabled(move || state.busy.get() || selected.with(Option::is_none) || proxy.with(Option::is_none))
                    .on_press(move || {
                        let (Some(port), Some(proxy)) = (selected.get(), proxy.get()) else { return };
                        let opened = open_url(proxy_url(port, proxy));
                        spawn(async move {
                            if !opened.await { error.set(Some("浏览器未打开预览。可复制链接手动打开。".into())); }
                        });
                    })))
            .child(text(move || match (selected.get(), proxy.get()) {
                (Some(dev), Some(proxy)) => proxy_url(dev, proxy),
                _ if !manual.with(String::is_empty) => "请输入有效的开发服务器端口。".into(),
                _ => "正在从开发服务器日志读取端口。也可以手动输入。".into(),
            }).app_class(Some("text-xs font-mono text-muted-foreground".into())).selectable(true))
            .child(when(move || error.with(Option::is_some), move || {
                text(move || error.get().unwrap_or_default()).app_class(Some("text-xs text-destructive".into()))
            }))
        },
    )
}

fn proxy_url(dev_port: u16, proxy_port: u16) -> String {
    format!("http://{dev_port}.localhost:{proxy_port}/")
}

async fn watch_port(
    api: &Api,
    id: &str,
    blocked: [Option<u16>; 3],
    output: Signal<Option<u16>>,
) -> Result<(), String> {
    let (mut frames, _connection) =
        api.stream(&format!("/api/execution-processes/{id}/raw-logs/ws"))?;
    let mut log = String::new();
    while let Some(frame) = frames.next().await {
        let value: serde_json::Value =
            serde_json::from_str(&frame?).map_err(|error| error.to_string())?;
        if value["finished"] == true {
            return Ok(());
        }
        let Some(patches) = value["JsonPatch"].as_array() else {
            continue;
        };
        for patch in patches {
            let value = &patch["value"];
            if matches!(value["type"].as_str(), Some("STDOUT" | "STDERR"))
                && let Some(content) = value["content"].as_str()
            {
                append_log(&mut log, content);
            }
        }
        if let Some(port) = find_port(&log, &blocked) {
            output.set(Some(port));
        }
    }
    Err("开发服务器日志连接已关闭。".into())
}

fn append_log(log: &mut String, content: &str) {
    let mut chars = content.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.as_str().starts_with('[') {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        } else {
            log.push(ch);
        }
    }
    if log.len() > LOG_LIMIT {
        let start = log.ceil_char_boundary(log.len() - LOG_LIMIT);
        log.drain(..start);
    }
}

fn authority_port(authority: &str) -> Option<u16> {
    let (_, port) = authority.rsplit_once(':')?;
    port.parse().ok()
}

fn find_port(log: &str, blocked: &[Option<u16>]) -> Option<u16> {
    // Scan newest port first, retaining log chunks for split startup banners.
    scan_port(log, blocked)
}

fn scan_port(log: &str, blocked: &[Option<u16>]) -> Option<u16> {
    let host_char = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-');
    log.rmatch_indices(':').find_map(|(colon, _)| {
        let before = &log[..colon];
        let start = if before.ends_with(']') {
            before.rfind('[')?
        } else {
            before.rfind(|ch| !host_char(ch)).map_or(0, |index| {
                index + before[index..].chars().next().unwrap().len_utf8()
            })
        };
        let host = &before[start..];
        if !host.eq_ignore_ascii_case("localhost")
            && host.trim_matches(['[', ']']).parse::<IpAddr>().is_err()
        {
            return None;
        }
        let suffix = &log[colon + 1..];
        // A token may span log chunks with whitespace inserted inside a port.
        let mut port = 0u16;
        let mut digits = 0;
        let mut length = 0;
        let mut remaining = suffix;
        for ch in suffix.chars().take(10) {
            if let Some(digit) = ch.to_digit(10) {
                digits += 1;
                if digits > 5 {
                    return None;
                }
                port = port.checked_mul(10)?.checked_add(digit as u16)?;
                length = suffix.len() - remaining.len() + ch.len_utf8();
            } else if !ch.is_ascii_whitespace() || digits == 0 {
                break;
            }
            remaining = &remaining[ch.len_utf8()..];
        }
        (digits >= 2
            && port != 0
            && !blocked.contains(&Some(port))
            && !suffix[length..].chars().next().is_some_and(host_char))
        .then_some(port)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_detected_dev_port_on_the_isolated_origin() {
        let mut log = String::new();
        append_log(&mut log, "\x1b[32mLocal: http://localhost:40\x1b[0m");
        append_log(&mut log, "00/hello\nNetwork: http://192.168.10.1:4000\n");
        assert_eq!(find_port(&log, &[Some(3000)]), Some(4000));
        assert_eq!(
            find_port("Listening at http://[::]:40\n00/hello", &[]),
            Some(4000)
        );
        assert_eq!(
            find_port(
                "http://4000.localhost:3009/path\nhttp://localhost:3000",
                &[Some(3000)]
            ),
            None
        );
        assert_eq!(
            find_port("https://localhost.example.com:5150\nlocalhost:99999", &[]),
            None
        );
        assert_eq!(proxy_url(4000, 3009), "http://4000.localhost:3009/");
    }
}
