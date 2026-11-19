use futures_channel::mpsc::{UnboundedReceiver, unbounded};

pub(crate) type Frames = UnboundedReceiver<Result<String, String>>;

#[cfg(windows)]
pub(crate) fn connect(url: String) -> Result<(Frames, ()), String> {
    use std::{io::ErrorKind, net::TcpStream, time::Duration};

    use tungstenite::{Error, Message, client};

    let (tx, rx) = unbounded();
    std::thread::Builder::new()
        .name("vibe-websocket".into())
        .spawn(move || {
            let run = || -> Result<(), String> {
                let authority = url
                    .strip_prefix("ws://")
                    .and_then(|rest| rest.split_once('/'))
                    .ok_or("Expected a local ws:// URL")?
                    .0;
                let tcp = TcpStream::connect(authority).map_err(|e| e.to_string())?;
                tcp.set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|e| e.to_string())?;
                let (mut socket, _) = client(url, tcp).map_err(|e| e.to_string())?;
                socket
                    .get_ref()
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .map_err(|e| e.to_string())?;
                while !tx.is_closed() {
                    match socket.read() {
                        Ok(Message::Text(message)) => {
                            if tx.unbounded_send(Ok(message.to_string())).is_err() {
                                break;
                            }
                        }
                        Ok(Message::Close(_)) | Err(Error::ConnectionClosed) => break,
                        Ok(Message::Ping(_)) => socket.flush().map_err(|e| e.to_string())?,
                        Ok(_) => {}
                        Err(Error::Io(e))
                            if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                let _ = socket.close(None);
                Ok(())
            };
            if let Err(error) = run() {
                let _ = tx.unbounded_send(Err(error));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok((rx, ()))
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::{JsCast, closure::Closure};
    use web_sys::{Event, MessageEvent, WebSocket};

    use super::{Frames, unbounded};

    pub(crate) struct Connection {
        socket: WebSocket,
        _message: Closure<dyn FnMut(MessageEvent)>,
        _error: Closure<dyn FnMut(Event)>,
        _close: Closure<dyn FnMut(Event)>,
    }

    impl Drop for Connection {
        fn drop(&mut self) {
            self.socket.set_onmessage(None);
            self.socket.set_onerror(None);
            self.socket.set_onclose(None);
            let _ = self.socket.close();
        }
    }

    pub(crate) fn connect(url: String) -> Result<(Frames, Connection), String> {
        let socket = WebSocket::new(&url).map_err(|e| format!("{e:?}"))?;
        let (tx, rx) = unbounded();
        let message_tx = tx.clone();
        let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = message_tx.unbounded_send(Ok(text));
            }
        });
        let error_tx = tx.clone();
        let error = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = error_tx.unbounded_send(Err("WebSocket connection failed".into()));
        });
        let close = Closure::<dyn FnMut(Event)>::new(move |_| tx.close_channel());
        socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
        socket.set_onerror(Some(error.as_ref().unchecked_ref()));
        socket.set_onclose(Some(close.as_ref().unchecked_ref()));
        Ok((
            rx,
            Connection {
                socket,
                _message: message,
                _error: error,
                _close: close,
            },
        ))
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::connect;
