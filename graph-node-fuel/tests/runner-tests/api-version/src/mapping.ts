import { Entity, Value, store } from "@graphprotocol/graph-ts";
import { TestEvent } from "../generated/Contract/Contract";
import { TestResult } from "../generated/schema";

export function handleTestEvent(event: TestEvent): void {
  let testResult = new TestResult(event.params.testCommand);
  testResult.message = event.params.testCommand;
  let testResultEntity = testResult as Entity;
  testResultEntity.set(
    "invalid_field",
    Value.fromString("This is an invalid field"),
  );
  store.set("TestResult", testResult.id, testResult);
  testResult.save();
}
