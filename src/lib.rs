//! `web-vlog` implements `v-log` with the goal of being feature complete but minimal in size.
//! This goal is achieved by offloading the drawing to a webbrowser. The webpage is served
//! exactly once before changing to a websocket connection, which handles the potentially
//! high datarates. This setup doesn't have the performance of a direct GPU renderer, but
//! it has decent performance at very little compiletime and runtime cost for the vlogging
//! process itself.
//!
//! The webpage uses SVG to render the vlogging surfaces and provides clickable links
//! to open the relevant lines in VSCode.
//!
//! This crate depends on `sha1` and `base64` due to the websocket handshake, which requires both.
//! **Nothing is encrypted, as this is a debug utility, which should not be shipped in production code.**
//!
//! # Usage
//!
//! ```
//! use v_log::message;
//!
//! // Initialize the vlogger on any free port.
//! // This should be done as early as possible in the binary.
//! let port = web_vlog::init();
//! println!("Listening on port {port}");
//!
//! // Now we need a webbrowser to connect to the port.
//! // This can be accelerated using the `open` crate.
//! let _ = open::that(format!("http://localhost:{port}/"));
//!
//! // wait for a webbrowser to connect to the port.
//! web_vlog::wait_for_connection();
//!
//! message!(target: "custom_target_1", "surface", "First message");
//! message!(target: "custom_target_2", "surface", "Second message");
//! message!(target: "custom_target_2::submodule", "surface", "Third message");
//! # std::thread::sleep(std::time::Duration::from_millis(100));
//! ```
//!
//! When called without environment variables, all 3 messages will be logged.
//! Using the environment variable `RUST_VLOG` it is possible to filter by target prefixes.
//! The environment variable is interpreted as a comma separated list of target prefix filters.
//! Each filter, allows all targets which start with it to be vlogged. In our example
//! above, running it with
//! ```cmd
//! $ RUST_VLOG=custom_target_1 ./main
//! ```
//! would only produce the message "First message". When instead the second target is specified
//! ```cmd
//! $ RUST_VLOG=custom_target_2 cargo run
//! ```
//! the output is "Second message" and "Third message". This is due to the filter being a prefix filter.
//! Executing the executable directly with an environment variable, and executing using
//! `cargo run` both work. This way it is also possible to use filtering in tests using `RUST_VLOG=... cargo test`.
//! Tests in a library should only use a vlogger implementation as dev-dependency.
//!
//! The target filters can also be chosen in the programm using the [`Builder`] to initialize the [`WebVLogger`].
//! That would be done using the following code:
//! ```
//! // Init a vlogger on port 1234, ignoring the environment variable and
//! // choosing "custom_target_1" as an allowed prefix for the vlogger.
//! web_vlog::Builder::new().port(1234).add_target("custom_target_1").init().unwrap();
//! ```

use base64::{prelude::BASE64_STANDARD, Engine};
use sha1::Digest;
use std::{
    fmt::{self, Write as _},
    io::{self, prelude::*, BufReader, BufWriter},
    net::*,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Condvar, Mutex,
    },
};
use v_log::{Color, Record, SetVLoggerError, VLog, Visual};

static WAIT: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// A builder for [`WebVLogger`].
pub struct Builder {
    port: u16,
    targets: Vec<String>,
}
/// A Vlogger implementation, which hosts a webpage for the visualisation.
pub struct WebVLogger {
    sender: Sender<String>,
    targets: Vec<String>,
}

/// The error type returned by [`init`].
///
/// [`init`]: fn.init.html
#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub enum InitError {
    SetVLoggerError(SetVLoggerError),
    TcpError(io::Error),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SetVLoggerError(e) => e.fmt(f),
            Self::TcpError(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for InitError {}

impl From<SetVLoggerError> for InitError {
    fn from(value: SetVLoggerError) -> Self {
        Self::SetVLoggerError(value)
    }
}
impl From<io::Error> for InitError {
    fn from(value: io::Error) -> Self {
        Self::TcpError(value)
    }
}

impl Builder {
    /// Create a new [`Builder`] for [`WebVLogger`] with
    /// the default port `0`, which means the OS will choose the port.
    pub fn new() -> Self {
        Self {
            port: 0,
            targets: vec![],
        }
    }
    /// Set the port on which the server will be made available.
    ///
    /// If set to 0, an available port will be choosen by the OS.
    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port = port;
        self
    }
    /// Add a target to the target whitelist.
    /// If the whitelist is left empty, all targets are allowed.
    pub fn add_target(&mut self, target: &str) -> &mut Self {
        self.targets.push(target.to_owned());
        self
    }
    /// Read the targets from the
    pub fn targets_from_env(&mut self) -> &mut Self {
        if let Ok(var) = std::env::var("RUST_VLOG") {
            for target in var.split(",") {
                let target = target.trim();
                if !target.is_empty() {
                    self.add_target(target);
                }
            }
        }
        self
    }
    /// Initialize the [`WebVLogger`] and set it as the global vlogger for [`v_log`].
    ///
    /// Returns the actual port, which the server runs on.
    /// This is only relevant if the port was set to 0.
    ///
    /// # Errors
    ///
    /// If the global vlogger has already been set an [`InitError::SetVLoggerError`] is returned.
    /// If the server could not be started on the chosen port, the [`std::io::Error`] is returned inside [`InitError::TcpError`].
    pub fn init(&self) -> Result<u16, InitError> {
        let port = self.port;
        let (sender, rx) = channel();
        let mut vlogger = WebVLogger {
            sender,
            targets: self.targets.clone(),
        };
        vlogger.targets.sort();
        vlogger.targets.dedup();
        // first try to set the vlogger.
        v_log::set_boxed_vlogger(Box::new(vlogger))?;
        // then try to open the port on localhost
        // If this fails, the `rx` will be dropped.
        // The vlogger will therefore stop.
        let listener = TcpListener::bind(("localhost", port))?;
        let addr = listener.local_addr()?;
        log::info!("web-vlog server started on {addr}");
        // If the vlogger is successfully set, start the webserver.
        std::thread::spawn(move || {
            server_loop(listener, rx);
        });
        if port != 0 {
            assert_eq!(port, addr.port());
        }
        Ok(addr.port())
    }
}

impl VLog for WebVLogger {
    fn enabled(&self, metadata: &v_log::Metadata) -> bool {
        self.targets.is_empty()
            || self
                .targets
                .iter()
                .any(|target| metadata.target().starts_with(target))
    }
    fn vlog(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // convert the record into a message to be send to the frontend.
        let surface = record.surface().escape_default();
        let size = record.size();
        let color_meta = |start| {
            let mut msg = format!("{start},\"meta\":{{\"target\":\"{}\",\"file\":\"{}/{}\",\"line\":{}}},\"col\":\"",
                record.target().escape_default(),
                env!("CARGO_MANIFEST_DIR").escape_default(),
                record.file()
                      .unwrap_or("")
                      .trim_start_matches('.')
                      .escape_default(),
                record.line().unwrap_or(0),
            );
            match *record.color() {
                Color::Base => msg.push_str("var(--base)\"}"),
                Color::Healthy => msg.push_str("var(--healthy)\"}"),
                Color::Error => msg.push_str("var(--error)\"}"),
                Color::Warn => msg.push_str("var(--warn)\"}"),
                Color::Info => msg.push_str("var(--info)\"}"),
                Color::X => msg.push_str("var(--x)\"}"),
                Color::Y => msg.push_str("var(--y)\"}"),
                Color::Z => msg.push_str("var(--z)\"}"),
                Color::Hex(hexcode) => {
                    write!(&mut msg, "#{hexcode:08X}\"}}").unwrap()
                }
                _ => unimplemented!(),
            }
            msg
        };
        let mut tmp = String::new();
        let label = record.args().as_str().map_or_else(
            || {
                tmp = record.args().to_string();
                tmp.escape_default()
            },
            |s| s.escape_default(),
        );
        let msg = match record.visual() {
            Visual::Message => {
                color_meta(format_args!("{{\"msg\":\"{label}\",\"surf\":\"{surface}\""))
            }
            Visual::Label { x, y, z, alignment } => {
                color_meta(format_args!("{{\"lbl\":\"{label}\",\"pos\":[{x},{y},{z}],\"align\":{},\"surf\":\"{surface}\",\"size\":{size}", *alignment as u8))
            }
            Visual::Point { x, y, z, style } => {
                color_meta(format_args!("{{\"lbl\":\"{label}\",\"pos\":[{x},{y},{z}],\"style\":\"{style:?}\",\"surf\":\"{surface}\",\"size\":{size}"))
            }
            Visual::Line { x1, y1, z1, x2, y2, z2, style } => {
                color_meta(format_args!("{{\"lbl\":\"{label}\",\"pos\":[{x1},{y1},{z1}],\"pos2\":[{x2},{y2},{z2}],\"style\":\"{style:?}\",\"surf\":\"{surface}\",\"size\":{size}"))
            }
        };
        // If the receiver is dropped, the messages will still be constructed, but no longer sent.
        // This case doesn't have to be optimized with an early return, as it's the error state.
        let _ = self.sender.send(msg);
    }
    fn clear(&self, surface: &str) {
        let _ = self.sender.send(format!(
            "{{\"clear\":1,\"surf\":\"{}\"}}",
            surface.escape_default()
        ));
    }
}

/// Initialise the vlogger with a custom port and otherwise default configuation.
/// If the custom port is set to 0, a free port will be choosen by the OS and
/// returned by this function. This function never panics.
///
/// Vlog messages will not be filtered.
/// The `RUST_VLOG` environment variable is not used.
pub fn init_port(port: u16) -> Result<u16, InitError> {
    Builder::new().port(port).init()
}

/// Initialise the vlogger with the default configuation.
/// The target whitelist gets loaded from the environment variable
/// `RUST_VLOG`. If it is not set, all targets are whitelisted.
///
/// Returns the port at which the server is made available.
///
/// # Panics
///
/// This function will panic if the vlogger has already been
/// set or the server could not be started. For a non panicking
/// version see [`init_port`].
pub fn init() -> u16 {
    Builder::new().targets_from_env().init().unwrap()
}

/// Wait for a client to connect to the vlogging server.
/// This blocks indefinitely if no server has been started.
#[allow(dead_code)]
pub fn wait_for_connection() {
    let lock = WAIT.0.lock().unwrap();
    let _lock = WAIT.1.wait_while(lock, |v| !*v).unwrap();
}

fn server_loop(listener: TcpListener, rx: Receiver<String>) {
    // It's ok to panic in this thread to notify the user that something went wrong.
    while let Ok((mut stream, addr)) = listener.accept() {
        log::info!("vlogger connection from {addr}");
        if let Err(err) = handle_connection(&stream, &rx) {
            if let Err(err) = stream
                .write_all(format!("HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\n{err}").as_bytes())
            {
                log::error!("an error occurred: {err:?}");
            }
        }
    }
}

fn handle_connection(stream: &TcpStream, rx: &Receiver<String>) -> std::io::Result<()> {
    let mut buf_reader = BufReader::new(stream);
    let mut buf_writer = BufWriter::new(stream);
    // only use the first line
    let mut buf = String::new();
    let mut http_request = String::new();
    let mut key_back = String::new();
    while let Ok(bytes) = buf_reader.read_line(&mut buf) {
        let l = buf.trim_end();
        log::debug!("{l}");
        if bytes == 0 || l.is_empty() {
            break;
        }
        if http_request.is_empty() {
            http_request.push_str(l);
        }
        // see https://datatracker.ietf.org/doc/html/rfc6455
        else if let Some(key) = l.strip_prefix("Sec-WebSocket-Key: ") {
            let key = key.to_owned() + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
            let digest = sha1::Sha1::digest(key);
            key_back = BASE64_STANDARD.encode(digest);
        }
        buf.clear();
    }
    let (get, rest) = http_request.split_once(' ').unwrap_or(("", ""));
    let (path, http) = rest.split_once(' ').unwrap_or(("", ""));
    if get == "GET" && http == "HTTP/1.1" {
        if !key_back.is_empty() {
            log::debug!("vlogging client connected");
            {
                let mut guard = WAIT.0.lock().unwrap();
                *guard = true;
                WAIT.1.notify_all();
            }
            buf_writer.write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {key_back}\r\n\r\n").as_bytes())?;
            buf_writer.flush()?;
            stream.set_nonblocking(true)?;
            let mut byte_buf = [0u8; 64];
            while let Ok(msg) = rx.recv() {
                // first check if a socket close is received
                while let Ok(bytes) = buf_reader.read(&mut byte_buf) {
                    // don't parse it properly. Only ever expect close events to happen.
                    // if bytes = 0, the connection has ended already without the closing message.
                    if bytes == 0 || byte_buf[..bytes].iter().any(|b| *b == 0x88) {
                        // close connection so the server can listen for a new connection.
                        log::info!("vlogger connection closed");
                        {
                            let mut guard = WAIT.0.lock().unwrap();
                            *guard = false;
                            WAIT.1.notify_all();
                        }
                        return Ok(());
                    }
                }
                // send message
                if msg.len() < 126 {
                    buf_writer.write_all(&[0x81, msg.len() as u8])?;
                    buf_writer.write_all(msg.as_bytes())?;
                } else if msg.len() <= u16::MAX as usize {
                    buf_writer.write_all(&[0x81, 126])?;
                    buf_writer.write_all(&(msg.len() as u16).to_be_bytes())?;
                    buf_writer.write_all(msg.as_bytes())?;
                } else {
                    buf_writer.write_all(&[0x81, 127])?;
                    buf_writer.write_all(&(msg.len() as u64).to_be_bytes())?;
                    buf_writer.write_all(msg.as_bytes())?;
                }
                buf_writer.flush()?;
            }
        } else if path == "/" {
            buf_writer.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes())?;
            buf_writer.write_all(include_bytes!("site.html"))?;
        } else {
            buf_writer.write_all(
                "HTTP/1.1 404 NOT FOUND\r\n\r\n<html><body>Path not found</body></html>".as_bytes(),
            )?;
        }
    } else {
        buf_writer.write_all("HTTP/1.1 400 BAD REQUEST\r\n\r\n".as_bytes())?;
    }
    stream.set_nonblocking(false)?;
    buf_writer.flush()?;
    Ok(())
}
