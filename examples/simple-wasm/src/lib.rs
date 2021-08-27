#![no_std]
use core::convert::TryInto;

use fuel_indexer::Entity;
use fuel_indexer::types::*;

//////////////////////Should be Auto generated....///////////////////////
const NAMESPACE: &'static str = "test_namespace";


#[no_mangle]
fn get_namespace_ptr() -> *const u8 {
    NAMESPACE.as_ptr()
}

#[no_mangle]
fn get_namespace_len() -> u32 {
    NAMESPACE.len() as u32
}

#[derive(Debug, PartialEq, Eq)]
pub struct Thing1 {
    ident: u64,
    account: Address,
}

// TODO: derive macro for Entity...
impl Entity for Thing1 {
    const TYPE_ID: u32 = 0;

    fn id(&self) -> u64 {
        self.ident
    }

    fn to_buffer(&self, buf: &mut [u8]) -> u32 {
        let mut pos = 0;

        let size = core::mem::size_of::<u32>();
        buf[pos..pos+size].copy_from_slice(&u32::to_le_bytes(ColumnType::Ident.into()));
        pos += size;

        let size = core::mem::size_of::<u64>();
        buf[pos..pos+WASM_WORD].copy_from_slice(&(size as u32).to_le_bytes());
        pos += WASM_WORD;
        buf[pos..pos+size].copy_from_slice(&self.ident.to_le_bytes());
        pos += size;

        let size = core::mem::size_of::<u32>();
        buf[pos..pos+size].copy_from_slice(&u32::to_le_bytes(ColumnType::Address.into()));
        pos += size;

        let size = core::mem::size_of::<Address>();
        buf[pos..pos+WASM_WORD].copy_from_slice(&(size as u32).to_le_bytes());
        pos += WASM_WORD;
        buf[pos..pos+size].copy_from_slice(&*self.account);
        pos += size;

        pos as u32
    }

    fn from_buffer(buf: &[u8]) -> Self {
        let mut pos = 0;

        let size = core::mem::size_of::<u32>();
        let ty = ColumnType::from(u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length")));
        pos += size;
        debug_assert_eq!(ty, ColumnType::Ident);

        let field_size = u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;

        let size = core::mem::size_of::<u64>();
        debug_assert_eq!(field_size as usize, size);
        let ident = u64::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;

        let size = core::mem::size_of::<u32>();
        let ty = ColumnType::from(u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length")));
        pos += size;
        debug_assert_eq!(ty, ColumnType::Address);

        let field_size = u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;
        debug_assert_eq!(field_size as usize, core::mem::size_of::<Address>());

        let mut bytes = [0u8; core::mem::size_of::<Address>()];
        bytes.copy_from_slice(&buf[pos..pos+core::mem::size_of::<Address>()]);

        let account = Address::from(bytes);

        Thing1 {
            ident,
            account,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Thing2 {
    ident: u64,
    account: Address,
    hash: Bytes32,
}

// TODO: derive macro for Entity...
impl Entity for Thing2 {
    const TYPE_ID: u32 = 1;

    fn id(&self) -> u64 {
        self.ident
    }

    fn to_buffer(&self, buf: &mut [u8]) -> u32 {
        let mut pos = 0;

        let size = core::mem::size_of::<u32>();
        buf[pos..pos+size].copy_from_slice(&u32::to_le_bytes(ColumnType::Ident.into()));
        pos += size;

        let size = core::mem::size_of::<u64>();
        buf[pos..pos+WASM_WORD].copy_from_slice(&(size as u32).to_le_bytes());
        pos += WASM_WORD;
        buf[pos..pos+size].copy_from_slice(&self.ident.to_le_bytes());
        pos += size;

        let size = core::mem::size_of::<u32>();
        buf[pos..pos+size].copy_from_slice(&u32::to_le_bytes(ColumnType::Address.into()));
        pos += size;

        let size = core::mem::size_of::<Address>();
        buf[pos..pos+WASM_WORD].copy_from_slice(&(size as u32).to_le_bytes());
        pos += WASM_WORD;
        buf[pos..pos+size].copy_from_slice(&*self.account);
        pos += size;

        let size = core::mem::size_of::<u32>();
        buf[pos..pos+size].copy_from_slice(&u32::to_le_bytes(ColumnType::Bytes32.into()));
        pos += size;

        let size = core::mem::size_of::<Bytes32>();
        buf[pos..pos+WASM_WORD].copy_from_slice(&(size as u32).to_le_bytes());
        pos += WASM_WORD;
        buf[pos..pos+size].copy_from_slice(&*self.hash);
        pos += size;

        pos as u32
    }

    fn from_buffer(buf: &[u8]) -> Self {
        let mut pos = 0;

        let size = core::mem::size_of::<u32>();
        let ty = ColumnType::from(u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length")));
        pos += size;
        debug_assert_eq!(ty, ColumnType::Ident);

        let field_size = u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;
        let size = core::mem::size_of::<u64>();
        debug_assert_eq!(field_size as usize, size);

        let ident = u64::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;

        let size = core::mem::size_of::<u32>();
        let ty = ColumnType::from(u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length")));
        pos += size;
        debug_assert_eq!(ty, ColumnType::Address);

        let field_size = u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;
        debug_assert_eq!(field_size as usize, core::mem::size_of::<Address>());

        let mut bytes = [0u8; core::mem::size_of::<Address>()];
        bytes.copy_from_slice(&buf[pos..pos+core::mem::size_of::<Address>()]);
        pos += bytes.len();

        let account = Address::from(bytes);

        let size = core::mem::size_of::<u32>();
        let ty = ColumnType::from(u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length")));
        pos += size;
        debug_assert_eq!(ty, ColumnType::Bytes32);

        let field_size = u32::from_le_bytes(buf[pos..pos+size].try_into().expect("Invalid slice length"));
        pos += size;
        debug_assert_eq!(field_size as usize, core::mem::size_of::<Bytes32>());

        let mut bytes = [0u8; core::mem::size_of::<Bytes32>()];
        bytes.copy_from_slice(&buf[pos..pos+core::mem::size_of::<Bytes32>()]);

        let hash = Bytes32::from(bytes);

        Thing2 {
            ident,
            account,
            hash,
        }
    }
}

//////////////////////Above Should be Auto generated....///////////////////////


#[no_mangle]
fn function_one(ptr: *const u8, len: u32) {
    let t6 = Thing1 {
        ident: 463,
        account: Address::from([4u8; 32]),
    };
    t6.save();

    let t1 = Thing1::load(463);

    //assert_eq!(t1, t6);
}


#[no_mangle]
fn function_two(ptr: *const u8, len: u32) {
    let t6 = Thing1 {
        ident: 468,
        account: Address::from([4u8; 32]),
    };
    t6.save();

    let t1 = Thing1::load(463);

    //assert_eq!(t1, t6);
}
