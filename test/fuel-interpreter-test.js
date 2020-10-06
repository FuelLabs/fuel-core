const { expect } = require("chai");

describe("FuelInterpreter", function() {
  it("Should return the new FuelInterpreter once it's initialized", async function() {
    const FuelInterpreter = await ethers.getContractFactory("FuelInterpreter");
    const interpreter = await FuelInterpreter.deploy();

    await interpreter.deployed();
    await interpreter.init([1, 2]);

    expect(await interpreter.getRegisterCount()).to.equal(2);
  });
});