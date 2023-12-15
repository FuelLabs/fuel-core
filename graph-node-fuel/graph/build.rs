fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .out_dir("src/firehose")
        .compile(
            &[
                "proto/firehose.proto",
                "proto/ethereum/transforms.proto",
                "proto/near/transforms.proto",
                "proto/cosmos/transforms.proto",
            ],
            &["proto"],
        )
        .expect("Failed to compile Firehose proto(s)");

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .out_dir("src/substreams")
        .compile(&["proto/substreams.proto"], &["proto"])
        .expect("Failed to compile Substreams proto(s)");

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .extern_path(".sf.substreams.v1", "crate::substreams")
        .out_dir("src/substreams_rpc")
        .compile(&["proto/substreams-rpc.proto"], &["proto"])
        .expect("Failed to compile Substreams RPC proto(s)");
}
