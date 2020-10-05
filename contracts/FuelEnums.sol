pragma solidity ^0.6.8;

enum Opcode {
    OP_INC,
    OP_DEC,
    OP_ADDI,
    OP_SUBI,
    OP_PUSH1,
    OP_PUSH2,
    OP_PUSH3,
    OP_POP,
    OP_VERIFY_MERKLE_PROOF,
    OP_CHECK_TEMPLATE_VERIFY, // also known as CheckOutputVerify
    OP_EXECUTE_ETH_SMART_CONTRACT
}

enum interpret_result {
    SUCCESS,
    ERROR_TEMPLATE_FAILURE, // applies only to OP_CHECK_TEMPLATE_VERIFY
    ERROR_UNKNOWN_OPCODE // will often never apply when enum used to populate bytecode
}
