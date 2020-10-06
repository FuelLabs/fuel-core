pragma solidity ^0.6.8;

import "@nomiclabs/buidler/console.sol";
import "./FuelEnums.sol";

contract FuelInterpreter {

    bytes32[] register;

    // initialize stack: a stack is common in register-based VM approaches
    uint256[] stackStorage;

    constructor(bytes32[] memory _initialRegister) public {
        console.log("Deploying a FuelInterpreter with registers:", _initialRegister.length);
        register = _initialRegister;
    }

    function processRegister(Opcode[] calldata codes, uint256[] calldata extra) external returns (interpret_result result) {
//        uint256 pc = 0;
        // program counter for bytecode
        uint256 pc_extra = 0;
        // program counter for data

        // start going through bytecode
        for (uint i = 0; i < codes.length; i++) {
            Opcode current = codes[i];

            // set default value to failure
            // method return of 'result' can be invoked eagerly based on VM semantics config
            result = interpret_result.ERROR_UNKNOWN_OPCODE;

            // we try to produce results to register[0]
            if (current == Opcode.OP_STOP) {
              result = result = interpret_result.STOPPED;
                // presumably, return code can include register and stack values, e.g. hash
                // this is a starting point for recursive covenant approaches
            } else if (current == Opcode.OP_INC) {
                uint256 tmp = uint(register[0]);
                register[0] = bytes32(tmp++);
            } else if (current == Opcode.OP_DEC) {
                uint256 tmp = uint(register[0]);
                register[0] = bytes32(tmp--);
            } else if (current == Opcode.OP_VERIFY_MERKLE_PROOF) {
                // WIP
                // generally preceded by a push3 of:
                // 1. merkle root
                // 2. branch length
                // 3. type identifier: 0 bytecode for fuel vm, 1 ethereum smart contract
                // for script execution at leaf (upon successful branch proof)
                // define-check: code[pc-1] == Opcode.PUSH3

                // given: root of Merkle Tree
                // root is supposed to be embedded in smart contract
                // not in this specific version
//                uint256 op_vmp_root = uint256(register[0]);

                // branch is given as sequences of hashes
                // indicate branch length from calldata
//                uint256 branchLength = uint256(register[1]);

                // check proof
                // TODO - check proof

                // execute script to execute at leaf
                // or execute smart contract
                // option is specified through register
            } else if (current == Opcode.OP_CHECK_TEMPLATE_VERIFY) {
                // we can apply covenants using our interpreter, even recursively

                // get template mask length
                uint256 maskLength = uint256(register[0]);

                // the following lines of code are shown simply for explaining semantics,
                // the implementation is optimal without these 2 lines
                // * get mask
                // uint256[] mask = extra[pc_extra:pc_extra+maskLength];
                // pc_extra += maskLength;
                // * get template
                // uint256[] template = extra[pc_extra:pc_extra+maskLength];
                // pc_extra += maskLength;
                // * get output bytecode
                // uint256[] output = extract[pc_extra:pc_extra+maskLength];
                // pc_extra += maskLength;

                for (uint m = 0; m < maskLength; m++) {
                    uint256 mask = extra[pc_extra];
                    if (mask == 0) {
                        pc_extra++;
                        continue;
                    }
                    uint256 template = extra[pc_extra + maskLength];
                    uint256 output = extra[pc_extra + maskLength * 2];
                    if (template != output) {
                        // record the template failure offset of extra data
                        register[0] = bytes32(pc_extra);
                        result = interpret_result.ERROR_TEMPLATE_FAILURE;
                        return result;
                    }
                    pc_extra++;
                }

                // check and return result
                // record 0 as check result indicating success
                // it's okay for 0 to be shared in error and success domains
                // since the result status is not an error here
                register[0] = 0;
            } else if (current == Opcode.OP_EXECUTE_ETH_SMART_CONTRACT) {
                // TODO - implement OP_EXECUTE_ETH_SMART_CONTRACT
            } else {
                // set error return
                result = interpret_result.ERROR_UNKNOWN_OPCODE;
                return result;
            }
        }

        // if completed successfully, set success status
        result = interpret_result.SUCCESS;
    }



}

// security note:
// - incrementing of pc and pc_extra needs to be validated in some cases