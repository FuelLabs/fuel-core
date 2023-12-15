const PROTO_FILE: &str = "proto/cosmos.proto";

fn main() {
    println!("cargo:rerun-if-changed=proto");

    let types =
        graph_chain_common::parse_proto_file(PROTO_FILE).expect("Unable to parse proto file!");

    let array_types = types
        .iter()
        .flat_map(|(_, t)| t.fields.iter())
        .filter(|t| t.is_array)
        .map(|t| t.type_name.clone())
        .collect::<std::collections::HashSet<_>>();

    let mut builder = tonic_build::configure().out_dir("src/protobuf");

    for (name, ptype) in types {
        //generate Asc<Type>
        builder = builder.type_attribute(
            name.clone(),
            format!(
                "#[graph_runtime_derive::generate_asc_type({})]",
                ptype.fields().unwrap_or_default()
            ),
        );

        //generate data index id
        builder = builder.type_attribute(
            name.clone(),
            "#[graph_runtime_derive::generate_network_type_id(Cosmos)]",
        );

        //generate conversion from rust type to asc
        builder = builder.type_attribute(
            name.clone(),
            format!(
                "#[graph_runtime_derive::generate_from_rust_type({})]",
                ptype.fields().unwrap_or_default()
            ),
        );

        if array_types.contains(&ptype.name) {
            builder = builder.type_attribute(
                name.clone(),
                "#[graph_runtime_derive::generate_array_type(Cosmos)]",
            );
        }
    }

    builder
        .compile(&[PROTO_FILE], &["proto"])
        .expect("Failed to compile Firehose Cosmos proto(s)");
}
