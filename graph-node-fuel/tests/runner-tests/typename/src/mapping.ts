import { ethereum, BigInt } from "@graphprotocol/graph-ts";
import { ExampleEntity } from "../generated/schema";

export function handleBlock(block: ethereum.Block): void {
  // At Block 0, we create the Entity.
  if (block.number == BigInt.fromI32(0)) {
    let example = new ExampleEntity("1234");
    example.save();
    assert(example.get("__typename") == null, "__typename should be null");
  // At Block 2, we merge the previously created Entity.
  // Obs: Block 1 there was a reorg, we do nothing and wait the Entity cache to clear.
  } else if (block.number == BigInt.fromI32(2)) {
    let example = new ExampleEntity("1234");
    example.save();
    assert(example.get("__typename") == null, "__typename should still be null");
  // At Block 3, we load the merged Entity, which should NOT bring
  // the __typename field.
  } else if (block.number == BigInt.fromI32(3)) {
    let example = ExampleEntity.load("1234")!;
    assert(example.get("__typename") == null, "__typename should still be null");
  }
}
