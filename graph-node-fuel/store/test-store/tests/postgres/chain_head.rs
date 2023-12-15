//! Test ChainStore implementation of Store, in particular, how
//! the chain head pointer gets updated in various situations

use graph::blockchain::{BlockHash, BlockPtr};
use graph::env::ENV_VARS;
use graph::prelude::futures03::executor;
use std::future::Future;
use std::sync::Arc;

use graph::prelude::web3::types::H256;
use graph::prelude::{anyhow::anyhow, anyhow::Error};
use graph::prelude::{serde_json as json, EthereumBlock};
use graph::prelude::{BlockNumber, QueryStoreManager, QueryTarget};
use graph::{cheap_clone::CheapClone, prelude::web3::types::H160};
use graph::{components::store::BlockStore as _, prelude::DeploymentHash};
use graph::{components::store::ChainStore as _, prelude::EthereumCallCache as _};
use graph_store_postgres::Store as DieselStore;
use graph_store_postgres::{layout_for_tests::FAKE_NETWORK_SHARED, ChainStore as DieselChainStore};

use test_store::block_store::{
    FakeBlock, FakeBlockList, BLOCK_FIVE, BLOCK_FOUR, BLOCK_ONE, BLOCK_ONE_NO_PARENT,
    BLOCK_ONE_SIBLING, BLOCK_THREE, BLOCK_THREE_NO_PARENT, BLOCK_TWO, BLOCK_TWO_NO_PARENT,
    GENESIS_BLOCK, NO_PARENT,
};
use test_store::*;

// The ancestor count we use for chain head updates. We keep this very small
// to make setting up the tests easier
const ANCESTOR_COUNT: BlockNumber = 3;

/// Test harness for running database integration tests.
fn run_test<F>(chain: FakeBlockList, test: F)
where
    F: Fn(Arc<DieselChainStore>, Arc<DieselStore>) -> Result<(), Error> + Send + 'static,
{
    run_test_sequentially(|store| async move {
        for name in &[NETWORK_NAME, FAKE_NETWORK_SHARED] {
            block_store::set_chain(chain.clone(), name).await;

            let chain_store = store.block_store().chain_store(name).expect("chain store");

            // Run test
            test(chain_store.cheap_clone(), store.cheap_clone())
                .unwrap_or_else(|_| panic!("test finishes successfully on network {}", name));
        }
    });
}

fn run_test_async<R, F>(chain: FakeBlockList, test: F)
where
    F: Fn(Arc<DieselChainStore>, Arc<DieselStore>, Vec<(BlockPtr, BlockHash)>) -> R
        + Send
        + Sync
        + 'static,
    R: Future<Output = ()> + Send + 'static,
{
    run_test_sequentially(|store| async move {
        for name in &[NETWORK_NAME, FAKE_NETWORK_SHARED] {
            let cached = block_store::set_chain(chain.clone(), name).await;

            let chain_store = store.block_store().chain_store(name).expect("chain store");

            // Run test
            test(chain_store.cheap_clone(), store.clone(), cached).await;
        }
    });
}

/// Check that `attempt_chain_head_update` works as expected on the given
/// chain. After writing the blocks in `chain` to the store, call
/// `attempt_chain_head_update` and check its result. Check that the new head
/// is the one indicated in `head_exp`. If `missing` is not `None`, check that
/// `attempt_chain_head_update` reports that block as missing
fn check_chain_head_update_cache(
    chain: FakeBlockList,
    head_exp: Option<&'static FakeBlock>,
    missing: Option<&'static str>,
    cached_exp: usize,
) {
    let cached_exp = ENV_VARS.store.recent_blocks_cache_capacity.min(cached_exp);

    run_test_async(chain, move |store, _, cached| async move {
        let missing_act: Vec<_> = store
            .clone()
            .attempt_chain_head_update(ANCESTOR_COUNT)
            .await
            .expect("attempt_chain_head_update failed")
            .iter()
            .map(|h| format!("{:x}", h))
            .collect();
        let missing_exp: Vec<_> = missing.into_iter().collect();
        assert_eq!(missing_exp, missing_act);

        let head_hash_exp = head_exp.map(|block| block.hash.clone());
        let head_hash_act = store
            .chain_head_ptr()
            .await
            .expect("chain_head_ptr failed")
            .map(|ebp| ebp.hash_hex());
        assert_eq!(head_hash_exp, head_hash_act);

        assert_eq!(cached_exp, cached.len());
    })
}

fn check_chain_head_update(
    chain: FakeBlockList,
    head_exp: Option<&'static FakeBlock>,
    missing: Option<&'static str>,
) {
    let cached_exp = chain
        .iter()
        .filter(|block| block.number != 0)
        .fold((0, None), |(len, parent_hash), block| {
            match (len, parent_hash) {
                (0, None) => (1, Some(&block.hash)),
                (0, Some(_)) | (_, None) => unreachable!(),
                (len, Some(parent_hash)) => {
                    if &block.parent_hash == parent_hash {
                        (len + 1, Some(&block.hash))
                    } else {
                        (1, Some(&block.hash))
                    }
                }
            }
        })
        .0;

    check_chain_head_update_cache(chain, head_exp, missing, cached_exp);
}

#[test]
fn genesis_only() {
    check_chain_head_update(vec![&*GENESIS_BLOCK], Some(&GENESIS_BLOCK), None);
}

#[test]
fn genesis_plus_one() {
    check_chain_head_update(vec![&*GENESIS_BLOCK, &*BLOCK_ONE], Some(&BLOCK_ONE), None);
}

#[test]
fn genesis_plus_two() {
    check_chain_head_update(
        vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_TWO],
        Some(&*BLOCK_TWO),
        None,
    );
}

#[test]
fn genesis_plus_one_with_sibling() {
    // Two valid blocks at the same height should give an error, but
    // we currently get one of them at random
    let chain = vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_ONE_SIBLING];
    check_chain_head_update_cache(chain, Some(&*BLOCK_ONE), None, 1);
}

#[test]
fn short_chain_missing_parent() {
    let chain = vec![&*BLOCK_ONE_NO_PARENT];
    check_chain_head_update(chain, None, Some(NO_PARENT));
}

#[test]
fn long_chain() {
    let chain = vec![
        &*BLOCK_ONE,
        &*BLOCK_TWO,
        &*BLOCK_THREE,
        &*BLOCK_FOUR,
        &*BLOCK_FIVE,
    ];
    check_chain_head_update_cache(chain, Some(&*BLOCK_FIVE), None, 5);
}

#[test]
fn long_chain_missing_blocks_within_ancestor_count() {
    // BLOCK_THREE does not have a parent in the store
    let chain = vec![&*BLOCK_THREE, &*BLOCK_FOUR, &*BLOCK_FIVE];
    check_chain_head_update(chain, None, Some(&BLOCK_THREE.parent_hash));
}

#[test]
fn long_chain_missing_blocks_beyond_ancestor_count() {
    // We don't mind missing blocks ANCESTOR_COUNT many blocks out, in
    // this case BLOCK_ONE
    let chain = vec![&*BLOCK_TWO, &*BLOCK_THREE, &*BLOCK_FOUR, &*BLOCK_FIVE];
    check_chain_head_update(chain, Some(&*BLOCK_FIVE), None);
}

#[test]
fn long_chain_with_uncles() {
    let chain = vec![
        &*BLOCK_ONE,
        &*BLOCK_TWO,
        &*BLOCK_TWO_NO_PARENT,
        &*BLOCK_THREE,
        &*BLOCK_THREE_NO_PARENT,
        &*BLOCK_FOUR,
    ];
    check_chain_head_update_cache(chain, Some(&*BLOCK_FOUR), None, 4);
}

#[test]
fn test_get_block_number() {
    let chain = vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_TWO];
    let subgraph = DeploymentHash::new("nonExistentSubgraph").unwrap();

    run_test_async(chain, move |_, subgraph_store, _| {
        let subgraph = subgraph.cheap_clone();
        async move {
            create_test_subgraph(&subgraph, "type Dummy @entity { id: ID! }").await;

            let query_store = subgraph_store
                .query_store(
                    QueryTarget::Deployment(subgraph.cheap_clone(), Default::default()),
                    false,
                )
                .await
                .unwrap();

            let block = query_store
                .block_number(&GENESIS_BLOCK.block_hash())
                .await
                .expect("Found genesis block");
            assert_eq!(Some(0), block);

            let block = query_store
                .block_number(&BLOCK_ONE.block_hash())
                .await
                .expect("Found block 1");
            assert_eq!(Some(1), block);

            let block = query_store
                .block_number(&BLOCK_THREE.block_hash())
                .await
                .expect("Looked for block 3");
            assert!(block.is_none());
        }
    })
}

#[test]
fn block_hashes_by_number() {
    let chain = vec![
        &*GENESIS_BLOCK,
        &*BLOCK_ONE,
        &*BLOCK_TWO,
        &*BLOCK_TWO_NO_PARENT,
    ];
    run_test(chain, move |store, _| {
        let hashes = store.block_hashes_by_block_number(1).unwrap();
        assert_eq!(vec![BLOCK_ONE.block_hash()], hashes);

        let hashes = store.block_hashes_by_block_number(2).unwrap();
        assert_eq!(2, hashes.len());
        assert!(hashes.contains(&BLOCK_TWO.block_hash()));
        assert!(hashes.contains(&BLOCK_TWO_NO_PARENT.block_hash()));

        let hashes = store.block_hashes_by_block_number(127).unwrap();
        assert_eq!(0, hashes.len());

        let deleted = store
            .confirm_block_hash(1, &BLOCK_ONE.block_hash())
            .unwrap();
        assert_eq!(0, deleted);

        let deleted = store
            .confirm_block_hash(2, &BLOCK_TWO.block_hash())
            .unwrap();
        assert_eq!(1, deleted);

        // Make sure that we do not delete anything for a nonexistent block
        let deleted = store
            .confirm_block_hash(127, &GENESIS_BLOCK.block_hash())
            .unwrap();
        assert_eq!(0, deleted);

        let hashes = store.block_hashes_by_block_number(1).unwrap();
        assert_eq!(vec![BLOCK_ONE.block_hash()], hashes);

        let hashes = store.block_hashes_by_block_number(2).unwrap();
        assert_eq!(vec![BLOCK_TWO.block_hash()], hashes);
        Ok(())
    })
}

#[track_caller]
fn check_ancestor(
    store: &Arc<DieselChainStore>,
    child: &FakeBlock,
    offset: BlockNumber,
    exp: &FakeBlock,
) -> Result<(), Error> {
    let act = executor::block_on(
        store
            .cheap_clone()
            .ancestor_block(child.block_ptr(), offset),
    )?
    .map(json::from_value::<EthereumBlock>)
    .transpose()?
    .ok_or_else(|| anyhow!("block {} has no ancestor at offset {}", child.hash, offset))?;
    let act_hash = format!("{:x}", act.block.hash.unwrap());
    let exp_hash = &exp.hash;

    if &act_hash != exp_hash {
        Err(anyhow!(
            "expected hash `{}` but got `{}`",
            exp_hash,
            act_hash
        ))
    } else {
        Ok(())
    }
}

#[test]
fn ancestor_block_simple() {
    let chain = vec![
        &*GENESIS_BLOCK,
        &*BLOCK_ONE,
        &*BLOCK_TWO,
        &*BLOCK_THREE,
        &*BLOCK_FOUR,
        &*BLOCK_FIVE,
    ];

    run_test(chain, move |store, _| -> Result<(), Error> {
        check_ancestor(&store, &BLOCK_FIVE, 1, &BLOCK_FOUR)?;
        check_ancestor(&store, &BLOCK_FIVE, 2, &BLOCK_THREE)?;
        check_ancestor(&store, &BLOCK_FIVE, 3, &BLOCK_TWO)?;
        check_ancestor(&store, &BLOCK_FIVE, 4, &BLOCK_ONE)?;
        check_ancestor(&store, &BLOCK_FIVE, 5, &GENESIS_BLOCK)?;
        check_ancestor(&store, &BLOCK_THREE, 2, &BLOCK_ONE)?;

        for offset in [6, 7, 8, 50].iter() {
            let offset = *offset;
            let res = executor::block_on(
                store
                    .cheap_clone()
                    .ancestor_block(BLOCK_FIVE.block_ptr(), offset),
            );
            assert!(res.is_err());
        }

        let block = executor::block_on(store.ancestor_block(BLOCK_TWO_NO_PARENT.block_ptr(), 1))?;
        assert!(block.is_none());
        Ok(())
    });
}

#[test]
fn ancestor_block_ommers() {
    let chain = vec![
        &*GENESIS_BLOCK,
        &*BLOCK_ONE,
        &*BLOCK_ONE_SIBLING,
        &*BLOCK_TWO,
    ];

    run_test(chain, move |store, _| -> Result<(), Error> {
        check_ancestor(&store, &BLOCK_ONE, 1, &GENESIS_BLOCK)?;
        check_ancestor(&store, &BLOCK_ONE_SIBLING, 1, &GENESIS_BLOCK)?;
        check_ancestor(&store, &BLOCK_TWO, 1, &BLOCK_ONE)?;
        check_ancestor(&store, &BLOCK_TWO, 2, &GENESIS_BLOCK)?;
        Ok(())
    });
}

#[test]
fn eth_call_cache() {
    let chain = vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_TWO];

    run_test(chain, |store, _| {
        let address = H160([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let call: [u8; 6] = [1, 2, 3, 4, 5, 6];
        let return_value: [u8; 3] = [7, 8, 9];

        store
            .set_call(address, &call, BLOCK_ONE.block_ptr(), &return_value)
            .unwrap();

        let ret = store
            .get_call(address, &call, GENESIS_BLOCK.block_ptr())
            .unwrap();
        assert!(ret.is_none());

        let ret = store
            .get_call(address, &call, BLOCK_ONE.block_ptr())
            .unwrap()
            .unwrap();
        assert_eq!(&return_value, ret.as_slice());

        let ret = store
            .get_call(address, &call, BLOCK_TWO.block_ptr())
            .unwrap();
        assert!(ret.is_none());

        let new_return_value: [u8; 3] = [10, 11, 12];
        store
            .set_call(address, &call, BLOCK_TWO.block_ptr(), &new_return_value)
            .unwrap();
        let ret = store
            .get_call(address, &call, BLOCK_TWO.block_ptr())
            .unwrap()
            .unwrap();
        assert_eq!(&new_return_value, ret.as_slice());

        Ok(())
    })
}

#[test]
/// Tests only query correctness. No data is involved.
fn test_transaction_receipts_in_block_function() {
    let chain = vec![];
    run_test_async(chain, move |store, _, _| async move {
        let receipts = store
            .transaction_receipts_in_block(&H256::zero())
            .await
            .unwrap();
        assert!(receipts.is_empty())
    })
}
