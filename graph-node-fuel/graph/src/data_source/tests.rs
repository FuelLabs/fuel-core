use cid::Cid;

use crate::{
    blockchain::mock::{MockBlockchain, MockDataSource},
    ipfs_client::CidFile,
    prelude::Link,
};

use super::{
    offchain::{Mapping, Source},
    *,
};

#[test]
fn offchain_duplicate() {
    let a = new_datasource();
    let mut b = a.clone();

    // Equal data sources are duplicates.
    assert!(a.is_duplicate_of(&b));

    // The causality region, the creation block and the done status are ignored in the duplicate check.
    b.causality_region = a.causality_region.next();
    b.creation_block = Some(1);
    b.set_done_at(Some(1));
    assert!(a.is_duplicate_of(&b));

    // The manifest idx, the source and the context are relevant for duplicate detection.
    let mut c = a.clone();
    c.manifest_idx = 1;
    assert!(!a.is_duplicate_of(&c));

    let mut c = a.clone();
    c.source = Source::Ipfs(CidFile {
        cid: Cid::default(),
        path: Some("/foo".into()),
    });
    assert!(!a.is_duplicate_of(&c));

    let mut c = a.clone();
    c.context = Arc::new(Some(DataSourceContext::new()));
    assert!(!a.is_duplicate_of(&c));
}

#[test]
#[should_panic]
fn offchain_mark_processed_error() {
    let x = new_datasource();
    x.mark_processed_at(-1)
}

#[test]
fn data_source_helpers() {
    let offchain = new_datasource();
    let offchain_ds = DataSource::<MockBlockchain>::Offchain(offchain.clone());
    assert!(offchain_ds.causality_region() == offchain.causality_region);
    assert!(offchain_ds
        .as_offchain()
        .unwrap()
        .is_duplicate_of(&offchain));

    let onchain = DataSource::<MockBlockchain>::Onchain(MockDataSource {
        api_version: Version::new(1, 0, 0),
        kind: "mock/kind".into(),
        network: Some("mock_network".into()),
    });
    assert!(onchain.causality_region() == CausalityRegion::ONCHAIN);
    assert!(onchain.as_offchain().is_none());
}

fn new_datasource() -> offchain::DataSource {
    offchain::DataSource::new(
        offchain::OffchainDataSourceKind::Ipfs,
        "theName".into(),
        0,
        Source::Ipfs(CidFile {
            cid: Cid::default(),
            path: None,
        }),
        Mapping {
            language: String::new(),
            api_version: Version::new(0, 0, 0),
            entities: vec![],
            handler: String::new(),
            runtime: Arc::new(vec![]),
            link: Link {
                link: String::new(),
            },
        },
        Arc::new(None),
        Some(0),
        CausalityRegion::ONCHAIN.next(),
    )
}
