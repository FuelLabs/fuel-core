use super::universe::TxPoolUniverse;

#[test]
fn simple_insert() {
    let mut universe = TxPoolUniverse::new();
    let tx = universe.create_default_transaction(0);
    let insertion_results = universe.insert_transactions_in_pool(vec![tx]);
    assert_eq!(insertion_results.len(), 1);
    for result in insertion_results {
        assert!(result.is_ok())
    }
}
