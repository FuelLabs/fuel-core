import { Bytes, store } from "@graphprotocol/graph-ts";
import { TestEvent } from "../generated/Contract/Contract";
import { Bar, Foo, BFoo, BBar, TestResult } from "../generated/schema";
import {
  assertBBarsArrayEqual,
  assertBBarsEqual,
  assertBarsArrayEqual,
  assertBarsEqual,
  createBBar,
  createBar,
  saveBBarsToTestResult,
  saveBarsToTestResult,
  logTestResult,
} from "./helpers";

export function handleTestEvent(event: TestEvent): void {
  let testResult = new TestResult(event.params.testCommand);
  handleTestEventForID(event, testResult);
  handleTestEventForBytesAsIDs(event, testResult);
  logTestResult(testResult);
  testResult.save();
}

function handleTestEventForID(event: TestEvent, testResult: TestResult): void {
  // This test is to check that the derived entities are loaded correctly
  // in the case where the derived entities are created in the same handler call
  // ie: updates are coming from `entity_cache.handler_updates`
  if (event.params.testCommand == "1_0") {
    let foo = new Foo("0");
    foo.value = 0;
    foo.save();

    let bar = createBar("0", "0", 0, 0);
    let bar1 = createBar("1", "0", 0, 0);
    let bar2 = createBar("2", "0", 0, 0);

    let fooLoaded = Foo.load("0");
    let barDerived = fooLoaded!.bar.load();

    saveBarsToTestResult(barDerived, testResult, event.params.testCommand);

    // bar0, bar1, bar2 should be loaded
    assertBarsArrayEqual(barDerived, [bar, bar1, bar2]);
  }

  // This test is to check that the derived entities are loaded correctly
  // in the case where the derived entities are created in the same block
  // ie: updates are coming from `entity_cache.updates`
  if (event.params.testCommand == "1_1") {
    let fooLoaded = Foo.load("0");

    let barLoaded = Bar.load("0");
    barLoaded!.value = 1;
    barLoaded!.save();

    // remove bar1 to test that it is not loaded
    // This tests the case where the entity is present in `entity_cache.updates` but is removed by
    // An update from `entity_cache.handler_updates`
    store.remove("Bar", "1");

    let barDerivedLoaded = fooLoaded!.bar.load();
    saveBarsToTestResult(
      barDerivedLoaded,
      testResult,
      event.params.testCommand
    );

    // bar1 should not be loaded as it was removed
    assert(barDerivedLoaded.length == 2, "barDerivedLoaded.length != 2");
    // bar0 should be loaded with the updated value
    assertBarsEqual(barDerivedLoaded[0], barLoaded!);
  }

  if (event.params.testCommand == "2_0") {
    let fooLoaded = Foo.load("0");
    let barDerived = fooLoaded!.bar.load();

    // update bar0
    // This tests the case where the entity is present in `store` but is updated by
    // An update from `entity_cache.handler_updates`
    let barLoaded = Bar.load("0");
    barLoaded!.value = 2;
    barLoaded!.save();

    // remove bar2 to test that it is not loaded
    // This tests the case where the entity is present in store but is removed by
    // An update from `entity_cache.handler_updates`
    store.remove("Bar", "2");

    let barDerivedLoaded = fooLoaded!.bar.load();
    assert(barDerivedLoaded.length == 1, "barDerivedLoaded.length != 1");
    // bar0 should be loaded with the updated value
    assertBarsEqual(barDerivedLoaded[0], barLoaded!);

    saveBarsToTestResult(
      barDerivedLoaded,
      testResult,
      event.params.testCommand
    );
  }
}

// Same as handleTestEventForID but uses Bytes as IDs
function handleTestEventForBytesAsIDs(
  event: TestEvent,
  testResult: TestResult
): void {
  if (event.params.testCommand == "1_0") {
    let bFoo = new BFoo(Bytes.fromUTF8("0"));
    bFoo.value = 0;
    bFoo.save();

    let bBar = createBBar(Bytes.fromUTF8("0"), Bytes.fromUTF8("0"), 0, 0);
    let bBar1 = createBBar(Bytes.fromUTF8("1"), Bytes.fromUTF8("0"), 0, 0);
    let bBar2 = createBBar(Bytes.fromUTF8("2"), Bytes.fromUTF8("0"), 0, 0);

    let bFooLoaded = BFoo.load(Bytes.fromUTF8("0"));
    let bBarDerived: BBar[] = bFooLoaded!.bar.load();

    saveBBarsToTestResult(bBarDerived, testResult, event.params.testCommand);

    assertBBarsArrayEqual(bBarDerived, [bBar, bBar1, bBar2]);
  }

  if (event.params.testCommand == "1_1") {
    let bFooLoaded = BFoo.load(Bytes.fromUTF8("0"));
    let bBarDerived = bFooLoaded!.bar.load();

    let bBarLoaded = BBar.load(Bytes.fromUTF8("0"));
    bBarLoaded!.value = 1;
    bBarLoaded!.save();

    store.remove("BBar", Bytes.fromUTF8("1").toHex());

    let bBarDerivedLoaded = bFooLoaded!.bar.load();
    saveBBarsToTestResult(
      bBarDerivedLoaded,
      testResult,
      event.params.testCommand
    );

    assert(bBarDerivedLoaded.length == 2, "bBarDerivedLoaded.length != 2");
    assertBBarsEqual(bBarDerivedLoaded[0], bBarLoaded!);
  }

  if (event.params.testCommand == "2_0") {
    let bFooLoaded = BFoo.load(Bytes.fromUTF8("0"));
    let bBarDerived = bFooLoaded!.bar.load();

    let bBarLoaded = BBar.load(Bytes.fromUTF8("0"));
    bBarLoaded!.value = 2;
    bBarLoaded!.save();

    store.remove("BBar", Bytes.fromUTF8("2").toHex());

    let bBarDerivedLoaded = bFooLoaded!.bar.load();
    saveBBarsToTestResult(
      bBarDerivedLoaded,
      testResult,
      event.params.testCommand
    );

    assert(bBarDerivedLoaded.length == 1, "bBarDerivedLoaded.length != 1");
    assertBBarsEqual(bBarDerivedLoaded[0], bBarLoaded!);
  }
}