#![cfg_attr(not(feature = "use-std"), no_std)]

use core::panic::PanicInfo;

#[cfg_attr(not(feature = "use-std"), panic_handler)]
fn panic(info: &PanicInfo<'_>) -> ! { loop {} }

pub mod types;

extern "C" {
    fn ff_get_object(type_id: u32, ptr: *const u8) -> *const u8;
    fn ff_put_object(type_id: u32, ptr: *const u8, len: u32);
}


const BUFFER_SIZE: u32 = 8192;
static mut BYTE_BUF: [u8; BUFFER_SIZE as usize] = [0u8; BUFFER_SIZE as usize];


pub trait Entity: Sized + PartialEq + Eq {
    const TYPE_ID: u32;

    fn id(&self) -> u64;

    fn from_buffer(buf: &[u8]) -> Self;

    fn to_buffer(&self, buf: &mut [u8]) -> u32;

    fn load(id: u64) -> Self {
        unsafe {
            let ptr = ff_get_object(Self::TYPE_ID, id.to_le_bytes().as_ptr());
            Self::from_buffer(&BYTE_BUF)
        }
    }

    fn save(&self) {
        unsafe {
            let len = self.to_buffer(&mut BYTE_BUF);
            ff_put_object(Self::TYPE_ID, BYTE_BUF.as_ptr(), len)
        }
    }
}
