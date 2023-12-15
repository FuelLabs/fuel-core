// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SimpleContract {
    event Trigger(uint16 x);

    constructor() {
        emit Trigger(0);
    }

    function emitTrigger(uint16 x) public {
        emit Trigger(x);
    }
}
