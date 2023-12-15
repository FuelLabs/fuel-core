import {
  ethereum,
  dataSource,
  BigInt,
  Bytes,
} from "@graphprotocol/graph-ts";
import { File } from "../generated/schema";

const KNOWN_HASH = "8APeQ5lW0-csTcBaGdPBDLAL2ci2AT9pTn2tppGPU_8";

export function handleBlock(block: ethereum.Block): void {
  if (block.number == BigInt.fromI32(0)) {
    dataSource.create("File", [KNOWN_HASH]);
  }
}

export function handleFile(data: Bytes): void {
  let entity = new File(dataSource.stringParam());
  entity.content = data.toString();
  entity.save();

  // Test that an offchain data source can load its own entities
  let loaded_entity = File.load(dataSource.stringParam())!;
  assert(loaded_entity.content == entity.content);
}

