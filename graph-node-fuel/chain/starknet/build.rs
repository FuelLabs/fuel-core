fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .out_dir("src/protobuf")
        .compile(&["proto/starknet.proto"], &["proto"])
        .expect("Failed to compile Firehose StarkNet proto(s)");
}
