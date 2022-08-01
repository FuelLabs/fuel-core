#!/usr/bin/env bash

grep "openssl-sys" Cargo.lock &> /dev/null
test $? -ne 0
