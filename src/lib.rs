use anyhow::{Error, Result};
use base64::{prelude::BASE64_STANDARD, Engine};
use sha1::Digest;
use std::{
    io::{prelude::*, BufReader, BufWriter},
    net::*,
    sync::mpsc::{channel, Receiver, Sender},
};
use v_log::{Color, Record, SetVLoggerError, VLog, Visual};

struct WebVLogger {
    sender: Option<Sender<String>>,
}

impl WebVLogger {
    fn new() -> Self {
        Self { sender: None }
    }
    fn init(mut self) -> Result<(), SetVLoggerError> {
        let (tx, rx) = channel();
        self.sender = Some(tx);
        v_log::set_boxed_vlogger(Box::new(self))?;
        // If the vlogger is successfully set, start the webserver.
        std::thread::spawn(move || {
            main_loop_socket(rx);
        });
        std::thread::spawn(main_loop);
        Ok(())
    }
}

impl VLog for WebVLogger {
    fn enabled(&self, _metadata: &v_log::Metadata) -> bool {
        true
    }
    fn vlog(&self, record: &Record) {
        // convert the record into a message to be send to the frontend.
        if let Some(sender) = self.sender.as_ref() {
            let surface = record.surface().escape_default();
            let msg;
            let size = record.size();
            let hexcode;
            let color = match *record.color() {
                Color::Base => format_args!("var(--base)"),
                Color::Healthy => format_args!("var(--healthy)"),
                Color::Error => format_args!("var(--error)"),
                Color::Warn => format_args!("var(--warn)"),
                Color::Info => format_args!("var(--info)"),
                Color::X => format_args!("var(--x)"),
                Color::Y => format_args!("var(--y)"),
                Color::Z => format_args!("var(--z)"),
                Color::Hex(code) => {
                    hexcode = code;
                    format_args!("#{hexcode:08X}")
                }
            };
            let meta = format_args!(
                "{};{}/{}:{}",
                record.target().escape_default(),
                env!("CARGO_MANIFEST_DIR").escape_default(),
                record.file().unwrap_or("").trim_start_matches('.').escape_default(),
                record.line().unwrap_or(0)
            );
            match record.visual() {
                Visual::Message => {
                    msg = format!(
                        "{{\"msg\":\"{}\",\"surf\":\"{surface}\",\"color\":\"{color}\",\"meta\":\"{meta}\"}}",
                        record.args().to_string().escape_default()
                    );
                }
                Visual::Label { x, y, z, alignment } => {
                    msg = format!(
                        "{{\"label\":\"{}\",\"pos\":[{x},{y},{z}],\"align\":{},\"surf\":\"{surface}\",\"size\":{size},\"color\":\"{color}\",\"meta\":\"{meta}\"}}",
                        record.args().to_string().escape_default(),
                        *alignment as u8
                    );
                }
                Visual::Point { x, y, z, style } => {
                    msg = format!(
                        "{{\"label\":\"{}\",\"pos\":[{x},{y},{z}],\"style\":\"{:?}\",\"surf\":\"{surface}\",\"size\":{size},\"color\":\"{color}\",\"meta\":\"{meta}\"}}",
                        record.args().to_string().escape_default(),
                        style
                    );
                }
                Visual::Line {
                    x1,
                    y1,
                    z1,
                    x2,
                    y2,
                    z2,
                    style,
                } => {
                    msg = format!(
                        "{{\"label\":\"{}\",\"pos\":[{x1},{y1},{z1}],\"pos2\":[{x2},{y2},{z2}],\"style\":\"{:?}\",\"surf\":\"{surface}\",\"size\":{size},\"color\":\"{color}\",\"meta\":\"{meta}\"}}",
                        record.args().to_string().escape_default(),
                        style
                    );
                }
            }
            let _ = sender.send(msg);
        }
    }
    fn clear(&self, surface: &str) {
        if let Some(sender) = self.sender.as_ref() {
            let _ = sender.send(format!(
                "{{\"clear\":1,\"surf\":\"{}\"}}",
                surface.escape_default()
            ));
        }
    }
}

fn main_loop() {
    std::panic::set_hook(Box::new(|_info| log::error!("http server thread panicked")));
    let listener = TcpListener::bind("127.0.0.1:13700").unwrap();
    for mut stream in listener.incoming().flatten() {
        if let Err(err) = handle_connection(&mut stream) {
            if let Err(err) = stream
                .write_all(format!("HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\n{err}").as_bytes())
            {
                log::error!("an error occurred: {err:?}");
            }
        }
    }
}

fn main_loop_socket(rx: Receiver<String>) {
    std::panic::set_hook(Box::new(|_info| log::error!("WebSocket thread panicked")));
    let listener = TcpListener::bind("127.0.0.1:13701").unwrap();
    while let Ok((stream, addr)) = listener.accept() {
        log::info!("vlog connection {addr}");
        // only allow one socket connection at the moment, so only one receiver is needed.
        if let Err(err) = handle_socket_connection(stream, &rx) {
            log::error!("an error occurred: {err:?}");
        }
    }
}

/// Initialise the logger with its default configuration.
///
/// Log messages will not be filtered.
/// The `RUST_LOG` environment variable is not used.
pub fn init() -> Result<(), SetVLoggerError> {
    WebVLogger::new().init()
}

fn handle_connection(stream: &mut TcpStream) -> Result<(), Error> {
    let buf_reader = BufReader::new(&*stream);
    // only use the first line
    let http_request = buf_reader
        .lines()
        .next()
        .ok_or(Error::msg("empty/invalid http request"))??;

    log::debug!("{http_request}");
    let (get, rest) = http_request.split_once(' ').unwrap_or(("", ""));
    let (path, http) = rest.split_once(' ').unwrap_or(("", ""));

    if get == "GET" && http == "HTTP/1.1" {
        if path == "/" {
            stream.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes())?;
            stream.write_all(include_bytes!("site.html"))?;
        } else {
            stream.write_all(
                "HTTP/1.1 404 NOT FOUND\r\n\r\n<html><body>Path not found</body></html>".as_bytes(),
            )?;
        }
    } else {
        stream.write_all("HTTP/1.1 400 BAD REQUEST\r\n\r\n".as_bytes())?;
    }
    Ok(())
}

fn handle_socket_connection(stream: TcpStream, rx: &Receiver<String>) -> Result<(), anyhow::Error> {
    // see https://datatracker.ietf.org/doc/html/rfc6455
    let mut buf_reader = BufReader::new(&stream);
    let mut buf_writer = BufWriter::new(&stream);

    let mut buf = String::new();
    let mut key_back = String::new();
    'handshake: while let Ok(_bytes) = buf_reader.read_line(&mut buf) {
        let (get, _) = buf.split_once(' ').unwrap_or(("", ""));
        if get == "GET" {
            buf.clear();
            while let Ok(_bytes) = buf_reader.read_line(&mut buf) {
                if buf.trim().is_empty() {
                    buf_writer.write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {key_back}\r\n\r\n").as_bytes())?;
                    buf_writer.flush()?;
                    break 'handshake;
                }
                if let Some(key) = buf.strip_prefix("Sec-WebSocket-Key: ") {
                    let key = key.trim().to_owned() + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
                    let digest = sha1::Sha1::digest(key);
                    key_back = BASE64_STANDARD.encode(digest);
                }
                buf.clear();
            }
        }
    }
    log::debug!("connected client successfully");
    // ignore all received data.
    while let Ok(msg) = rx.recv() {
        // send message
        if msg.len() < 126 {
            buf_writer.write_all(&[0x81, msg.len() as u8])?;
            buf_writer.write_all(msg.as_bytes())?;
        } else if msg.len() <= u16::MAX as usize {
            buf_writer.write_all(&[0x81, 126])?;
            buf_writer.write_all(&bytemuck::cast_slice(&[(msg.len() as u16).swap_bytes()]))?;
            buf_writer.write_all(msg.as_bytes())?;
        } else {
            buf_writer.write_all(&[0x81, 127])?;
            buf_writer.write_all(&bytemuck::cast_slice(&[(msg.len() as u64).swap_bytes()]))?;
            buf_writer.write_all(msg.as_bytes())?;
        }
        buf_writer.flush()?;
    }
    Ok(())
}
