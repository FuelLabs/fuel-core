use crate::client::schema;

macro_rules! include_from_impls {
    ($(#[$meta:meta])* $vis:vis struct $name:ident {
        $($field_vis:vis $field_name:ident: $field_type:ty,)*
    }) => {
        $(#[$meta])*
        $vis struct $name {
            $($field_vis $field_name: $field_type,)*
        }

        impl From<$name> for fuel_core_types::fuel_tx::GasCosts {
           fn from(value: $name) -> Self {
               let values = fuel_core_types::fuel_tx::GasCostsValues {
                   $($field_name: value.$field_name.into(),)*
               };
               Self::new(values)
           }
        }

        impl From<schema::chain::GasCosts> for $name {
            fn from(value: schema::chain::GasCosts) -> Self {
                Self {
                    $($field_name: value.$field_name.into(),)*
                }
            }

        }
    }
}

include_from_impls! {
    #[derive(Clone, Debug)]
    pub struct GasCosts {
        pub add: u64,
        pub addi: u64,
        pub aloc: u64,
        pub and: u64,
        pub andi: u64,
        pub bal: u64,
        pub bhei: u64,
        pub bhsh: u64,
        pub burn: u64,
        pub cb: u64,
        pub cfei: u64,
        pub cfsi: u64,
        pub croo: u64,
        pub div: u64,
        pub divi: u64,
        pub eck1: u64,
        pub ecr1: u64,
        pub ed19: u64,
        pub eq: u64,
        pub exp: u64,
        pub expi: u64,
        pub flag: u64,
        pub gm: u64,
        pub gt: u64,
        pub gtf: u64,
        pub ji: u64,
        pub jmp: u64,
        pub jne: u64,
        pub jnei: u64,
        pub jnzi: u64,
        pub jmpf: u64,
        pub jmpb: u64,
        pub jnzf: u64,
        pub jnzb: u64,
        pub jnef: u64,
        pub jneb: u64,
        pub lb: u64,
        pub log: u64,
        pub lt: u64,
        pub lw: u64,
        pub mint: u64,
        pub mlog: u64,
        pub mod_op: u64,
        pub modi: u64,
        pub move_op: u64,
        pub movi: u64,
        pub mroo: u64,
        pub mul: u64,
        pub muli: u64,
        pub mldv: u64,
        pub noop: u64,
        pub not: u64,
        pub or: u64,
        pub ori: u64,
        pub poph: u64,
        pub popl: u64,
        pub pshh: u64,
        pub pshl: u64,
        pub ret: u64,
        pub rvrt: u64,
        pub sb: u64,
        pub sll: u64,
        pub slli: u64,
        pub srl: u64,
        pub srli: u64,
        pub srw: u64,
        pub sub: u64,
        pub subi: u64,
        pub sw: u64,
        pub sww: u64,
        pub time: u64,
        pub tr: u64,
        pub tro: u64,
        pub wdcm: u64,
        pub wqcm: u64,
        pub wdop: u64,
        pub wqop: u64,
        pub wdml: u64,
        pub wqml: u64,
        pub wddv: u64,
        pub wqdv: u64,
        pub wdmd: u64,
        pub wqmd: u64,
        pub wdam: u64,
        pub wqam: u64,
        pub wdmm: u64,
        pub wqmm: u64,
        pub xor: u64,
        pub xori: u64,

        pub call: DependentCost,
        pub ccp: DependentCost,
        pub csiz: DependentCost,
        pub k256: DependentCost,
        pub ldc: DependentCost,
        pub logd: DependentCost,
        pub mcl: DependentCost,
        pub mcli: DependentCost,
        pub mcp: DependentCost,
        pub mcpi: DependentCost,
        pub meq: DependentCost,
        pub retd: DependentCost,
        pub s256: DependentCost,
        pub scwq: DependentCost,
        pub smo: DependentCost,
        pub srwq: DependentCost,
        pub swwq: DependentCost,

        // Non-opcode prices
        pub contract_root: DependentCost,
        pub state_root: DependentCost,
        pub vm_initialization: DependentCost,
        pub new_storage_per_byte: u64,
    }
}

#[derive(Clone, Debug)]
pub enum DependentCost {
    LightOperation { base: u64, units_per_gas: u64 },
    HeavyOperation { base: u64, gas_per_unit: u64 },
}

impl From<DependentCost> for fuel_core_types::fuel_tx::DependentCost {
    fn from(value: DependentCost) -> Self {
        match value {
            DependentCost::LightOperation {
                base,
                units_per_gas,
            } => fuel_core_types::fuel_tx::DependentCost::LightOperation {
                base,
                units_per_gas,
            },
            DependentCost::HeavyOperation { base, gas_per_unit } => {
                fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                    base,
                    gas_per_unit,
                }
            }
        }
    }
}

impl From<schema::chain::DependentCost> for DependentCost {
    fn from(value: schema::chain::DependentCost) -> Self {
        match value {
            schema::chain::DependentCost::LightOperation(
                schema::chain::LightOperation {
                    base,
                    units_per_gas,
                },
            ) => DependentCost::LightOperation {
                base: base.into(),
                units_per_gas: units_per_gas.into(),
            },
            schema::chain::DependentCost::HeavyOperation(
                schema::chain::HeavyOperation { base, gas_per_unit },
            ) => DependentCost::HeavyOperation {
                base: base.into(),
                gas_per_unit: gas_per_unit.into(),
            },
            schema::chain::DependentCost::Unknown => DependentCost::HeavyOperation {
                base: 0,
                gas_per_unit: 0,
            },
        }
    }
}
