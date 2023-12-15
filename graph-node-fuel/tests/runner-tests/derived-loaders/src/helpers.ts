import { Bytes, log } from "@graphprotocol/graph-ts";
import {
  BBar,
  BBarTestResult,
  Bar,
  BarTestResult,
  TestResult,
} from "../generated/schema";

/**
 * Asserts that two `Bar` instances are equal.
 */
export function assertBarsEqual(a: Bar, b: Bar): void {
  assert(
    a.id == b.id &&
      a.value == b.value &&
      a.value2 == b.value2 &&
      a.fooValue == b.fooValue,
    "Bar instances are not equal"
  );
}

/**
 * Asserts that two `BBar` instances are equal.
 */
export function assertBBarsEqual(
  a: BBar,
  b: BBar,
  message: string = "BBar instances are not equal"
): void {
  assert(
    a.id.toHex() == b.id.toHex() &&
      a.value == b.value &&
      a.value2 == b.value2 &&
      a.fooValue.toHex() == b.fooValue.toHex(),
    message
  );
}

/**
 * Creates a new `Bar` entity and saves it.
 */
export function createBar(
  id: string,
  fooValue: string,
  value: i64,
  value2: i64
): Bar {
  let bar = new Bar(id);
  bar.fooValue = fooValue;
  bar.value = value;
  bar.value2 = value2;
  bar.save();
  return bar;
}

/**
 * Creates a new `BBar` entity and saves it.
 */
export function createBBar(
  id: Bytes,
  fooValue: Bytes,
  value: i64,
  value2: i64
): BBar {
  let bBar = new BBar(id);
  bBar.fooValue = fooValue;
  bBar.value = value;
  bBar.value2 = value2;
  bBar.save();
  return bBar;
}

/**
 * A function to loop over an array of `Bar` instances and assert that the values are equal.
 */
export function assertBarsArrayEqual(bars: Bar[], expected: Bar[]): void {
  assert(bars.length == expected.length, "bars.length != expected.length");
  for (let i = 0; i < bars.length; i++) {
    assertBarsEqual(bars[i], expected[i]);
  }
}

/**
 * A function to loop over an array of `BBar` instances and assert that the values are equal.
 */
export function assertBBarsArrayEqual(bBars: BBar[], expected: BBar[]): void {
  assert(bBars.length == expected.length, "bBars.length != expected.length");
  for (let i = 0; i < bBars.length; i++) {
    assertBBarsEqual(bBars[i], expected[i]);
  }
}

export function convertBarToBarTestResult(
  barInstance: Bar,
  testId: string
): BarTestResult {
  const barTestResult = new BarTestResult(barInstance.id + "_" + testId);

  barTestResult.value = barInstance.value;
  barTestResult.value2 = barInstance.value2;
  barTestResult.fooValue = barInstance.fooValue;
  barTestResult.save();

  return barTestResult;
}

export function convertbBarToBBarTestResult(
  bBarInstance: BBar,
  testId: string
): BBarTestResult {
  const bBarTestResult = new BBarTestResult(
    Bytes.fromUTF8(bBarInstance.id.toString() + "_" + testId)
  );

  bBarTestResult.value = bBarInstance.value;
  bBarTestResult.value2 = bBarInstance.value2;
  bBarTestResult.fooValue = bBarInstance.fooValue;
  bBarTestResult.save();

  return bBarTestResult;
}

// convertBarArrayToBarTestResultArray
export function saveBarsToTestResult(
  barArray: Bar[],
  testResult: TestResult,
  testID: string
): void {
  let result: string[] = [];
  for (let i = 0; i < barArray.length; i++) {
    result.push(convertBarToBarTestResult(barArray[i], testID).id);
  }
  testResult.barDerived = result;
}

// convertBBarArrayToBBarTestResultArray
export function saveBBarsToTestResult(
  bBarArray: BBar[],
  testResult: TestResult,
  testID: string
): void {
  let result: Bytes[] = [];
  for (let i = 0; i < bBarArray.length; i++) {
    result.push(convertbBarToBBarTestResult(bBarArray[i], testID).id);
  }
  testResult.bBarDerived = result;
}

export function logTestResult(testResult: TestResult): void {
  log.info("TestResult with ID: {} has barDerived: {} and bBarDerived: {}", [
    testResult.id,
    testResult.barDerived ? testResult.barDerived!.join(", ") : "null",
    testResult.bBarDerived
      ? testResult
          .bBarDerived!.map<string>((b) => b.toHex())
          .join(", ")
      : "null",
  ]);
}
