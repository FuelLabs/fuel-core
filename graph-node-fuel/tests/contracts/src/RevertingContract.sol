// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RevertingContract {
    event Trigger();

    constructor() {
        emit Trigger();
    }

    function inc(uint256 value) public pure returns (uint256) {
        require(value < 10, "can only handle values < 10");
        return value + 1;
    }
}
