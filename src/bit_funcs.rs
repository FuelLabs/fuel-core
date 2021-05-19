pub fn set_bits_in_u32(v: u32, toset: u32, left: u8, len: u8) -> u32 {
    if len == 0 {
        return v;
    }
    if left + len > 32 {
        return set_bits_in_u32(v, toset, left, 32 - left);
    }
    // need to clear bits with an AND first
    let new_v = !(((1 << len) - 1) << (32 - left - len)) & v;
    // then set bits with an OR
    new_v | (toset << (32 - left - len))
}

pub fn from_u32_to_u8_recurse(v: u32, left: u8, len: u8) -> u32 {
    if (left + len) > 32 {
        // constrain len to ensure correct shifts
        return from_u32_to_u8_recurse(v, left, 32 - left);
    }

    if left == 0 && len == 32 {
        // to prevent overflow on left-shift below
        return v;
    }
    (v >> (32 - left - len))
        & (
            // can overflow when len = 64
            (1 << len) - 1
        )
}

pub fn set_bits_in_u64(v: u64, toset: u64, left: u8, len: u8) -> u64 {
    if len == 0 {
        return v;
    }
    if left + len > 64 {
        return set_bits_in_u64(v, toset, left, 64 - left);
    }
    // need to clear bits with an AND first
    let new_v = !(((1 << len) - 1) << (64 - left - len)) & v;
    // then set bits with an OR
    new_v | (toset << (64 - left - len))
}

pub fn from_u64_to_u8(v: u64, left: u8, len: u8) -> u64 {
    let mut local_len: u8 = len;
    if (left + len) > 64 {
        // constrain len to ensure correct shifts
        local_len = 64 - left
        // alternative architecture: instead of local_len var, recurse with new len
    }

    if left == 0 && local_len == 64 {
        // to prevent overflow on left-shift below
        return v;
    }
    (v >> (64 - left - local_len))
        & (
            // can overflow when len = 64
            (1 << local_len) - 1
        )
}

pub fn from_u64_to_u8_recurse(v: u64, left: u8, len: u8) -> u64 {
    if (left + len) > 64 {
        // constrain len to ensure correct shifts
        return from_u64_to_u8_recurse(v, left, 64 - left);
    }

    if left == 0 && len == 64 {
        // to prevent overflow on left-shift below
        return v;
    }
    (v >> (64 - left - len))
        & (
            // can overflow when len = 64
            (1 << len) - 1
        )
}

pub fn transform_from_u8_to_u32(input: &Vec<u8>) -> Vec<u32> {
    if input.len() % 4 != 0 {
        panic!("Insufficient u8 bytes to form u32");
    }
    let mut out: Vec<u32> = Vec::new();
    for b in 0..input.len() / 4 {
        // println!("input[0]: {}, {:08b}", (input[(b * 4) as usize] ), (input[(b * 4) as usize] ));
        // println!("input[1]: {:08b}", (input[((b * 4) + 1) as usize] ));
        // println!("input[2]: {:08b}", (input[((b * 4) + 2) as usize] ) as u8);
        // println!("input[3]: {:08b}", (input[((b * 4) + 3) as usize] as u8));

        let mut op: u32 = (input[(b * 4) as usize] as u32) << 24;
        op = op | (input[((b * 4) + 1) as usize] as u32) << 16 as u32;
        op = op | (input[((b * 4) + 2) as usize] as u32) << 8 as u32;
        op = op | (input[((b * 4) + 3) as usize] as u32);

        println!("op: {:032b}", op);
        out.push(op);
    }
    out
}

pub fn transform_from_u32_to_u8(input: &Vec<u32>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for b in 0..input.len() {
        // println!("op: {:032b}", input[b] as u32);
        out.push((input[b] >> 24) as u8 & u8::MAX);
        out.push((input[b] >> 16) as u8 & u8::MAX);
        out.push((input[b] >> 8) as u8 & u8::MAX);
        out.push(input[b] as u8 & u8::MAX);
    }
    // println!("vec: {:?}", out);
    out
}

pub fn transform_from_u32_to_u64(input: &Vec<u32>) -> Vec<u64> {
    if input.len() % 2 != 0 {
        panic!("Insufficient u32 bytes to form u64");
    }
    let mut out: Vec<u64> = Vec::new();
    for b in 0..input.len() / 2 {
        let mut op: u64 = (input[(b * 2) as usize] as u64) << 32 as u64;
        op = op | input[((b * 2) + 1) as usize] as u64;
        out.push(op);
    }
    out
}

pub fn transform_from_u64_to_u32(input: &Vec<u64>) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for b in 0..input.len() {
        out.push((input[b] >> 32) as u32 & u32::MAX);
        out.push(input[b] as u32 & u32::MAX as u32);
    }
    out
}

pub fn transform_from_u64_to_u8(input: &Vec<u64>) -> Vec<u8> {
    transform_from_u32_to_u8(&transform_from_u64_to_u32(input))
}

pub fn transform_from_u8_to_u64(input: &Vec<u8>) -> Vec<u64> {
    transform_from_u32_to_u64(&transform_from_u8_to_u32(input))
}
