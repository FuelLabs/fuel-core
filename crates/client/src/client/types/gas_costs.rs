use crate::client::schema;
use fuel_core_types::fuel_tx::Word;

#[derive(Clone, Debug)]
pub struct GasCosts(fuel_core_types::fuel_tx::GasCosts);

impl GasCosts {
    pub fn new(gas_costs: fuel_core_types::fuel_tx::GasCosts) -> Self {
        GasCosts(gas_costs)
    }
}

impl GasCosts {
    pub fn add(&self) -> Word {
        self.0.add()
    }

    pub fn addi(&self) -> Word {
        self.0.addi()
    }

    pub fn and(&self) -> Word {
        self.0.and()
    }

    pub fn andi(&self) -> Word {
        self.0.andi()
    }

    pub fn bal(&self) -> Word {
        self.0.bal()
    }

    pub fn bhei(&self) -> Word {
        self.0.bhei()
    }

    pub fn bhsh(&self) -> Word {
        self.0.bhsh()
    }

    pub fn burn(&self) -> Word {
        self.0.burn()
    }

    pub fn cb(&self) -> Word {
        self.0.cb()
    }

    pub fn cfsi(&self) -> Word {
        self.0.cfsi()
    }

    pub fn div(&self) -> Word {
        self.0.div()
    }

    pub fn divi(&self) -> Word {
        self.0.divi()
    }

    pub fn eck1(&self) -> Word {
        self.0.eck1()
    }

    pub fn ecr1(&self) -> Word {
        self.0.ecr1()
    }

    pub fn eq_(&self) -> Word {
        self.0.eq_()
    }

    pub fn exp(&self) -> Word {
        self.0.exp()
    }

    pub fn expi(&self) -> Word {
        self.0.expi()
    }

    pub fn flag(&self) -> Word {
        self.0.flag()
    }

    pub fn gm(&self) -> Word {
        self.0.gm()
    }

    pub fn gt(&self) -> Word {
        self.0.gt()
    }

    pub fn gtf(&self) -> Word {
        self.0.gtf()
    }

    pub fn ji(&self) -> Word {
        self.0.ji()
    }

    pub fn jmp(&self) -> Word {
        self.0.jmp()
    }

    pub fn jne(&self) -> Word {
        self.0.jne()
    }

    pub fn jnei(&self) -> Word {
        self.0.jnei()
    }

    pub fn jnzi(&self) -> Word {
        self.0.jnzi()
    }

    pub fn jmpf(&self) -> Word {
        self.0.jmpf()
    }

    pub fn jmpb(&self) -> Word {
        self.0.jmpb()
    }

    pub fn jnzf(&self) -> Word {
        self.0.jnzf()
    }

    pub fn jnzb(&self) -> Word {
        self.0.jnzb()
    }

    pub fn jnef(&self) -> Word {
        self.0.jnef()
    }

    pub fn jneb(&self) -> Word {
        self.0.jneb()
    }

    pub fn lb(&self) -> Word {
        self.0.lb()
    }

    pub fn log(&self) -> Word {
        self.0.log()
    }

    pub fn lt(&self) -> Word {
        self.0.lt()
    }

    pub fn lw(&self) -> Word {
        self.0.lw()
    }

    pub fn mint(&self) -> Word {
        self.0.mint()
    }

    pub fn mlog(&self) -> Word {
        self.0.mlog()
    }

    pub fn mod_op(&self) -> Word {
        self.0.mod_op()
    }

    pub fn modi(&self) -> Word {
        self.0.modi()
    }

    pub fn move_op(&self) -> Word {
        self.0.move_op()
    }

    pub fn movi(&self) -> Word {
        self.0.movi()
    }

    pub fn mroo(&self) -> Word {
        self.0.mroo()
    }

    pub fn mul(&self) -> Word {
        self.0.mul()
    }

    pub fn muli(&self) -> Word {
        self.0.muli()
    }

    pub fn mldv(&self) -> Word {
        self.0.mldv()
    }

    pub fn noop(&self) -> Word {
        self.0.noop()
    }

    pub fn not(&self) -> Word {
        self.0.not()
    }

    pub fn or(&self) -> Word {
        self.0.or()
    }

    pub fn ori(&self) -> Word {
        self.0.ori()
    }

    pub fn poph(&self) -> Word {
        self.0.poph()
    }

    pub fn popl(&self) -> Word {
        self.0.popl()
    }

    pub fn pshh(&self) -> Word {
        self.0.pshh()
    }

    pub fn pshl(&self) -> Word {
        self.0.pshl()
    }

    pub fn ret(&self) -> Word {
        self.0.ret()
    }

    pub fn rvrt(&self) -> Word {
        self.0.rvrt()
    }

    pub fn sb(&self) -> Word {
        self.0.sb()
    }

    pub fn sll(&self) -> Word {
        self.0.sll()
    }

    pub fn slli(&self) -> Word {
        self.0.slli()
    }

    pub fn srl(&self) -> Word {
        self.0.srl()
    }

    pub fn srli(&self) -> Word {
        self.0.srli()
    }

    pub fn srw(&self) -> Word {
        self.0.srw()
    }

    pub fn sub(&self) -> Word {
        self.0.sub()
    }

    pub fn subi(&self) -> Word {
        self.0.subi()
    }

    pub fn sw(&self) -> Word {
        self.0.sw()
    }

    pub fn sww(&self) -> Word {
        self.0.sww()
    }

    pub fn time(&self) -> Word {
        self.0.time()
    }

    pub fn tr(&self) -> Word {
        self.0.tr()
    }

    pub fn tro(&self) -> Word {
        self.0.tro()
    }

    pub fn wdcm(&self) -> Word {
        self.0.wdcm()
    }

    pub fn wqcm(&self) -> Word {
        self.0.wqcm()
    }
    pub fn wdop(&self) -> Word {
        self.0.wdop()
    }

    pub fn wqop(&self) -> Word {
        self.0.wqop()
    }

    pub fn wdml(&self) -> Word {
        self.0.wdml()
    }

    pub fn wqml(&self) -> Word {
        self.0.wqml()
    }

    pub fn wddv(&self) -> Word {
        self.0.wddv()
    }

    pub fn wqdv(&self) -> Word {
        self.0.wqdv()
    }

    pub fn wdmd(&self) -> Word {
        self.0.wdmd()
    }

    pub fn wqmd(&self) -> Word {
        self.0.wqmd()
    }

    pub fn wdam(&self) -> Word {
        self.0.wdam()
    }

    pub fn wqam(&self) -> Word {
        self.0.wqam()
    }

    pub fn wdmm(&self) -> Word {
        self.0.wdmm()
    }

    pub fn wqmm(&self) -> Word {
        self.0.wqmm()
    }

    pub fn xor(&self) -> Word {
        self.0.xor()
    }

    pub fn xori(&self) -> Word {
        self.0.xori()
    }

    pub fn aloc(&self) -> DependentCost {
        self.0.aloc().into()
    }

    pub fn cfe(&self) -> DependentCost {
        self.0.cfe().into()
    }

    pub fn cfei(&self) -> DependentCost {
        self.0.cfei().into()
    }

    pub fn call(&self) -> DependentCost {
        self.0.call().into()
    }

    pub fn ccp(&self) -> DependentCost {
        self.0.ccp().into()
    }

    pub fn croo(&self) -> DependentCost {
        self.0.croo().into()
    }

    pub fn csiz(&self) -> DependentCost {
        self.0.csiz().into()
    }

    pub fn ed19(&self) -> DependentCost {
        self.0.ed19().into()
    }

    pub fn k256(&self) -> DependentCost {
        self.0.k256().into()
    }

    pub fn ldc(&self) -> DependentCost {
        self.0.ldc().into()
    }

    pub fn logd(&self) -> DependentCost {
        self.0.logd().into()
    }

    pub fn mcl(&self) -> DependentCost {
        self.0.mcl().into()
    }

    pub fn mcli(&self) -> DependentCost {
        self.0.mcli().into()
    }

    pub fn mcp(&self) -> DependentCost {
        self.0.mcp().into()
    }

    pub fn mcpi(&self) -> DependentCost {
        self.0.mcpi().into()
    }

    pub fn meq(&self) -> DependentCost {
        self.0.meq().into()
    }

    pub fn retd(&self) -> DependentCost {
        self.0.retd().into()
    }

    pub fn s256(&self) -> DependentCost {
        self.0.s256().into()
    }

    pub fn scwq(&self) -> DependentCost {
        self.0.scwq().into()
    }

    pub fn smo(&self) -> DependentCost {
        self.0.smo().into()
    }

    pub fn srwq(&self) -> DependentCost {
        self.0.srwq().into()
    }

    pub fn swwq(&self) -> DependentCost {
        self.0.swwq().into()
    }

    pub fn contract_root(&self) -> DependentCost {
        self.0.contract_root().into()
    }

    pub fn state_root(&self) -> DependentCost {
        self.0.state_root().into()
    }

    pub fn new_storage_per_byte(&self) -> Word {
        self.0.new_storage_per_byte()
    }

    pub fn vm_initialization(&self) -> DependentCost {
        self.0.vm_initialization().into()
    }
}

impl From<GasCosts> for fuel_core_types::fuel_tx::GasCosts {
    fn from(value: GasCosts) -> Self {
        value.0
    }
}

#[derive(Copy, Clone, Debug)]
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

impl From<fuel_core_types::fuel_tx::DependentCost> for DependentCost {
    fn from(value: fuel_core_types::fuel_tx::DependentCost) -> Self {
        match value {
            fuel_core_types::fuel_tx::DependentCost::LightOperation {
                base,
                units_per_gas,
            } => DependentCost::LightOperation {
                base,
                units_per_gas,
            },
            fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                base,
                gas_per_unit,
            } => DependentCost::HeavyOperation { base, gas_per_unit },
        }
    }
}

impl From<schema::chain::DependentCost> for fuel_core_types::fuel_tx::DependentCost {
    fn from(value: schema::chain::DependentCost) -> Self {
        match value {
            schema::chain::DependentCost::LightOperation(
                schema::chain::LightOperation {
                    base,
                    units_per_gas,
                },
            ) => fuel_core_types::fuel_tx::DependentCost::LightOperation {
                base: base.into(),
                units_per_gas: units_per_gas.into(),
            },
            schema::chain::DependentCost::HeavyOperation(
                schema::chain::HeavyOperation { base, gas_per_unit },
            ) => fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                base: base.into(),
                gas_per_unit: gas_per_unit.into(),
            },
            schema::chain::DependentCost::Unknown => {
                fuel_core_types::fuel_tx::DependentCost::HeavyOperation {
                    base: 0,
                    gas_per_unit: 0,
                }
            }
        }
    }
}

impl From<schema::chain::DependentCost> for DependentCost {
    fn from(value: schema::chain::DependentCost) -> Self {
        let vm_value: fuel_core_types::fuel_tx::DependentCost = value.into();
        vm_value.into()
    }
}
