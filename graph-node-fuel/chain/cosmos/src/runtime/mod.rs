pub mod abi;

#[cfg(test)]
mod test {
    use crate::protobuf::*;

    use graph::semver::Version;

    /// A macro that takes an ASC struct value definition and calls AscBytes methods to check that
    /// memory layout is padded properly.
    macro_rules! assert_asc_bytes {
        ($struct_name:ident {
            $($field:ident : $field_value:expr),+
            $(,)? // trailing
        }) => {
            let value = $struct_name {
                $($field: $field_value),+
            };

            // just call the function. it will panic on misalignments
            let asc_bytes = value.to_asc_bytes().unwrap();

            let value_004 = $struct_name::from_asc_bytes(&asc_bytes, &Version::new(0, 0, 4)).unwrap();
            let value_005 = $struct_name::from_asc_bytes(&asc_bytes, &Version::new(0, 0, 5)).unwrap();

            // turn the values into bytes again to verify that they are the same as the original
            // because these types usually don't implement PartialEq
            assert_eq!(
                asc_bytes,
                value_004.to_asc_bytes().unwrap(),
                "Expected {} v0.0.4 asc bytes to be the same",
                stringify!($struct_name)
            );
            assert_eq!(
                asc_bytes,
                value_005.to_asc_bytes().unwrap(),
                "Expected {} v0.0.5 asc bytes to be the same",
                stringify!($struct_name)
            );
        };
    }

    #[test]
    fn test_asc_type_alignment() {
        // TODO: automatically generate these tests for each struct in derive(AscType) macro

        assert_asc_bytes!(AscBlock {
            header: new_asc_ptr(),
            evidence: new_asc_ptr(),
            last_commit: new_asc_ptr(),
            result_begin_block: new_asc_ptr(),
            result_end_block: new_asc_ptr(),
            transactions: new_asc_ptr(),
            validator_updates: new_asc_ptr(),
        });

        assert_asc_bytes!(AscHeaderOnlyBlock {
            header: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEventData {
            event: new_asc_ptr(),
            block: new_asc_ptr(),
            tx: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTransactionData {
            tx: new_asc_ptr(),
            block: new_asc_ptr(),
        });

        assert_asc_bytes!(AscMessageData {
            message: new_asc_ptr(),
            block: new_asc_ptr(),
            tx: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTransactionContext {
            hash: new_asc_ptr(),
            index: 20,
            code: 20,
            gas_wanted: 20,
            gas_used: 20,
        });

        assert_asc_bytes!(AscHeader {
            version: new_asc_ptr(),
            chain_id: new_asc_ptr(),
            height: 20,
            time: new_asc_ptr(),
            last_block_id: new_asc_ptr(),
            last_commit_hash: new_asc_ptr(),
            data_hash: new_asc_ptr(),
            validators_hash: new_asc_ptr(),
            next_validators_hash: new_asc_ptr(),
            consensus_hash: new_asc_ptr(),
            app_hash: new_asc_ptr(),
            last_results_hash: new_asc_ptr(),
            evidence_hash: new_asc_ptr(),
            proposer_address: new_asc_ptr(),
            hash: new_asc_ptr(),
        });

        assert_asc_bytes!(AscConsensus { block: 0, app: 0 });

        assert_asc_bytes!(AscTimestamp {
            seconds: 20,
            nanos: 20,
        });

        assert_asc_bytes!(AscBlockId {
            hash: new_asc_ptr(),
            part_set_header: new_asc_ptr(),
        });

        assert_asc_bytes!(AscPartSetHeader {
            total: 20,
            hash: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEvidenceList {
            evidence: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEvidence {
            duplicate_vote_evidence: new_asc_ptr(),
            light_client_attack_evidence: new_asc_ptr(),
        });

        assert_asc_bytes!(AscDuplicateVoteEvidence {
            vote_a: new_asc_ptr(),
            vote_b: new_asc_ptr(),
            total_voting_power: 20,
            validator_power: 20,
            timestamp: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEventVote {
            event_vote_type: 20,
            height: 20,
            round: 20,
            block_id: new_asc_ptr(),
            timestamp: new_asc_ptr(),
            validator_address: new_asc_ptr(),
            validator_index: 20,
            signature: new_asc_ptr(),
        });

        assert_asc_bytes!(AscLightClientAttackEvidence {
            conflicting_block: new_asc_ptr(),
            common_height: 20,
            total_voting_power: 20,
            byzantine_validators: new_asc_ptr(),
            timestamp: new_asc_ptr(),
        });

        assert_asc_bytes!(AscLightBlock {
            signed_header: new_asc_ptr(),
            validator_set: new_asc_ptr(),
        });

        assert_asc_bytes!(AscSignedHeader {
            header: new_asc_ptr(),
            commit: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCommit {
            height: 20,
            round: 20,
            block_id: new_asc_ptr(),
            signatures: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCommitSig {
            block_id_flag: 20,
            validator_address: new_asc_ptr(),
            timestamp: new_asc_ptr(),
            signature: new_asc_ptr(),
        });

        assert_asc_bytes!(AscValidatorSet {
            validators: new_asc_ptr(),
            proposer: new_asc_ptr(),
            total_voting_power: 20,
        });

        assert_asc_bytes!(AscValidator {
            address: new_asc_ptr(),
            pub_key: new_asc_ptr(),
            voting_power: 20,
            proposer_priority: 20,
        });

        assert_asc_bytes!(AscPublicKey {
            ed25519: new_asc_ptr(),
            secp256k1: new_asc_ptr(),
        });

        assert_asc_bytes!(AscResponseBeginBlock {
            events: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEvent {
            event_type: new_asc_ptr(),
            attributes: new_asc_ptr(),
        });

        assert_asc_bytes!(AscEventAttribute {
            key: new_asc_ptr(),
            value: new_asc_ptr(),
            index: true,
        });

        assert_asc_bytes!(AscResponseEndBlock {
            validator_updates: new_asc_ptr(),
            consensus_param_updates: new_asc_ptr(),
            events: new_asc_ptr(),
        });

        assert_asc_bytes!(AscValidatorUpdate {
            address: new_asc_ptr(),
            pub_key: new_asc_ptr(),
            power: 20,
        });

        assert_asc_bytes!(AscConsensusParams {
            block: new_asc_ptr(),
            evidence: new_asc_ptr(),
            validator: new_asc_ptr(),
            version: new_asc_ptr(),
        });

        assert_asc_bytes!(AscBlockParams {
            max_bytes: 20,
            max_gas: 20,
        });

        assert_asc_bytes!(AscEvidenceParams {
            max_age_num_blocks: 20,
            max_age_duration: new_asc_ptr(),
            max_bytes: 20,
        });

        assert_asc_bytes!(AscDuration {
            seconds: 20,
            nanos: 20,
        });

        assert_asc_bytes!(AscValidatorParams {
            pub_key_types: new_asc_ptr(),
        });

        assert_asc_bytes!(AscVersionParams { app_version: 20 });

        assert_asc_bytes!(AscTxResult {
            height: 20,
            index: 20,
            tx: new_asc_ptr(),
            result: new_asc_ptr(),
            hash: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTx {
            body: new_asc_ptr(),
            auth_info: new_asc_ptr(),
            signatures: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTxBody {
            messages: new_asc_ptr(),
            memo: new_asc_ptr(),
            timeout_height: 20,
            extension_options: new_asc_ptr(),
            non_critical_extension_options: new_asc_ptr(),
        });

        assert_asc_bytes!(AscAny {
            type_url: new_asc_ptr(),
            value: new_asc_ptr(),
        });

        assert_asc_bytes!(AscAuthInfo {
            signer_infos: new_asc_ptr(),
            fee: new_asc_ptr(),
            tip: new_asc_ptr(),
        });

        assert_asc_bytes!(AscSignerInfo {
            public_key: new_asc_ptr(),
            mode_info: new_asc_ptr(),
            sequence: 20,
        });

        assert_asc_bytes!(AscModeInfo {
            single: new_asc_ptr(),
            multi: new_asc_ptr(),
        });

        assert_asc_bytes!(AscModeInfoSingle { mode: 20 });

        assert_asc_bytes!(AscModeInfoMulti {
            bitarray: new_asc_ptr(),
            mode_infos: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCompactBitArray {
            extra_bits_stored: 20,
            elems: new_asc_ptr(),
        });

        assert_asc_bytes!(AscFee {
            amount: new_asc_ptr(),
            gas_limit: 20,
            payer: new_asc_ptr(),
            granter: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCoin {
            denom: new_asc_ptr(),
            amount: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTip {
            amount: new_asc_ptr(),
            tipper: new_asc_ptr(),
        });

        assert_asc_bytes!(AscResponseDeliverTx {
            code: 20,
            data: new_asc_ptr(),
            log: new_asc_ptr(),
            info: new_asc_ptr(),
            gas_wanted: 20,
            gas_used: 20,
            events: new_asc_ptr(),
            codespace: new_asc_ptr(),
        });

        assert_asc_bytes!(AscValidatorSetUpdates {
            validator_updates: new_asc_ptr(),
        });
    }

    // non-null AscPtr
    fn new_asc_ptr<T>() -> AscPtr<T> {
        AscPtr::new(12)
    }
}
