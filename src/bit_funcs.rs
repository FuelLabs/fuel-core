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
    new_v | (
        toset << (32 - left - len)
    )
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
    (
        v >> (32 - left - len)
    ) & (
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
    new_v | (
        toset << (64 - left - len)
    )
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
    (
        v >> (64 - left - local_len)
    ) & (
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
    (
        v >> (64 - left - len)
    ) & (
        // can overflow when len = 64
        (1 << len) - 1
    )
}
