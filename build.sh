#!/bin/bash

rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

wasm-pack build --target web --release

rm -rf webtest/pkg
cp -r pkg webtest/pkg
rm -rf pkg
