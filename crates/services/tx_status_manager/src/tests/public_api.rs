#[test]
fn public_api() {
    rustup_toolchain::install(public_api::MINIMUM_NIGHTLY_RUST_VERSION).unwrap();

    let rustdoc_json = rustdoc_json::Builder::default()
        .toolchain(public_api::MINIMUM_NIGHTLY_RUST_VERSION)
        .build()
        .unwrap();

    let public_api = public_api::Builder::from_rustdoc_json(rustdoc_json)
        .build()
        .unwrap();

    insta::assert_snapshot!(public_api);
}
