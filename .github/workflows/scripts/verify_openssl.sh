#!/usr/bin/env bash

err() {
    echo -e "\e[31m\e[1merror:\e[0m $@" 1>&2;
}

status() {
    WIDTH=12
    printf "\e[32m\e[1m%${WIDTH}s\e[0m %s\n" "$1" "$2"
}

grep "openssl-sys" Cargo.lock &> /dev/null
test $? -ne 0
openssl=$?

if [ $openssl != 0 ]; then
  err "detected openssl"
  exit 1
else
  status "no openssl dependencies detected"
fi
