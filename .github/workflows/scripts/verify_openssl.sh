#!/usr/bin/env bash

err() {
    echo -e "\e[31m\e[1merror:\e[0m $@" 1>&2;
}

status() {
    WIDTH=12
    printf "\e[32m\e[1m%${WIDTH}s\e[0m %s\n" "$1" "$2"
}

cargo tree -p fuel-core-bin --invert openssl-sys 2>&1 | grep -i "did not match any packages" &> /dev/null
test $? -ne 1
openssl=$?

if [ $openssl != 0 ]; then
  err "detected openssl"
  exit 1
else
  status "no openssl dependencies detected"
fi
