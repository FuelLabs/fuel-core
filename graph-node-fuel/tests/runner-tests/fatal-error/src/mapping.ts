import { ethereum } from "@graphprotocol/graph-ts";

export function handleBlock(block: ethereum.Block): void {
  if (block.number.toI32() == 3) {
    assert(false)
  }
}
