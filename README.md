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

Pending: RF4463 integration, message decoding and presentation, igate integration, UI to send messages

## Additional tools

### fake-radio

If no radio is available, frames can be sent and received over UDP for debugging.
cats-radio-node receives on 127.0.0.1:9073, and transmits to 127.0.0.1:9074.

The `fake-radio` binary can be used to inject frames for that, and decodes those sent by cats-radio-node.

Build with `cargo build --bin fake-radio`
