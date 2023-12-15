fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .out_dir("src/protobuf")
        .extern_path(".sf.near.codec.v1", "crate::codec::pbcodec")
        .compile(
            &["proto/near.proto", "proto/substreams-triggers.proto"],
            &["proto"],
        )
        .expect("Failed to compile Firehose NEAR proto(s)");
}
