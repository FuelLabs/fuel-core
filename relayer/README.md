# Relayer


Use ethers-rs to ferch data and be consistent what exactly we are getting. We need to be sure that we are receiving/reading all logs from out contracts that we are watching.

There is possitibility to subscribe to incomming `blocks` and `logs`: https://geth.ethereum.org/docs/rpc/pubsub that we can use as main way of communication. 
Tricky thing that can happen not so often is initial synchronization, when do we start subscription and when do we stop importing blocks.


We will need to connect with ethereum node provider or client to have open API.

Functionality of relayer:
 validator staking events
 asset deposits / withdrawals
 block publishing




Block related:
    * Receiving block from contract
    * Validating received block
    * pushing challenge if needed
    * Publishing new block*

Validator related:
    * Handle deposit. Send this info to ValidatorList use it in consensus
    * Handle withdrawal. Send this info to ValidatorList use it in consensus.

Bridge related:
    * Deposit asset
    * Withdwrap asset



Handle BlockComitted and send it to consensus:


events from fuel-v2-contract
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
