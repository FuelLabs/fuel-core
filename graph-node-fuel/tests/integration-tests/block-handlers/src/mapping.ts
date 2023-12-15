import { Address, BigInt, ethereum, log } from '@graphprotocol/graph-ts';
import { Contract, Trigger } from '../generated/Contract/Contract';
import {
  BlockFromOtherPollingHandler,
  BlockFromPollingHandler,
  Block,
  Foo,
  Initialize,
} from '../generated/schema';
import { ContractTemplate } from '../generated/templates';

export function handleBlock(block: ethereum.Block): void {
  log.info('handleBlock {}', [block.number.toString()]);
  let blockEntity = new Block(block.number.toString());
  blockEntity.number = block.number;
  blockEntity.hash = block.hash;
  blockEntity.save();

  if (block.number == BigInt.fromI32(2)) {
    ContractTemplate.create(
      Address.fromString('0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48')
    );
  }
}

export function handleBlockPolling(block: ethereum.Block): void {
  log.info('handleBlockPolling {}', [block.number.toString()]);
  let blockEntity = new BlockFromPollingHandler(block.number.toString());
  blockEntity.number = block.number;
  blockEntity.hash = block.hash;
  blockEntity.save();
}

export function handleBlockPollingFromTemplate(block: ethereum.Block): void {
  log.info('===> handleBlockPollingFromTemplate {}', [block.number.toString()]);
  let blockEntity = new BlockFromOtherPollingHandler(block.number.toString());
  blockEntity.number = block.number;
  blockEntity.hash = block.hash;
  blockEntity.save();
}

export function handleTrigger(event: Trigger): void {
  // We set the value to 0 to test that the subgraph
  // runs initialization handler before all other handlers
  if (event.params.x == 1) {
    let entity = Foo.load('initialize');

    // If the intialization handler is called first
    // this would set the value to -1 for Foo entity with id 0
    // If it is not called first then the value would be 0
    if (entity != null) {
      entity.value = -1;
      entity.id = 'initialize';
      entity.save();
    }
  }

  let obj = new Foo(event.params.x.toString());
  obj.id = event.params.x.toString();
  obj.value = event.params.x as i64;

  obj.save();
}

export function initialize(block: ethereum.Block): void {
  log.info('initialize called at block', [block.number.toString()]);
  let entity = new Initialize(block.number.toString());
  entity.id = block.number.toString();
  entity.block = block.number;
  entity.save();

  // If initialization handler is called then this would set
  // the value to 0 for Foo entity with id 0
  // This is to test that initialization handler is called
  // before all other handlers
  let foo = new Foo('initialize');
  foo.id = 'initialize';
  foo.value = 0;
  foo.save();
}
