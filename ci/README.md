CI Tooling
===

Cross-platform compilation is handled using [cross](https://github.com/cross-rs/cross). 
This is because cross ensures binaries will run on as many platforms as possible. 
It also works-around various [bugs](https://github.com/rust-lang/rust-bindgen/issues/1229) 
in rust bind-gen that prevent us from simply using `--target` with cargo to cross-compile.

We use custom extensions of the docker images provided by cross 
in order to support building rocksdb with clang. Each target requires its own dockerfile,
as they all have slightly different distros, package managers and compiler toolchains.

To test cross compilation of fuel core locally, build the provided dockerfiles
with the image tags expected by Cross.toml using the helper script: 

```shell
./build-images.sh
```

To cross-compile fuel core for a supported target, you can build it as you
would with cargo but using cross instead:

```shell
cargo install cross
cross build --profile=release --target ${target} -p fuel-core
```
