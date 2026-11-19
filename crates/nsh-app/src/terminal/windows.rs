use std::{
    cell::RefCell,
    io::ErrorKind,
    net::TcpStream,
    rc::Rc,
    sync::mpsc::{self, Sender, TryRecvError},
    time::Duration,
};

use futures_channel::mpsc::unbounded;
use futures_util::StreamExt;
use nsh_core::prelude::{ElementRef, cleanup, spawn};
use serde::Deserialize;
use tungstenite::{Error, Message, client};

#[derive(Deserialize)]
struct InitialSize {
    cols: u16,
    rows: u16,
}

pub(super) fn bridge(origin: &str, workspace: &str, view: ElementRef) -> impl Fn(&str) + use<> {
    let endpoint = format!("{origin}/api/terminal/ws?workspace_id={workspace}");
    let commands = Rc::new(RefCell::new(None::<Sender<String>>));
    let (outbound, mut frames) = unbounded::<String>();
    let delivery = spawn(async move {
        while let Some(frame) = frames.next().await {
            view.post_message(frame);
        }
    });
    let on_drop = Rc::clone(&commands);
    cleanup(move || {
        on_drop.borrow_mut().take();
        delivery.cancel();
    });

    move |message| {
        let mut commands = commands.borrow_mut();
        if let Some(commands) = commands.as_ref() {
            let _ = commands.send(message.to_owned());
            return;
        }
        let Ok(size) = serde_json::from_str::<InitialSize>(message) else {
            return;
        };
        let url = format!("{endpoint}&cols={}&rows={}", size.cols, size.rows);
        let (send, receive) = mpsc::channel::<String>();
        let outbound = outbound.clone();
        if std::thread::Builder::new()
            .name("vibe-terminal".into())
            .spawn(move || {
                let run = || -> Result<(), Error> {
                    let url = url.replacen("http://", "ws://", 1);
                    let host = url
                        .strip_prefix("ws://")
                        .and_then(|url| url.split_once('/'))
                        .map(|(host, _)| host)
                        .ok_or_else(|| std::io::Error::other("Expected a local terminal URL"))?;
                    let connection = TcpStream::connect(host)?;
                    connection.set_read_timeout(Some(Duration::from_secs(3)))?;
                    connection.set_write_timeout(Some(Duration::from_secs(3)))?;
                    let (mut socket, _) = client(url, connection)
                        .map_err(|error| std::io::Error::other(error.to_string()))?;
                    socket
                        .get_ref()
                        .set_read_timeout(Some(Duration::from_millis(35)))?;
                    let _ = outbound.unbounded_send("open".into());
                    loop {
                        loop {
                            match receive.try_recv() {
                                Ok(command) => socket.send(Message::Text(command.into()))?,
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    let _ = socket.close(None);
                                    return Ok(());
                                }
                            }
                        }
                        match socket.read() {
                            Ok(Message::Text(frame)) => {
                                if outbound.unbounded_send(frame.to_string()).is_err() {
                                    let _ = socket.close(None);
                                    return Ok(());
                                }
                            }
                            Ok(Message::Close(_)) | Err(Error::ConnectionClosed) => break,
                            Ok(Message::Ping(_)) => socket.flush()?,
                            Ok(_) => {}
                            Err(Error::Io(error))
                                if matches!(
                                    error.kind(),
                                    ErrorKind::TimedOut | ErrorKind::WouldBlock
                                ) => {}
                            Err(error) => return Err(error),
                        }
                    }
                    let _ = outbound.unbounded_send("disconnected".into());
                    Ok(())
                };
                if run().is_err() {
                    let _ = outbound.unbounded_send("failed".into());
                }
            })
            .is_ok()
        {
            *commands = Some(send);
        } else {
            view.post_message("failed");
        }
    }
}
