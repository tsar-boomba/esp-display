#!/bin/sh

# Of form "host: {target triple}"
HOST_TARGET=$(rustc --version --verbose | grep 'host:')
cargo run -p sim --target ${HOST_TARGET:6}
