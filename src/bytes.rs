use crate::types::Word;

use std::convert::TryFrom;
use std::{io, mem};

const WORD_SIZE: usize = mem::size_of::<Word>();

pub fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "The provided buffer is not big enough!")
}

pub fn store_bytes<'a>(mut buf: &'a mut [u8], bytes: &[u8]) -> io::Result<(usize, &'a mut [u8])> {
    let len = (bytes.len() as Word).to_be_bytes();
    let pad = bytes.len() % WORD_SIZE;
    let pad = if pad == 0 { 0 } else { WORD_SIZE - pad };

    if buf.len() < WORD_SIZE + bytes.len() + pad {
        return Err(eof());
    }

    buf[..WORD_SIZE].copy_from_slice(&len);
    buf = &mut buf[WORD_SIZE..];

    buf[..bytes.len()].copy_from_slice(bytes);
    buf = &mut buf[bytes.len()..];

    for i in &mut buf[..pad] {
        *i = 0
    }
    buf = &mut buf[pad..];

    Ok((WORD_SIZE + bytes.len() + pad, buf))
}

pub fn store_raw_bytes<'a>(mut buf: &'a mut [u8], bytes: &[u8]) -> io::Result<(usize, &'a mut [u8])> {
    let pad = bytes.len() % WORD_SIZE;
    let pad = if pad == 0 { 0 } else { WORD_SIZE - pad };
    if buf.len() < bytes.len() + pad {
        return Err(eof());
    }

    buf[..bytes.len()].copy_from_slice(bytes);
    buf = &mut buf[bytes.len()..];

    for i in &mut buf[..pad] {
        *i = 0
    }
    buf = &mut buf[pad..];

    Ok((bytes.len() + pad, buf))
}

pub fn restore_bytes(mut buf: &[u8]) -> io::Result<(usize, Vec<u8>, &[u8])> {
    let len = buf
        .chunks_exact(WORD_SIZE)
        .next()
        .map(|chunk| <[u8; WORD_SIZE]>::try_from(chunk).unwrap_or_else(|_| unreachable!()))
        .map(|len| Word::from_be_bytes(len) as usize)
        .ok_or(eof())?;
    buf = &buf[WORD_SIZE..];

    let pad = len % WORD_SIZE;
    let pad = if pad == 0 { 0 } else { WORD_SIZE - pad };
    if buf.len() < len + pad {
        return Err(eof());
    }

    let data = Vec::from(&buf[..len]);
    let buf = &buf[len + pad..];

    Ok((WORD_SIZE + len + pad, data, buf))
}

pub fn restore_raw_bytes(buf: &[u8], len: usize) -> io::Result<(usize, Vec<u8>, &[u8])> {
    let pad = len % WORD_SIZE;
    let pad = if pad == 0 { 0 } else { WORD_SIZE - pad };
    if buf.len() < len + pad {
        return Err(eof());
    }

    let data = Vec::from(&buf[..len]);
    let buf = &buf[len + pad..];

    Ok((len + pad, data, buf))
}

pub fn store_number<T>(buf: &mut [u8], number: T) -> io::Result<(usize, &mut [u8])>
where
    T: Into<Word>,
{
    buf.chunks_exact_mut(WORD_SIZE)
        .next()
        .map(|chunk| chunk.copy_from_slice(&number.into().to_be_bytes()))
        .ok_or(eof())?;

    Ok((WORD_SIZE, &mut buf[WORD_SIZE..]))
}

pub fn store_number_unchecked<T>(buf: &mut [u8], number: T) -> &mut [u8]
where
    T: Into<Word>,
{
    buf[..WORD_SIZE].copy_from_slice(&number.into().to_be_bytes());

    &mut buf[WORD_SIZE..]
}

pub fn restore_number_unchecked<T>(buf: &[u8]) -> (T, &[u8])
where
    T: From<Word>,
{
    let number = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
    let number = Word::from_be_bytes(number).into();

    (number, &buf[WORD_SIZE..])
}

pub fn restore_u8_unchecked(buf: &[u8]) -> (u8, &[u8]) {
    let number = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
    let number = Word::from_be_bytes(number) as u8;

    (number, &buf[WORD_SIZE..])
}

pub fn restore_u16_unchecked(buf: &[u8]) -> (u16, &[u8]) {
    let number = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
    let number = Word::from_be_bytes(number) as u16;

    (number, &buf[WORD_SIZE..])
}

pub fn restore_u32_unchecked(buf: &[u8]) -> (u32, &[u8]) {
    let number = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
    let number = Word::from_be_bytes(number) as u32;

    (number, &buf[WORD_SIZE..])
}

pub fn restore_usize_unchecked(buf: &[u8]) -> (usize, &[u8]) {
    let number = <[u8; WORD_SIZE]>::try_from(&buf[..WORD_SIZE]).unwrap_or_else(|_| unreachable!());
    let number = Word::from_be_bytes(number) as usize;

    (number, &buf[WORD_SIZE..])
}

pub fn restore_number<T>(buf: &[u8]) -> io::Result<(T, &[u8])>
where
    T: From<Word>,
{
    let number = buf
        .chunks_exact(WORD_SIZE)
        .next()
        .map(|chunk| <[u8; WORD_SIZE]>::try_from(chunk).unwrap_or_else(|_| unreachable!()))
        .map(|chunk| Word::from_be_bytes(chunk).into())
        .ok_or(eof())?;

    Ok((number, &buf[WORD_SIZE..]))
}

pub fn store_array<'a, const N: usize>(buf: &'a mut [u8], array: &[u8; N]) -> io::Result<&'a mut [u8]> {
    buf.chunks_exact_mut(N)
        .next()
        .map(|chunk| chunk.copy_from_slice(array))
        .ok_or(eof())?;

    Ok(&mut buf[N..])
}

pub fn store_array_unchecked<'a, const N: usize>(buf: &'a mut [u8], array: &[u8; N]) -> &'a mut [u8] {
    buf[..N].copy_from_slice(array);

    &mut buf[N..]
}

pub fn restore_array_unchecked<const N: usize>(buf: &[u8]) -> ([u8; N], &[u8]) {
    <[u8; N]>::try_from(&buf[..N])
        .map(|array| (array, &buf[N..]))
        .unwrap_or_else(|_| unreachable!())
}

pub fn restore_array<const N: usize>(buf: &[u8]) -> io::Result<([u8; N], &[u8])> {
    <[u8; N]>::try_from(&buf[..N])
        .map_err(|_| eof())
        .map(|array| (array, &buf[N..]))
}
