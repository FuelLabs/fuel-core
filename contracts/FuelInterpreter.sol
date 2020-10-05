pragma solidity ^0.6.8;

import "./FuelEnums.sol";

contract FuelInterpreter {

    // initialize registers
    uint256[3] register; // 3 registers to start

    // initialize stacks:
    uint256[10] stackStorage;

    function initializeRegisters(uint256[3] calldata params) external {
        register[0] = params[0];
        // copy objects from calldata to storage
        register[1] = params[1];
        register[2] = params[2];
    }

    // bytecode with statically (type-)checked opcodes
    function processRegister(Opcode[] calldata codes, uint256[] calldata extra) external returns (interpret_result result) {
        uint256 pc = 0;
        // program counter for bytecode
        uint256 pc_extra = 0;
        // program counter for data

        // start going through bytecode
        for (uint i = 0; i < codes.length; i++) {
            Opcode current = codes[i];

            // set default value to failure
            interpret_result result = interpret_result.ERROR_UNKNOWN_OPCODE;

            // we try to produce results to register[0]
            if (current == Opcode.OP_INC) {
                register[0]++;
            } else if (current == Opcode.OP_DEC) {
                register[0]--;
            } else if (current == Opcode.OP_ADDI) {
                // add a value from the input data
                register[0] += extra[pc_extra++];
            } else if (current == Opcode.OP_SUBI) {
                // subtract a value from the input data
                register[0] -= extra[pc_extra++];
            } else if (current == Opcode.OP_PUSH1) {
                // not actually pushing, just assigning
                // incremenets pc-extra, by 1
                register[0] = extra[pc_extra++];
            } else if (current == Opcode.OP_PUSH2) {
                // not actually pushing, by 2
                // incremenets pc-extra, twice
                register[0] = extra[pc_extra++];
                register[1] = extra[pc_extra++];
            } else if (current == Opcode.OP_PUSH3) {
                // not actually pushing, by 3
                // incremenets pc-extra, three times
                register[0] = extra[pc_extra++];
                register[1] = extra[pc_extra++];
                register[2] = extra[pc_extra++];
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
                uint256 op_vmp_root = register[0];

                // branch is given as sequences of hashes
                // indicate branch length from calldata
                uint256 branchLength = register[1];

                // check proof
                // TODO - check proof

                // execute script to execute at leaf
                // or execute smart contract
                // option is specified through register
            } else if (current == Opcode.OP_CHECK_TEMPLATE_VERIFY) {
                // we can apply covenants using our interpreter, even recursively

                // get template mask length
                uint256 maskLength = register[0];

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
                        register[0] = pc_extra;
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


    // contract state to maintain stateful register-based interpreter
    Opcode[] statefulCode;
    uint256[] statefulExtra;
    uint256 pc = 0; // program counter for bytecode
    uint256 pc_extra = 0; // program counter for data

    function processStackStateful(Opcode[] memory code, uint256[] memory extra) public returns (interpret_result result) {
        // store state in storage
        statefulCode = code;
        statefulExtra = extra;

        // the following might trail in version from the above implementation
        // this is going to be refactored after stabilizing the above code
        // start going through bytecode
        for (uint i = 0; i < code.length; i++) {
            Opcode current = code[i];
            uint x = 0;

            // set default value to failure
            interpret_result result = interpret_result.ERROR_UNKNOWN_OPCODE;

            if (current == Opcode.OP_VERIFY_MERKLE_PROOF) {
                // given: root of Merkle Tree
                // root is "embedded" in smart contract
                uint256 op_vmp_root = register[0];
                // branch is given
            } else if (current == Opcode.OP_INC) {
                register[2] = register[0] + 1;
            } else if (current == Opcode.OP_ADDI) {
                register[2] = register[0] + register[1];
            } else if (current == Opcode.OP_SUBI) {
                register[2] = register[0] - register[1];
            } else if (current == Opcode.OP_DEC) {
                register[2] = register[0] - 1;
            } else if (current == Opcode.OP_PUSH1) {
                register[0] = extra[i + 1];
                // not actually pushing, just assigning
                pc_extra += 1;
                // shift program counter of extra
            } else if (current == Opcode.OP_VERIFY_MERKLE_PROOF) {
                // nop
            } else if (current == Opcode.OP_CHECK_TEMPLATE_VERIFY) {
                // nop
            } else if (current == Opcode.OP_EXECUTE_ETH_SMART_CONTRACT) {
                // nop
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