fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .type_attribute(".", "#[allow(clippy::large_enum_variant)]")
        .compile_protos(&["proto/api.proto"], &["proto/"])?;
    Ok(())
}
