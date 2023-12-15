import {
  ethereum,
  dataSource,
  BigInt,
  Bytes,
  DataSourceContext,
} from "@graphprotocol/graph-ts";
import { TestEvent } from "../generated/Contract/Contract";
import { IpfsFile, IpfsFile1, SpawnTestEntity } from "../generated/schema";

// CID of `file-data-sources/abis/Contract.abi` after being processed by graph-cli.
const KNOWN_CID = "QmQ2REmceVtzawp7yrnxLQXgNNCtFHEnig6fL9aqE1kcWq";

export function handleBlock(block: ethereum.Block): void {
  let entity = new IpfsFile("onchain");
  entity.content = "onchain";
  entity.save();

  // This will create the same data source twice, once at block 0 and another at block 2.
  // The creation at block 2 should be detected as a duplicate and therefore a noop.
  if (block.number == BigInt.fromI32(0) || block.number == BigInt.fromI32(2)) {
    dataSource.create("File", [KNOWN_CID]);
  }

  if (block.number == BigInt.fromI32(1)) {
    let entity = IpfsFile.load("onchain")!;
    assert(entity.content == "onchain");

    // The test assumes file data sources are processed in the block in which they are created.
    // So the ds created at block 0 will have been processed.
    //
    // Test that onchain data sources cannot read offchain data.
    assert(IpfsFile.load(KNOWN_CID) == null);

    // Test that using an invalid CID will be ignored
    dataSource.create("File", ["hi, I'm not valid"]);
  }

  // This will invoke File1 data source with same CID, which will be used
  // to test whether same cid is triggered across different data source.
  if (block.number == BigInt.fromI32(3)) {
    // Test that onchain data sources cannot read offchain data (again, but this time more likely to hit the DB than the write queue).
    assert(IpfsFile.load(KNOWN_CID) == null);

    dataSource.create("File1", [KNOWN_CID]);
  }
}

export function handleTestEvent(event: TestEvent): void {
  let command = event.params.testCommand;

  if (command == "createFile2") {
    // Will fail the subgraph when processed due to mismatch in the entity type and 'entities'.
    dataSource.create("File2", [KNOWN_CID]);
  } else if (command == "saveConflictingEntity") {
    // Will fail the subgraph because the same entity has been created in a file data source.
    let entity = new IpfsFile(KNOWN_CID);
    entity.content = "empty";
    entity.save();
  } else if (command == "createFile1") {
    // Will fail the subgraph with a conflict between two entities created by offchain data sources.
    let context = new DataSourceContext();
    context.setBytes("hash", event.block.hash);
    dataSource.createWithContext("File1", [KNOWN_CID], context);
  } else if (command == "spawnOffChainHandlerTest") {
    // Used to test the spawning of a file data source from another file data source handler.
    // `SpawnTestHandler` will spawn a file data source that will be handled by `spawnOffChainHandlerTest`,
    // which creates another file data source `OffChainDataSource`, which will be handled by `handleSpawnedTest`.
    let context = new DataSourceContext();
    context.setString("command", command);
    dataSource.createWithContext("SpawnTestHandler", [KNOWN_CID], context);
  } else if (command == "spawnOnChainHandlerTest") {
    // Used to test the failure of spawning of on-chain data source from a file data source handler.
    // `SpawnTestHandler` will spawn a file data source that will be handled by `spawnTestHandler`,
    // which creates an `OnChainDataSource`, which should fail since spawning onchain datasources
    // from offchain handlers is not allowed.
    let context = new DataSourceContext();
    context.setString("command", command);
    dataSource.createWithContext("SpawnTestHandler", [KNOWN_CID], context);
  } else {
    assert(false, "Unknown command: " + command);
  }
}

export function handleFile(data: Bytes): void {
  // Test that offchain data sources cannot read onchain data.
  assert(IpfsFile.load("onchain") == null);

  if (
    dataSource.stringParam() != "QmVkvoPGi9jvvuxsHDVJDgzPEzagBaWSZRYoRDzU244HjZ"
  ) {
    // Test that an offchain data source cannot read from another offchain data source.
    assert(
      IpfsFile.load("QmVkvoPGi9jvvuxsHDVJDgzPEzagBaWSZRYoRDzU244HjZ") == null
    );
  }

  let entity = new IpfsFile(dataSource.stringParam());
  entity.content = data.toString();
  entity.save();

  // Test that an offchain data source can load its own entities
  let loaded_entity = IpfsFile.load(dataSource.stringParam())!;
  assert(loaded_entity.content == entity.content);
}

export function handleFile1(data: Bytes): void {
  let entity = new IpfsFile1(dataSource.stringParam());
  entity.content = data.toString();
  entity.save();
}

// Used to test spawning a file data source from another file data source handler.
// This function spawns a file data source that will be handled by `handleSpawnedTest`.
export function spawnTestHandler(data: Bytes): void {
  let context = new DataSourceContext();
  context.setString("file", "fromSpawnTestHandler");
  let command = dataSource.context().getString("command");
  if (command == "spawnOffChainHandlerTest") {
    dataSource.createWithContext("OffChainDataSource", [KNOWN_CID], context);
  } else if (command == "spawnOnChainHandlerTest") {
    dataSource.createWithContext("OnChainDataSource", [KNOWN_CID], context);
  }
}

// This is the handler for the data source spawned by `spawnOffChainHandlerTest`.
export function handleSpawnedTest(data: Bytes): void {
  let entity = new SpawnTestEntity(dataSource.stringParam());
  let context = dataSource.context().getString("file");
  entity.content = data.toString();
  entity.context = context;
  entity.save();
}
