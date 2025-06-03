#!/bin/bash

rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

wasm-pack test --chrome
