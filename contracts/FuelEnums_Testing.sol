import "./FuelEnums.sol";

contract interpret_result_Contract {
    function INITIAL() external view returns (interpret_result ret) { ret = interpret_result.INITIAL; }
    function SUCCESS() external view returns (interpret_result ret) { ret = interpret_result.SUCCESS; }
    function STOPPED() external view returns (interpret_result ret) { ret = interpret_result.STOPPED; }
    function ERROR_TEMPLATE_FAILURE() external view returns (interpret_result ret) { ret = interpret_result.ERROR_TEMPLATE_FAILURE; }
    function ERROR_UNKNOWN_OPCODE() external view returns (interpret_result ret) { ret = interpret_result.ERROR_UNKNOWN_OPCODE; }
}

contract Opcode_Contract {
    function OP_STOP() external view returns (Opcode ret) { ret = Opcode.OP_STOP; }
    function OP_ADD() external view returns (Opcode ret) { ret = Opcode.OP_ADD; }
    function OP_SUB() external view returns (Opcode ret) { ret = Opcode.OP_SUB; }
    function OP_MUL() external view returns (Opcode ret) { ret = Opcode.OP_MUL; }
    function OP_DIV() external view returns (Opcode ret) { ret = Opcode.OP_DIV; }
    function OP_SDIV() external view returns (Opcode ret) { ret = Opcode.OP_SDIV; }
    function OP_MOD() external view returns (Opcode ret) { ret = Opcode.OP_MOD; }
    function OP_SMOD() external view returns (Opcode ret) { ret = Opcode.OP_SMOD; }
    function OP_ADDMOD() external view returns (Opcode ret) { ret = Opcode.OP_ADDMOD; }
    function OP_MULMOD() external view returns (Opcode ret) { ret = Opcode.OP_MULMOD; }
    function OP_EXP() external view returns (Opcode ret) { ret = Opcode.OP_EXP; }
    function OP_SIGNEXTEND() external view returns (Opcode ret) { ret = Opcode.OP_SIGNEXTEND; }
    function OP_EQ() external view returns (Opcode ret) { ret = Opcode.OP_EQ; }
    function OP_LT() external view returns (Opcode ret) { ret = Opcode.OP_LT; }
    function OP_GT() external view returns (Opcode ret) { ret = Opcode.OP_GT; }

    function OP_SLT() external view returns (Opcode ret) { ret = Opcode.OP_SLT; }
    function OP_SGT() external view returns (Opcode ret) { ret = Opcode.OP_SGT; }
    function OP_ISZERO() external view returns (Opcode ret) { ret = Opcode.OP_ISZERO; }
    function OP_AND() external view returns (Opcode ret) { ret = Opcode.OP_AND; }
    function OP_OR() external view returns (Opcode ret) { ret = Opcode.OP_OR; }
    function OP_XOR() external view returns (Opcode ret) { ret = Opcode.OP_XOR; }
    function OP_NOT() external view returns (Opcode ret) { ret = Opcode.OP_NOT; }
    function OP_BYTE() external view returns (Opcode ret) { ret = Opcode.OP_BYTE; }
    function OP_SHL() external view returns (Opcode ret) { ret = Opcode.OP_SHL; }
    function OP_SHR() external view returns (Opcode ret) { ret = Opcode.OP_SHR; }
    function OP_SAR() external view returns (Opcode ret) { ret = Opcode.OP_SAR; }
    function OP_KECCAK() external view returns (Opcode ret) { ret = Opcode.OP_KECCAK; }
    function OP_GAS() external view returns (Opcode ret) { ret = Opcode.OP_GAS; }
    function OP_BALANCE() external view returns (Opcode ret) { ret = Opcode.OP_BALANCE; }
    function OP_ADDRESS() external view returns (Opcode ret) { ret = Opcode.OP_ADDRESS; }
    function OP_ORIGIN() external view returns (Opcode ret) { ret = Opcode.OP_ORIGIN; }
    function OP_CALLER() external view returns (Opcode ret) { ret = Opcode.OP_CALLER; }
    function OP_CALLVALUE() external view returns (Opcode ret) { ret = Opcode.OP_CALLVALUE; }
    function OP_CALLDATALOD() external view returns (Opcode ret) { ret = Opcode.OP_CALLDATALOD; }
    function OP_CALLDATASIZE() external view returns (Opcode ret) { ret = Opcode.OP_CALLDATASIZE; }
    function OP_CALLDATACOPY() external view returns (Opcode ret) { ret = Opcode.OP_CALLDATACOPY; }
    function OP_CODESIZE() external view returns (Opcode ret) { ret = Opcode.OP_CODESIZE; }
    function OP_CODECOPY() external view returns (Opcode ret) { ret = Opcode.OP_CODECOPY; }
    function OP_GASPRICE() external view returns (Opcode ret) { ret = Opcode.OP_GASPRICE; }
    function OP_EXTCODESIZE() external view returns (Opcode ret) { ret = Opcode.OP_EXTCODESIZE; }
    function OP_EXTCODECOPY() external view returns (Opcode ret) { ret = Opcode.OP_EXTCODECOPY; }
    function OP_RETURNDATASIZE() external view returns (Opcode ret) { ret = Opcode.OP_RETURNDATASIZE; }
    function OP_RETURNDATACOPY() external view returns (Opcode ret) { ret = Opcode.OP_RETURNDATACOPY; }
    function OP_EXTCODEHASH() external view returns (Opcode ret) { ret = Opcode.OP_EXTCODEHASH; }
    function OP_BLOCKHASH() external view returns (Opcode ret) { ret = Opcode.OP_BLOCKHASH; }
    function OP_COINBASE() external view returns (Opcode ret) { ret = Opcode.OP_COINBASE; }
    function OP_TIMESTAMP() external view returns (Opcode ret) { ret = Opcode.OP_TIMESTAMP; }
    function OP_NUMBER() external view returns (Opcode ret) { ret = Opcode.OP_NUMBER; }
    function OP_DIFFICULTY() external view returns (Opcode ret) { ret = Opcode.OP_DIFFICULTY; }
    function OP_GASLIMIT() external view returns (Opcode ret) { ret = Opcode.OP_GASLIMIT; }
    function OP_MLOAD() external view returns (Opcode ret) { ret = Opcode.OP_MLOAD; }
    function OP_MSTORE() external view returns (Opcode ret) { ret = Opcode.OP_MSTORE; }
    function OP_MSTORE8() external view returns (Opcode ret) { ret = Opcode.OP_MSTORE8; }
    function OP_SLOAD() external view returns (Opcode ret) { ret = Opcode.OP_SLOAD; }
    function OP_SSTORE() external view returns (Opcode ret) { ret = Opcode.OP_SSTORE; }
    function OP_MSIZE() external view returns (Opcode ret) { ret = Opcode.OP_MSIZE; }
    function OP_JUMP() external view returns (Opcode ret) { ret = Opcode.OP_JUMP; }
    function OP_JUMPI() external view returns (Opcode ret) { ret = Opcode.OP_JUMPI; }
    function OP_PC() external view returns (Opcode ret) { ret = Opcode.OP_PC; }
    function OP_JUMPDEST() external view returns (Opcode ret) { ret = Opcode.OP_JUMPDEST; }
    function OP_LOG0() external view returns (Opcode ret) { ret = Opcode.OP_LOG0; }
    function OP_LOG1() external view returns (Opcode ret) { ret = Opcode.OP_LOG1; }
    function OP_LOG2() external view returns (Opcode ret) { ret = Opcode.OP_LOG2; }
    function OP_LOG3() external view returns (Opcode ret) { ret = Opcode.OP_LOG3; }
    function OP_LOG4() external view returns (Opcode ret) { ret = Opcode.OP_LOG4; }
    function OP_CREATE() external view returns (Opcode ret) { ret = Opcode.OP_CREATE; }
    function OP_CALL() external view returns (Opcode ret) { ret = Opcode.OP_CALL; }
    function OP_CALLCODE() external view returns (Opcode ret) { ret = Opcode.OP_CALLCODE; }
    function OP_RETURN() external view returns (Opcode ret) { ret = Opcode.OP_RETURN; }
    function OP_DELEGATECALL() external view returns (Opcode ret) { ret = Opcode.OP_DELEGATECALL; }
    function OP_CREATE2() external view returns (Opcode ret) { ret = Opcode.OP_CREATE2; }
    function OP_STATICCALL() external view returns (Opcode ret) { ret = Opcode.OP_STATICCALL; }
    function OP_REVERT() external view returns (Opcode ret) { ret = Opcode.OP_REVERT; }
    function OP_INVALID() external view returns (Opcode ret) { ret = Opcode.OP_INVALID; }
    function OP_SELFDESTRUCT() external view returns (Opcode ret) { ret = Opcode.OP_SELFDESTRUCT; }
    function OP_POP() external view returns (Opcode ret) { ret = Opcode.OP_POP; }
    function OP_PUSH() external view returns (Opcode ret) { ret = Opcode.OP_PUSH; }
    function OP_DUP() external view returns (Opcode ret) { ret = Opcode.OP_DUP; }
    function OP_SWAP() external view returns (Opcode ret) { ret = Opcode.OP_SWAP; }

    function OP_VERIFY_MERKLE_PROOF() external view returns (Opcode ret) { ret = Opcode.OP_VERIFY_MERKLE_PROOF; }
    function OP_CHECK_TEMPLATE_VERIFY() external view returns (Opcode ret) { ret = Opcode.OP_CHECK_TEMPLATE_VERIFY; } // also known as CheckOutputVerify
    function OP_EXECUTE_ETH_SMART_CONTRACT() external view returns (Opcode ret) { ret = Opcode.OP_EXECUTE_ETH_SMART_CONTRACT; }

    function OP_INC() external view returns (Opcode ret) { ret = Opcode.OP_INC; }
    function OP_DEC() external view returns (Opcode ret) { ret = Opcode.OP_DEC; }



}