fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .out_dir("src/protobuf")
        .compile(&["proto/codec.proto"], &["proto"])
        .expect("Failed to compile Substreams entity proto(s)");
}
