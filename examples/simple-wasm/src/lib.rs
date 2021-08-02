use indexer_lib as il;

#[no_mangle]
fn run_indexer() {
    let trans = il::get_transaction();

    println!("It works!! Got transaction {:#?}", trans);
}
