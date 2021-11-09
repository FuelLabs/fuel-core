#!/bin/bash

if [[ "$OSTYPE" == "linux-gnu" ]]; then
	set -e
	if [[ $(whoami) == "root" ]]; then
		MAKE_ME_ROOT=
	else
		MAKE_ME_ROOT=sudo
	fi

	if [ -f /etc/arch-release ]; then
		echo "Arch Linux detected."
		$MAKE_ME_ROOT pacman -Syu --needed --noconfirm cmake gcc openssl-1.0 pkgconf git clang llvm11 llvm11-libs postgresql-libs
		export OPENSSL_LIB_DIR="/usr/lib/openssl-1.0";
		export OPENSSL_INCLUDE_DIR="/usr/include/openssl-1.0"
	elif [ -f /etc/debian_version ]; then
		echo "Ubuntu/Debian Linux detected."
		$MAKE_ME_ROOT apt update
		$MAKE_ME_ROOT apt install -y cmake pkg-config libssl-dev git gcc build-essential git clang libclang-dev llvm libpq-dev
	else
		echo "This OS is not supported with this script at present. Sorry."
		exit 1
	fi
elif [[ "$OSTYPE" == "darwin"* ]]; then
	set -e
	echo "Mac OS (Darwin) detected."

	if ! which brew >/dev/null 2>&1; then
		/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
	fi

	brew update
	brew install openssl cmake llvm libpq
else
	echo "This OS is not supported with this script at present. Sorry."
	exit 1
fi

if ! which rustup >/dev/null 2>&1; then
	curl https://sh.rustup.rs -sSf | sh -s -- -y
	source ~/.cargo/env
	rustup default stable
else
	rustup update
	rustup default stable
fi

echo "Run source ~/.cargo/env now to update environment"