use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        Instruction,
        RegId,
    },
    fuel_tx::{
        Address,
        AssetId,
    },
};

/// Generates the bytecode for the fee collection contract.
/// The contract expects `AssetId` and `output_index` as a first elements in `script_data`.
pub fn generate(address: Address) -> Vec<u8> {
    let start_jump = vec![
        // Jump over the embedded address, which is placed immediately after the jump
        op::ji((1 + (Address::LEN / Instruction::SIZE)).try_into().unwrap()),
    ];

    let asset_id_register = 0x10;
    let balance_register = 0x11;
    let contract_id_register = 0x12;
    let output_index_register = 0x13;
    let recipient_id_register = 0x14;
    let body = vec![
        // Load pointer to AssetId
        op::gtf_args(asset_id_register, 0x00, GTFArgs::ScriptData),
        // Load output index
        op::addi(
            output_index_register,
            asset_id_register,
            u16::try_from(AssetId::LEN).expect("The size is 32"),
        ),
        op::lw(output_index_register, output_index_register, 0),
        // Gets pointer to the contract id
        op::move_(contract_id_register, RegId::FP),
        // Get the balance of asset ID in the contract
        op::bal(balance_register, asset_id_register, contract_id_register),
        // If balance == 0, return early
        op::jnzf(balance_register, RegId::ZERO, 1),
        op::ret(RegId::ONE),
        // Pointer to the recipient address
        op::addi(
            recipient_id_register,
            RegId::IS,
            Instruction::SIZE.try_into().unwrap(),
        ),
        // Perform the transfer
        op::tro(
            recipient_id_register,
            output_index_register,
            balance_register,
            asset_id_register,
        ),
        // Return
        op::ret(RegId::ONE),
    ];

    let mut asm_bytes: Vec<u8> = start_jump.into_iter().collect();
    asm_bytes.extend_from_slice(address.as_slice()); // Embed the address
    let body: Vec<u8> = body.into_iter().collect();
    asm_bytes.extend(body.as_slice());

    asm_bytes
}
