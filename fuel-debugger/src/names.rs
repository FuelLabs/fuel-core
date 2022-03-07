use phf::phf_map;

pub static REGISTERS: phf::Map<&'static str, usize> = phf_map! {
    "zero" => 0x00,
    "one" => 0x01,
    "of" => 0x02,
    "pc" => 0x03,
    "ssp" => 0x04,
    "sp" => 0x05,
    "fp" => 0x06,
    "hp" => 0x07,
    "err" => 0x08,
    "ggas" => 0x09,
    "cgas" => 0x0a,
    "bal" => 0x0b,
    "is" => 0x0c,
    "ret" => 0x0d,
    "retl" => 0x0e,
    "flag" => 0x0f
};
