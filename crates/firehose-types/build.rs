use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(
        &["type.proto"],
        &["../../firehose-fuel/proto/sf/fuel/type/v1/"],
    )?;
    Ok(())
}
