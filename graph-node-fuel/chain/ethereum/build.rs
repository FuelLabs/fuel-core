fn main() {
    println!("cargo:rerun-if-changed=proto");

    tonic_build::configure()
        .out_dir("src/protobuf")
        .compile(&["proto/ethereum.proto"], &["proto"])
        .expect("Failed to compile Firehose Ethereum proto(s)");
}
