web-vlog
===

[![Latest version](https://img.shields.io/crates/v/web-vlog.svg)](https://crates.io/crates/web-vlog)
[![Documentation](https://docs.rs/web-vlog/badge.svg)](https://docs.rs/web-vlog)
![License](https://img.shields.io/crates/l/web-vlog.svg)

* [`web-vlog` documentation](https://docs.rs/web-vlog)

`web-vlog` implements `v-log` with the goal of being feature complete but minimal in size.
This goal is achieved by offloading the drawing to a webbrowser. The webpage is served
exactly once before changing to a websocket connection, which handles the potentially
high datarates. This setup doesn't have the performance of a direct GPU renderer, but
it has decent performance at very little compiletime and runtime cost for the vlogging
process itself.

The webpage uses SVG to render the vlogging surfaces and provides clickable links
to open the relevant lines in VSCode.

This crate depends on `sha1` and `base64` due to the websocket handshake, which requires both.
**Nothing is encrypted, as this is a debug utility, which should not be shipped in production code.**

## Usage

```rust
use v_log::message;

// Initialize the vlogger on any free port.
// This should be done as early as possible in the binary.
let port = web_vlog::init();
println!("Listening on port {port}");

// Now we need a webbrowser to connect to the port.
// This can be accelerated using the `open` crate.
let _ = open::that(format!("http://localhost:{port}/"));

// wait for a webbrowser to connect to the port.
web_vlog::wait_for_connection();

message!(target: "custom_target_1", "surface", "First message");
message!(target: "custom_target_2", "surface", "Second message");
message!(target: "custom_target_2::submodule", "surface", "Third message");
```

When called without environment variables, all 3 messages will be logged.
Using the environment variable `RUST_VLOG` it is possible to filter by target prefixes.
The environment variable is interpreted as a comma separated list of target prefix filters.
Each filter, allows all targets which start with it to be vlogged. In our example
above, running it with
```cmd
$ RUST_VLOG=custom_target_1 ./main
```
would only produce the message "First message". When instead the second target is specified
```cmd
$ RUST_VLOG=custom_target_2 cargo run
```
the output is "Second message" and "Third message". This is due to the filter being a prefix filter.
Executing the executable directly with an environment variable, and executing using
`cargo run` both work. This way it is also possible to use filtering in tests using `RUST_VLOG=... cargo test`.
Tests in a library should only use a vlogger implementation as dev-dependency.

The target filters can also be chosen in the programm using the `Builder` to initialize the `WebVLogger`.
That would be done using the following code:
```rust
// Init a vlogger on port 1234, ignoring the environment variable and
// choosing "custom_target_1" as an allowed prefix for the vlogger.
web_vlog::Builder::new().port(1234).add_target("custom_target_1").init().unwrap();
```

License: MIT OR Apache-2.0
