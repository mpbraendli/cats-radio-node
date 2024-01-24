# CATS Radio Node

This project contains a web user interface for controlling a
[https://cats.radio](CATS Radio) device, consisting of a Raspberry Pi with a
[https://gitlab.scd31.com/cats/pi-hardware](RF4463 hat).

## Goals

1. Show incoming packets, and store them in the sqlite database `cats-radio-node.db`
1. Allow the user to send custom packets
1. Configure igate and other settings, stored in `node-config.toml`

## Current state of the project

Configuration read/write through UI is done.
RF4463 integration, message decoding and presentation, UI to send messages.
Tunnel IP packets through Arbitrary whiskers, using TUN.
Live update of incoming packets using WebSocket, in the 'Chat' window.

### TODO:

* Nicer UI for presenting incoming packets. For now it just shows the Comment whisker.
* igate integration

## Additional tools

### fake-radio

If no radio is available, frames can be sent and received over UDP for debugging.
cats-radio-node receives on 127.0.0.1:9073, and transmits to 127.0.0.1:9074.

The `fake-radio` binary can be used to inject frames for that, and decodes those sent by cats-radio-node.

Build with `cargo build --bin fake-radio`

## Remarks

Careful when installing Rust on a Raspberry Pi with a 64-bit kernel running a 32-bit userland: `rustup` will want
to install the aarch64 toolchain, but that one doesn't work!

If that happens, be sure to select the `stable-arm-unknown-linux-gnueabihf` toolchain, and set it as default using
`rustup default`.

