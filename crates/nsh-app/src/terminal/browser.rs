use std::{borrow::Cow, cell::RefCell, rc::Rc};

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;
use nsh_core::prelude::{ElementRef, cleanup, spawn};
use serde::Deserialize;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Event, MessageEvent, WebSocket};

struct Connection {
    socket: WebSocket,
    _open: Closure<dyn FnMut(Event)>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(Event)>,
    _close: Closure<dyn FnMut(Event)>,
}

impl Connection {
    fn open(
        url: &str,
        tx: UnboundedSender<Cow<'static, str>>,
    ) -> Result<Self, wasm_bindgen::JsValue> {
        let socket = WebSocket::new(url)?;
        let opened = tx.clone();
        let open = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = opened.unbounded_send("open".into());
        });
        let output = tx.clone();
        let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = output.unbounded_send(text.into());
            }
        });
        let failed = tx.clone();
        let error = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = failed.unbounded_send("failed".into());
        });
        let close = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = tx.unbounded_send("disconnected".into());
        });
        socket.set_onopen(Some(open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
        socket.set_onerror(Some(error.as_ref().unchecked_ref()));
        socket.set_onclose(Some(close.as_ref().unchecked_ref()));
        Ok(Self {
            socket,
            _open: open,
            _message: message,
            _error: error,
            _close: close,
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Command {
    Connect { cols: u16, rows: u16 },
    Input,
    Resize,
}

/// The sandboxed `srcdoc` has Origin `null`, which the server rejects.
/// Open the socket here and accept commands only from this WebView's NSH
/// MessagePort. The workspace and origin come exclusively from the app.
pub(super) fn bridge(origin: &str, workspace: &str, view: ElementRef) -> impl Fn(&str) + use<> {
    let endpoint = format!("{origin}/api/terminal/ws?workspace_id={workspace}");
    let connection = Rc::new(RefCell::new(None::<Connection>));
    let on_drop = Rc::clone(&connection);
    // Browser WebSocket callbacks run outside NSH's reactive runtime. Its
    // executor must deliver the commands to the mounted WebView reference.
    let (tx, mut rx) = unbounded::<Cow<'static, str>>();
    let frames = spawn(async move {
        while let Some(frame) = rx.next().await {
            view.post_message(frame.into_owned());
        }
    });
    cleanup(move || {
        on_drop.borrow_mut().take();
        frames.cancel();
    });

    move |message| {
        let Ok(command) = serde_json::from_str::<Command>(message) else {
            return;
        };
        let mut connection = connection.borrow_mut();
        match command {
            Command::Connect { cols, rows } if connection.is_none() => {
                let url = format!("{endpoint}&cols={cols}&rows={rows}");
                match Connection::open(&url, tx.clone()) {
                    Ok(socket) => *connection = Some(socket),
                    Err(_) => view.post_message("failed"),
                }
            }
            Command::Input | Command::Resize => {
                if let Some(socket) = &*connection
                    && socket.socket.ready_state() == WebSocket::OPEN
                    && socket.socket.send_with_str(message).is_err()
                {
                    view.post_message("failed");
                }
            }
            Command::Connect { .. } => {}
        }
    }
}
