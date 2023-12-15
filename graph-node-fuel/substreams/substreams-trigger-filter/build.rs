fn main() {
    println!("cargo:rerun-if-changed=proto");
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .extern_path(
            ".sf.near.codec.v1",
            "::substreams_near_core::pb::sf::near::type::v1",
        )
        // .extern_path(
        //     ".sf.ethereum.type.v2",
        //     "graph_chain_ethereum::codec::pbcodec",
        // )
        // .extern_path(".sf.arweave.type.v1", "graph_chain_arweave::codec::pbcodec")
        // .extern_path(".sf.cosmos.type.v1", "graph_chain_cosmos::codec")
        .out_dir("src/pb")
        .compile(&["proto/receipts.proto"], &["proto"])
        .expect("Failed to compile Substreams entity proto(s)");
}
