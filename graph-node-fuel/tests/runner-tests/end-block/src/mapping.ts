import {
  ethereum,
} from '@graphprotocol/graph-ts';
import { TestEvent } from '../generated/Contract/Contract';
import { Block, TestEventEntity } from '../generated/schema';

export function handleBlock(block: ethereum.Block): void {
  let entity = new Block(block.number.toHex());
  entity.number = block.number;
  entity.hash = block.hash.toHexString();
  entity.save();
}

export function handleTestEvent(event: TestEvent): void {
  let command = event.params.testCommand;
  let entity = new TestEventEntity(
    event.transaction.hash.toHex() + "-" + event.logIndex.toString(),
  );
  entity.blockNumber = event.block.number;
  entity.blockHash = event.block.hash.toHexString();
  entity.command = command;
  entity.save();
}
