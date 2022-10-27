#!/usr/bin/env bash

docker build -t x86_64-linux-gnu -f Dockerfile.x86_64-unknown-linux-gnu-clang .

docker build -t aarch64-linux-gnu -f Dockerfile.aarch64-unknown-linux-gnu-clang .
