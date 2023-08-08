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
        pub k256: u64,
        pub lb: u64,
        pub log: u64,
        pub lt: u64,
        pub lw: u64,
        pub mcpi: u64,
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
        pub s256: u64,
        pub sb: u64,
        pub scwq: u64,
        pub sll: u64,
        pub slli: u64,
        pub srl: u64,
        pub srli: u64,
        pub srw: u64,
        pub sub: u64,
        pub subi: u64,
        pub sw: u64,
        pub sww: u64,
        pub swwq: u64,
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
        pub ldc: DependentCost,
        pub logd: DependentCost,
        pub mcl: DependentCost,
        pub mcli: DependentCost,
        pub mcp: DependentCost,
        pub meq: DependentCost,
        pub retd: DependentCost,
        pub smo: DependentCost,
        pub srwq: DependentCost,
    }
}

#[derive(Clone, Debug)]
pub struct DependentCost {
    pub base: u64,
    pub dep_per_unit: u64,
}

impl From<DependentCost> for fuel_core_types::fuel_tx::DependentCost {
    fn from(value: DependentCost) -> Self {
        Self {
            base: value.base,
            dep_per_unit: value.dep_per_unit,
        }
    }
}

impl From<schema::chain::DependentCost> for DependentCost {
    fn from(value: schema::chain::DependentCost) -> Self {
        Self {
            base: value.base.into(),
            dep_per_unit: value.dep_per_unit.into(),
        }
    }
}
