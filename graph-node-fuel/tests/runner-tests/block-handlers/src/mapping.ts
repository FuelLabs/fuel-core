import { ethereum, log } from "@graphprotocol/graph-ts";
import { Contract, TestEvent } from "../generated/Contract/Contract";
import { ContractTemplate } from "../generated/templates";
import {
  BlockFromOtherPollingHandler,
  BlockFromPollingHandler,
} from "../generated/schema";

export function handleBlockPolling(block: ethereum.Block): void {
  log.info("===> handleBlockPolling {}", [block.number.toString()]);
  let blockEntity = new BlockFromPollingHandler(block.number.toString());
  blockEntity.number = block.number;
  blockEntity.hash = block.hash;
  blockEntity.save();
}

export function handleBlockPollingFromTemplate(block: ethereum.Block): void {
  log.info("===> handleBlockPollingFromTemplate {}", [block.number.toString()]);
  let blockEntity = new BlockFromOtherPollingHandler(block.number.toString());
  blockEntity.number = block.number;
  blockEntity.hash = block.hash;
  blockEntity.save();
}

export function handleCommand(event: TestEvent): void {
  let command = event.params.testCommand;

  if (command == "hello_world") {
    log.info("Hello World!", []);
  } else if (command == "create_template") {
    log.info("===> Creating template {}", [event.address.toHexString()]);
    ContractTemplate.create(event.address);
  }
}
