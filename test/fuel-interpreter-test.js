const { expect } = require("chai");

describe("FuelInterpreter", function() {
  it("Should return the new FuelInterpreter once it's initialized", async function() {
    const FuelInterpreter = await ethers.getContractFactory("FuelInterpreter");
    const interpreter = await FuelInterpreter.deploy();

    await interpreter.deployed();
    await interpreter.init([]);
    expect(await interpreter.getRegisterCount()).to.equal(0);

    await interpreter.init([1, 2]);
    expect(await interpreter.getRegisterCount()).to.equal(2);
  });
});

describe("FuelInterpreter", function() {
  it("Should run initial opcode test", async function() {
    const FuelInterpreter = await ethers.getContractFactory("FuelInterpreter");

    const interpreter = await FuelInterpreter.deploy();

    await interpreter.deployed();
    await interpreter.init([]);
    expect(await interpreter.getRegisterCount()).to.equal(0);

    const FuelEnums = await ethers.getContractFactory("Opcode_Contract");
    const enums = await FuelEnums.deploy();
    await enums.deployTransaction.wait();

    let op_stop = await enums.OP_STOP();
    console.log("op_stop:",op_stop);
    expect(op_stop).to.equal(0); // STOP

    let op_add = await enums.OP_ADD();
    console.log("op_add:",op_add);
    expect(op_add).to.equal(1); // ADD

  });
});