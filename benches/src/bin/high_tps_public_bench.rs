// Define arguments

use console::{
    style,
    Emoji,
};
use fuel_core::{
    service::config::Trigger,
    upgradable_executor::native_executor::ports::TransactionExt,
};
use fuel_core_chain_config::{
    ChainConfig,
    CoinConfig,
    SnapshotMetadata,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_asm::{
        op,
        GMArgs,
        GTFArgs,
        RegId,
    },
    fuel_crypto::{
        coins_bip32::ecdsa::signature::Signer,
        *,
    },
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        AssetId,
        Finalizable,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::{
        Immediate12,
        Immediate18,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
        predicate::EmptyStorage,
    },
};
use indicatif::{
    ProgressBar,
    ProgressStyle,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use test_helpers::builder::{
    TestContext,
    TestSetupBuilder,
};
fn checked_parameters() -> CheckPredicateParams {
    let metadata = SnapshotMetadata::read("./local-testnet").unwrap();
    let chain_conf = ChainConfig::from_snapshot_metadata(&metadata).unwrap();
    chain_conf.consensus_parameters.into()
}
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[clap(short = 'c', long, default_value = "16")]
    pub number_of_cores: usize,
    #[clap(short = 't', long, default_value = "150000")]
    pub number_of_transactions: u64,
}

fn generate_transactions(nb_txs: u64, rng: &mut StdRng) -> Vec<Transaction> {
    let spinner_style =
        ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");
    let pb = ProgressBar::new(nb_txs);
    pb.set_style(spinner_style.clone());
    let mut transactions = Vec::with_capacity(nb_txs as usize);
    for i in 0..nb_txs {
        let ed19_secret = ed25519_dalek::SigningKey::generate(rng);
        let public = ed19_secret.verifying_key();

        let message = b"The gift of words is the gift of deception and illusion.";
        let message = Message::new(message);

        let signature = ed19_secret.sign(&*message).to_bytes();

        let predicate = vec![
            op::gm_args(0x20, GMArgs::GetVerifyingPredicate),
            op::gtf_args(0x20, 0x20, GTFArgs::InputCoinPredicateData),
            op::addi(0x21, 0x20, PublicKey::LEN as Immediate12),
            op::addi(0x22, 0x21, signature.len() as Immediate12),
            op::movi(0x24, message.as_ref().len() as Immediate18),
            op::ed19(0x20, 0x21, 0x22, 0x24),
            op::eq(0x12, RegId::ERR, RegId::ONE),
            op::ret(0x12),
        ]
        .into_iter()
        .collect::<Vec<u8>>();
        let owner = Input::predicate_owner(&predicate);

        let predicate_data: Vec<u8> = public
            .to_bytes()
            .iter()
            .copied()
            .chain(
                signature
                    .iter()
                    .copied()
                    .chain(message.as_ref().iter().copied()),
            )
            .collect();

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                predicate_data.clone(),
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .finalize();
        tx.estimate_predicates(
            &checked_parameters(),
            MemoryInstance::new(),
            &EmptyStorage,
        )
        .expect("Predicate check failed");
        transactions.push(tx.into());
        pb.inc(1);
        pb.set_message(format!("[{}/{}]", i + 1, nb_txs));
    }
    pb.finish_and_clear();
    transactions
}

static TRUCK: Emoji<'_, '_> = Emoji("🚚  ", "");
static CLIP: Emoji<'_, '_> = Emoji("🔗  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");
static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", ":-)");

fn main() {
    let args = Args::parse();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    let spinner_style =
        ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

    println!(
        "{} {}Generating {} transactions...",
        style("[1/4]").bold().dim(),
        TRUCK,
        args.number_of_transactions
    );
    let transactions = generate_transactions(args.number_of_transactions, &mut rng);
    let metadata = SnapshotMetadata::read("./local-testnet").unwrap();
    let chain_conf = ChainConfig::from_snapshot_metadata(&metadata).unwrap();

    println!(
        "{} {}Creating initial coins for transactions inputs...",
        style("[2/4]").bold().dim(),
        CLIP
    );
    let pb = ProgressBar::new(args.number_of_transactions);
    pb.set_style(spinner_style.clone());
    let mut test_builder = TestSetupBuilder::new(2322);
    // setup genesis block with coins that transactions can spend
    // We don't use the function to not have to convert Script to transactions
    test_builder.initial_coins.extend(
        transactions
            .iter()
            .flat_map(|t| t.inputs().unwrap())
            .enumerate()
            .filter_map(|(i, input)| {
                let res = if let Input::CoinSigned(CoinSigned {
                    amount,
                    owner,
                    asset_id,
                    utxo_id,
                    tx_pointer,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    amount,
                    owner,
                    asset_id,
                    utxo_id,
                    tx_pointer,
                    ..
                }) = input
                {
                    Some(CoinConfig {
                        tx_id: *utxo_id.tx_id(),
                        output_index: utxo_id.output_index(),
                        tx_pointer_block_height: tx_pointer.block_height(),
                        tx_pointer_tx_idx: tx_pointer.tx_index(),
                        owner: *owner,
                        amount: *amount,
                        asset_id: *asset_id,
                    })
                } else {
                    None
                };
                pb.inc(1);
                pb.set_message(format!("[{}/{}]", i + 1, args.number_of_transactions));
                res
            }),
    );
    pb.finish_and_clear();

    // disable automated block production
    test_builder.trigger = Trigger::Never;
    test_builder.utxo_validation = true;
    test_builder.gas_limit = Some(
        transactions
            .iter()
            .filter_map(|tx| {
                if tx.is_mint() {
                    return None;
                }
                Some(tx.max_gas(&chain_conf.consensus_parameters).unwrap())
            })
            .sum(),
    );
    test_builder.block_size_limit = Some(1_000_000_000_000_000);
    test_builder.max_txs = transactions.len();
    #[cfg(feature = "parallel-executor")]
    {
        test_builder.number_threads_pool_verif = args.number_of_cores;
        test_builder.executor_number_of_cores = args.number_of_cores;
    }

    // spin up node
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();
    let block = rt.block_on({
        let transactions = transactions.clone();
        let chain_conf = chain_conf.clone();
        let mut test_builder = test_builder.clone();
        let spinner_style = spinner_style.clone();
        async move {
            test_builder.set_chain_config(chain_conf);
            // start the producer node
            let TestContext { srv, client, .. } = test_builder.finalize().await;

            // insert all transactions
            let mut subscriber = srv
                .shared
                .txpool_shared_state
                .new_tx_notification_subscribe();
            let pb = ProgressBar::new(args.number_of_transactions);
            pb.set_style(spinner_style.clone());
            let mut nb_left = args.number_of_transactions;
            println!(
                "{} {}Inserting transactions in the mempool...",
                style("[3/4]").bold().dim(),
                ROCKET
            );
            srv.shared
                .txpool_shared_state
                .try_insert(transactions.clone())
                .unwrap();
            while nb_left > 0 {
                let _ = subscriber.recv().await.unwrap();
                pb.inc(1);
                pb.set_message(format!(
                    "[{}/{}]",
                    args.number_of_transactions - nb_left,
                    args.number_of_transactions
                ));
                nb_left -= 1;
            }
            client.produce_blocks(1, None).await.unwrap();
            let block = srv
                .shared
                .database
                .on_chain()
                .latest_view()
                .unwrap()
                .get_sealed_block_by_height(&1.into())
                .unwrap()
                .unwrap();
            pb.finish_and_clear();
            block
        }
    });

    let duration = rt.block_on(async move {
        test_builder.set_chain_config(chain_conf.clone());
        let TestContext { srv, .. } = test_builder.finalize().await;

        println!(
            "{} {}Execute and commit block...",
            style("[4/4]").bold().dim(),
            SPARKLE
        );
        let pb = ProgressBar::new(args.number_of_transactions);
        pb.set_style(spinner_style.clone());
        let start = std::time::Instant::now();
        srv.shared
            .block_importer
            .execute_and_commit(block)
            .await
            .expect("Should validate the block");
        pb.finish_and_clear();
        println!(
            "{} Block committed in {:?}ms",
            SPARKLE,
            start.elapsed().as_millis()
        );
        start.elapsed()
    });
    display_results(args.number_of_transactions, duration);
}

fn display_results(nb_txs: u64, duration: std::time::Duration) {
    use console::{
        pad_str,
        Alignment,
    };

    let width = 25;
    let text = "                                                                                                                                                                                                                                                                                  
             :=========================:.     =@@@@@@@@@@@@@@@@%. .%@@+            -@@@-  .*@@@@@@@@@@@@@@@@*.  .@@@:                          
             -=========================:.    .*@@@@@@@@@@@@@@@%-  .%@@+            -@@@-  :%@@@@@@@@@@@@@@@*.   .@@@:                          
             -==+@@@@@@@@@@@@+=#@@%+===:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -===+@@@@@@@@@+=#@@@======:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -====*@@@@@@*=*@@@*=======:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -=====#@@@*=*@@@*=========:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -======#*==#%#*===========:.    .*@@@@@@@@@@@@@@@*   .%@@+            -@@@-  :%@@@@@@@@@@@@@@%-    .@@@:                          
             -==+%%%%%%%%+=============:.    .*@@@@@@@@@@@@@@@*   .%@@+            -@@@-  :%@@@@@@@@@@@@@@%-    .@@@:                          
             -==*@@@@@@#===============:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -==*@@@@#=================:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -==*@@%===================:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -==*%+====================:.    .*@@*.               .%@@+            -@@@-  :%@@-                 .@@@:                          
             -========================-.     .*@@*.               .%@@@%%%%%%%%%%%%@@@@-  :%@@@%%%%%%%%%%%%%%.  .@@@@%%%%%%%%%%%%%             
             -======================-.       .*@@*.                +@@@@@@@@@@@@@@@@@@#:  .#@@@@@@@@@@@@@@@@@.   #@@@@@@@@@@@@@@@@             
";
    let text2 = format!("\x1b[92m{}\x1b[39m", text);

    let text2 = pad_str(&text2, width, Alignment::Center, None);

    println!("{text2}");
    let text3 = text_to_ascii_art::to_art(
        format!("{} TXs executed in {} ms", nb_txs, duration.as_millis()),
        "small",
        3,
        0,
        3,
    )
    .unwrap();
    let text4 = text_to_ascii_art::to_art(
        format!("{} TPS", (nb_txs as u128 / duration.as_millis()) * 1000),
        "default",
        10,
        1,
        10,
    )
    .unwrap();
    println!("\x1b[92m{text3}\x1b[39m");
    println!("\x1b[92m{text4}\x1b[39m");
}
