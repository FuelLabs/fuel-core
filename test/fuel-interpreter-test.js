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

  it("Should validate access to opcodes test", async function() {
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

  it("Should run interpreter and return status test", async function() {
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

    const InterpreterResult = await ethers.getContractFactory("interpreter_result_Contract");
    const interpreterResult = await InterpreterResult.deploy();
    let tx_res0 = await interpreter.getLastResult();
    let initial = await interpreterResult.INITIAL();
    console.log("initial:", initial);
    expect(initial).to.equal(tx_res0);

    let tx_process1 = await interpreter.processRegister([op_stop],[]);
    let tx_res = await interpreter.getLastResult();
    let stopped = await interpreterResult.STOPPED();
    console.log("stopped:", stopped);
    expect(stopped).to.equal(tx_res);

  });
});