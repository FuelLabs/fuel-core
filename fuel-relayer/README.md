

# Relayer


We will need to connect with ethereum node provider or client to have open API.

Functionalities expected from Relayer:

* Validator staking:
    * Handle deposit. Handle ValidatorList use it in consensus
    * Handle withdrawal. Handle ValidatorList use it in consensus.
* Bridge:
    * Deposit asset
    * Withdwrap asset
* Block related (mostly skipped for first version):
    * Publishing new block
    * Calculating new ValidatorSet.
    * Receiving block from contract (Not impl)
    * Validating received block (Not impl)
    * pushing challenge if needed (Not impl)

## Validity and finality

With ethereums The Merge comming in few months we are gaining finality of blocks in ethereum after two epochs, epoch contains 32 slots, slot takes 12s so epoch is ~6,4min and two epocs are ~12.8min, so after 13min we are sure that our deposits will not be reverted by some big reorganization. So we are okay to say that everything older then ~100blocks are finalized.

Second finality that we have is related to fuel block atestation timelimit, how long are we going to wait until challenge comes. It should be at least longer than ethereum finality. Not relavent for first version

### Validator related stake:
Validator stake is deposited on Ethereum contract side,

Problem1: Vilidator deposit to ethereum gets reverted by block reorg. (Eth clients usually have priority for reverted txs but this does not mean it cant happen)
Solution: Introduce sliding window, only deposits that are at least eth finality long can be included in validators leader selection.

Example of sliding window:
![Sliding Window](../docs/diagrams/fuel_v2_validator_sliding_window.jpg)


Problem2: Validator before its eth finality passes, withdraws its stake. We have sliding windows that is in past but if we allow withdrawal immediately there will be no stake to take if slashing happens in present.
Solution: Withdrawal should be two step process, first step is WithdrawalIntention that will lock funds for Withdrawal and wait for eth finality time to kick in. After WithdrawalIntention gets first eth finalize, stake is removed from Fuel Validation Leader selection. Additional time that needs to be added to WithdrawalIntention is time for challange, only after those two passes we are safe to unlock withdrawal. After this time passes user can withdraw its stake.

### Bridge

Bridge has functionality to connect ethereum ERC-20 tokens with Fuel network and allow transfer of token between them. Deposit and withdrawal are lot simpler in comparising to staking.

Deposit transfers token into contract and after eth finalization passes it can be used inside Fuel network.

Problem: How would we specify Deposit as Input to be used by Tx, if it is UtxoId we can use TxId (index_output as zero) but how would this integrate with database.
Maybe solution: Introduce DepositInput it feels cleaner?

For Withdrawal, any Tx can have OutputWithdrawal and that information is send inside a fuel block to eth contract and token is set for withdrawal.

Problem: How long does finalization of fuel block takes. It probably depends on challenge timeframe. Only when block is finalized than we are safe to do withdrawal without thinking of slashing or reverting of fuel blocks. Should withdrawal wait for eth finality to come and have someting like PendingWithdrawal? 

Note: token is solidity contract means ethereum ERC-20 address

## Implementation:

### Sync flow

Initial sync:
1. sync from HardCoddedContractCreatingBlock->BestEthBlock-100)
2. sync overlap from LastIncludedEthBlock-> BestEthBlock) they are saved in dequeue.
3. Start listening to eth events
4. Check if our LastIncludedEthBlock is same as BestEthBlock and that we didn't receive new events.
  If not the same, stop listening to events and do 2,3,4 steps again.
7. Continue to active listen on eth events. and prune(commit to db) dequeue for older finalized events

On activ listening of eth events:
1. Receive event:
    1. if it is removal, append them into pending_removal vec
    2. if it is new event. Apply pending_removal_vec and add new event to dequeue.
2. On new fuel block, commit changed to db.
     1. just to be sure introduce check of events by calling eth block logs. This is a double check just to be sure
         that we match with that is inside contract.

/ To have validity of ValidatorSet we need to have logs from ethereum to reconstruct the set.
/ So It does not matter for us if we start syncing to fuel network if we are not connected to eth client.

WIP

Think about relayer as receiving list of logs from ethereum side with possibility of some events getting

Propagating of block is simplest thing, whatever consensus give us FuelBlock transfer it into PackedFuelBlock and send it to contract. Validation and challange comes later.

For fetching logs from contract use ethers-rs to fetch data and be consistent what exactly we are getting. We need to be sure that we are receiving/reading all logs from out contracts that we are watching. That means having index what we have read.

There is possibility to subscribe with websocket to incomming `blocks` and `logs`: https://geth.ethereum.org/docs/rpc/pubsub that we can use as main way of communication.
Tricky thing that can happen not so often is initial synchronization, when do we start subscription and when do we stop importing blocks.

Log data from log subscription:
* "address":0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2,
* "topics":[0x00,0x00,0x00],
* "data":"Bytes",
* "block_hash":Some(Hash)),
* "block_number":Some(14069822),
* "transaction_hash":Some(0xcf7baaa6e6cd2d863807a3b8d3168f8d2699ad043e3ac249e82887fda6c4ef55),
* "transaction_index":Some(130),
* "log_index":Some(172),
* "transaction_log_index":"None",
* "log_type":"None",
* "removed":"Some(false)"

TODO!

We need to have both reverts and additional of logs

Structures:
* FuelBlock number tied with EthBlockNumber. It is assumed that all event from same block are processed?
    * Logs subscription return a lot of additional data that will halp us to sort them
* Pending validator stake is from


## events from fuel-v2-contract

* ERC20
    * Approval
    * Transfer
* IERC20
    * Approval
    * Transfer
* Token
    * Approval
    * Transfer
* fuel.json
    * BlockCommitted 
* DepositHandler
    * DepositMade
* WithdrawalHandler
    * WithdrawalMade 
* LeaderSelection:
    * NewRound
    * Submission
    * Deposit
    * Withdrawal
* PerpetualBurnAuction
    * AuctionEnd
    * Bid
* DSAuth
    * LogSetAuthority
    * LogSetOwner
* DSAuthEvents
    * LogSetAuthority
    * LogSetOwner
* DSGuard
    * LogForbid
    * LogPermit
    * LogSetAuthority
    * LogSetOwner
* DSGuardEvents
    * LogForbid
    * LogPermit
* DSToken
    * Approval
    * Burn
    * LogSetAuthority
    * LogSetOwner
    * Mint
    * Start
    * Stop
    * Transfer
