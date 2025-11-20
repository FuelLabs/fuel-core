fn main() {
    println!("cargo:rerun-if-changed=./assets/debugAdapterProtocol.json");

    generate_dap_schema();
}

fn generate_dap_schema() {
    use schemafy_lib::{Expander, Schema};
    use std::{
        env,
        fs::{self, File},
        io::prelude::*,
        path::PathBuf,
    };

    let path = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("assets/debugAdapterProtocol.json"))
        .expect("Failed to fetch JSON schema");

    let json = fs::read_to_string(&path).expect("Failed to parse JSON from schema");
    let schema: Schema =
        serde_json::from_str(&json).expect("Failed to parse Schema from JSON");
    let root_name = schema.title.clone().unwrap_or_else(|| "Root".to_owned());

    let path = path.into_os_string();
    let path = path.into_string().expect("Failed to convert path");
    let mut expander = Expander::new(Some(&root_name), path.as_str(), &schema);
    let contents = expander.expand(&schema).to_string();

    let schema = env::var("OUT_DIR")
        .map(PathBuf::from)
        .map(|mut f| {
            f.set_file_name("schema");
            f.set_extension("rs");
            f
        })
        .expect("Failed to fetch schema path");

    File::create(schema)
        .and_then(|mut f| {
            f.write_all(contents.as_bytes())?;
            f.sync_all()
        })
        .expect("Failed to create schema.rs file");
}
