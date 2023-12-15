use diesel::deserialize::FromSql;
use diesel::serialize::ToSql;
use diesel_derives::{AsExpression, FromSqlRow};
use hex;
use num_bigint;
use serde::{self, Deserialize, Serialize};
use stable_hash::utils::AsInt;
use stable_hash::{FieldAddress, StableHash};
use stable_hash_legacy::SequenceNumber;
use thiserror::Error;
use web3::types::*;

use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::ops::{Add, BitAnd, BitOr, Deref, Div, Mul, Rem, Shl, Shr, Sub};
use std::str::FromStr;

pub use num_bigint::Sign as BigIntSign;

use crate::blockchain::BlockHash;
use crate::runtime::gas::{Gas, GasSizeOf, SaturatingInto};
use crate::util::stable_hash_glue::{impl_stable_hash, AsBytes};

/// All operations on `BigDecimal` return a normalized value.
// Caveat: The exponent is currently an i64 and may overflow. See
// https://github.com/akubera/bigdecimal-rs/issues/54.
// Using `#[serde(from = "BigDecimal"]` makes sure deserialization calls `BigDecimal::new()`.
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, AsExpression, FromSqlRow,
)]
#[serde(from = "bigdecimal::BigDecimal")]
#[sql_type = "diesel::sql_types::Numeric"]
pub struct BigDecimal(bigdecimal::BigDecimal);

impl From<bigdecimal::BigDecimal> for BigDecimal {
    fn from(big_decimal: bigdecimal::BigDecimal) -> Self {
        BigDecimal(big_decimal).normalized()
    }
}

impl BigDecimal {
    /// These are the limits of IEEE-754 decimal128, a format we may want to switch to. See
    /// https://en.wikipedia.org/wiki/Decimal128_floating-point_format.
    pub const MIN_EXP: i32 = -6143;
    pub const MAX_EXP: i32 = 6144;
    pub const MAX_SIGNFICANT_DIGITS: i32 = 34;

    pub fn new(digits: BigInt, exp: i64) -> Self {
        // bigdecimal uses `scale` as the opposite of the power of ten, so negate `exp`.
        Self::from(bigdecimal::BigDecimal::new(digits.inner(), -exp))
    }

    pub fn parse_bytes(bytes: &[u8]) -> Option<Self> {
        bigdecimal::BigDecimal::parse_bytes(bytes, 10).map(Self)
    }

    pub fn zero() -> BigDecimal {
        use bigdecimal::Zero;

        BigDecimal(bigdecimal::BigDecimal::zero())
    }

    pub fn as_bigint_and_exponent(&self) -> (num_bigint::BigInt, i64) {
        self.0.as_bigint_and_exponent()
    }

    pub fn digits(&self) -> u64 {
        self.0.digits()
    }

    // Copy-pasted from `bigdecimal::BigDecimal::normalize`. We can use the upstream version once it
    // is included in a released version supported by Diesel.
    #[must_use]
    pub fn normalized(&self) -> BigDecimal {
        if self == &BigDecimal::zero() {
            return BigDecimal::zero();
        }

        // Round to the maximum significant digits.
        let big_decimal = self.0.with_prec(Self::MAX_SIGNFICANT_DIGITS as u64);

        let (bigint, exp) = big_decimal.as_bigint_and_exponent();
        let (sign, mut digits) = bigint.to_radix_be(10);
        let trailing_count = digits.iter().rev().take_while(|i| **i == 0).count();
        digits.truncate(digits.len() - trailing_count);
        let int_val = num_bigint::BigInt::from_radix_be(sign, &digits, 10).unwrap();
        let scale = exp - trailing_count as i64;

        BigDecimal(bigdecimal::BigDecimal::new(int_val, scale))
    }
}

impl Display for BigDecimal {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

impl fmt::Debug for BigDecimal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BigDecimal({})", self.0)
    }
}

impl FromStr for BigDecimal {
    type Err = <bigdecimal::BigDecimal as FromStr>::Err;

    fn from_str(s: &str) -> Result<BigDecimal, Self::Err> {
        Ok(Self::from(bigdecimal::BigDecimal::from_str(s)?))
    }
}

impl From<i32> for BigDecimal {
    fn from(n: i32) -> Self {
        Self::from(bigdecimal::BigDecimal::from(n))
    }
}

impl From<i64> for BigDecimal {
    fn from(n: i64) -> Self {
        Self::from(bigdecimal::BigDecimal::from(n))
    }
}

impl From<u64> for BigDecimal {
    fn from(n: u64) -> Self {
        Self::from(bigdecimal::BigDecimal::from(n))
    }
}

impl From<f64> for BigDecimal {
    fn from(n: f64) -> Self {
        Self::from(bigdecimal::BigDecimal::from(n))
    }
}

impl Add for BigDecimal {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::from(self.0.add(other.0))
    }
}

impl Sub for BigDecimal {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::from(self.0.sub(other.0))
    }
}

impl Mul for BigDecimal {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::from(self.0.mul(other.0))
    }
}

impl Div for BigDecimal {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        if other == BigDecimal::from(0) {
            panic!("Cannot divide by zero-valued `BigDecimal`!")
        }

        Self::from(self.0.div(other.0))
    }
}

// Used only for JSONB support
impl ToSql<diesel::sql_types::Numeric, diesel::pg::Pg> for BigDecimal {
    fn to_sql<W: Write>(
        &self,
        out: &mut diesel::serialize::Output<W, diesel::pg::Pg>,
    ) -> diesel::serialize::Result {
        <_ as ToSql<diesel::sql_types::Numeric, _>>::to_sql(&self.0, out)
    }
}

impl FromSql<diesel::sql_types::Numeric, diesel::pg::Pg> for BigDecimal {
    fn from_sql(
        bytes: Option<&<diesel::pg::Pg as diesel::backend::Backend>::RawValue>,
    ) -> diesel::deserialize::Result<Self> {
        Ok(Self::from(bigdecimal::BigDecimal::from_sql(bytes)?))
    }
}

impl bigdecimal::ToPrimitive for BigDecimal {
    fn to_i64(&self) -> Option<i64> {
        self.0.to_i64()
    }
    fn to_u64(&self) -> Option<u64> {
        self.0.to_u64()
    }
}

impl stable_hash_legacy::StableHash for BigDecimal {
    fn stable_hash<H: stable_hash_legacy::StableHasher>(
        &self,
        mut sequence_number: H::Seq,
        state: &mut H,
    ) {
        let (int, exp) = self.as_bigint_and_exponent();
        // This only allows for backward compatible changes between
        // BigDecimal and unsigned ints
        stable_hash_legacy::StableHash::stable_hash(&exp, sequence_number.next_child(), state);
        stable_hash_legacy::StableHash::stable_hash(
            &BigInt::unchecked_new(int),
            sequence_number,
            state,
        );
    }
}

impl StableHash for BigDecimal {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        // This implementation allows for backward compatible changes from integers (signed or unsigned)
        // when the exponent is zero.
        let (int, exp) = self.as_bigint_and_exponent();
        StableHash::stable_hash(&exp, field_address.child(1), state);
        // Normally it would be a red flag to pass field_address in after having used a child slot.
        // But, we know the implemecntation of StableHash for BigInt will not use child(1) and that
        // it will not in the future due to having no forward schema evolutions for ints and the
        // stability guarantee.
        //
        // For reference, ints use child(0) for the sign and write the little endian bytes to the parent slot.
        BigInt::unchecked_new(int).stable_hash(field_address, state);
    }
}

impl GasSizeOf for BigDecimal {
    fn gas_size_of(&self) -> Gas {
        let (int, _) = self.as_bigint_and_exponent();
        BigInt::unchecked_new(int).gas_size_of()
    }
}

// Use a private module to ensure a constructor is used.
pub use big_int::BigInt;
mod big_int {
    use std::{
        f32::consts::LOG2_10,
        fmt::{self, Display, Formatter},
    };

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct BigInt(num_bigint::BigInt);

    impl Display for BigInt {
        fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
            self.0.fmt(f)
        }
    }

    impl BigInt {
        // Postgres `numeric` has a limit documented here [https://www.postgresql.org/docs/current/datatype-numeric.htm]:
        // "Up to 131072 digits before the decimal point; up to 16383 digits after the decimal point"
        // So based on this we adopt a limit of 131072 decimal digits for big int, converted here to bits.
        pub const MAX_BITS: u32 = (131072.0 * LOG2_10) as u32 + 1; // 435_412

        pub fn new(inner: num_bigint::BigInt) -> Result<Self, anyhow::Error> {
            // `inner.bits()` won't include the sign bit, so we add 1 to account for it.
            let bits = inner.bits() + 1;
            if bits > Self::MAX_BITS as usize {
                anyhow::bail!(
                    "BigInt is too big, total bits {} (max {})",
                    bits,
                    Self::MAX_BITS
                );
            }
            Ok(Self(inner))
        }

        /// Creates a BigInt without checking the digit limit.
        pub(super) fn unchecked_new(inner: num_bigint::BigInt) -> Self {
            Self(inner)
        }

        pub fn sign(&self) -> num_bigint::Sign {
            self.0.sign()
        }

        pub fn to_bytes_le(&self) -> (super::BigIntSign, Vec<u8>) {
            self.0.to_bytes_le()
        }

        pub fn to_bytes_be(&self) -> (super::BigIntSign, Vec<u8>) {
            self.0.to_bytes_be()
        }

        pub fn to_signed_bytes_le(&self) -> Vec<u8> {
            self.0.to_signed_bytes_le()
        }

        pub fn bits(&self) -> usize {
            self.0.bits()
        }

        pub(super) fn inner(self) -> num_bigint::BigInt {
            self.0
        }
    }
}

impl stable_hash_legacy::StableHash for BigInt {
    #[inline]
    fn stable_hash<H: stable_hash_legacy::StableHasher>(
        &self,
        sequence_number: H::Seq,
        state: &mut H,
    ) {
        stable_hash_legacy::utils::AsInt {
            is_negative: self.sign() == BigIntSign::Minus,
            little_endian: &self.to_bytes_le().1,
        }
        .stable_hash(sequence_number, state)
    }
}

impl StableHash for BigInt {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        AsInt {
            is_negative: self.sign() == BigIntSign::Minus,
            little_endian: &self.to_bytes_le().1,
        }
        .stable_hash(field_address, state)
    }
}

#[derive(Error, Debug)]
pub enum BigIntOutOfRangeError {
    #[error("Cannot convert negative BigInt into type")]
    Negative,
    #[error("BigInt value is too large for type")]
    Overflow,
}

impl<'a> TryFrom<&'a BigInt> for u64 {
    type Error = BigIntOutOfRangeError;
    fn try_from(value: &'a BigInt) -> Result<u64, BigIntOutOfRangeError> {
        let (sign, bytes) = value.to_bytes_le();

        if sign == num_bigint::Sign::Minus {
            return Err(BigIntOutOfRangeError::Negative);
        }

        if bytes.len() > 8 {
            return Err(BigIntOutOfRangeError::Overflow);
        }

        // Replace this with u64::from_le_bytes when stabilized
        let mut n = 0u64;
        let mut shift_dist = 0;
        for b in bytes {
            n |= (b as u64) << shift_dist;
            shift_dist += 8;
        }
        Ok(n)
    }
}

impl TryFrom<BigInt> for u64 {
    type Error = BigIntOutOfRangeError;
    fn try_from(value: BigInt) -> Result<u64, BigIntOutOfRangeError> {
        (&value).try_into()
    }
}

impl fmt::Debug for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BigInt({})", self)
    }
}

impl BigInt {
    pub fn from_unsigned_bytes_le(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        BigInt::new(num_bigint::BigInt::from_bytes_le(
            num_bigint::Sign::Plus,
            bytes,
        ))
    }

    pub fn from_signed_bytes_le(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        BigInt::new(num_bigint::BigInt::from_signed_bytes_le(bytes))
    }

    pub fn from_signed_bytes_be(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        BigInt::new(num_bigint::BigInt::from_signed_bytes_be(bytes))
    }

    /// Deprecated. Use try_into instead
    pub fn to_u64(&self) -> u64 {
        self.try_into().unwrap()
    }

    pub fn from_unsigned_u128(n: U128) -> Self {
        let mut bytes: [u8; 16] = [0; 16];
        n.to_little_endian(&mut bytes);
        // Unwrap: 128 bits is much less than BigInt::MAX_BITS
        BigInt::from_unsigned_bytes_le(&bytes).unwrap()
    }

    pub fn from_unsigned_u256(n: &U256) -> Self {
        let mut bytes: [u8; 32] = [0; 32];
        n.to_little_endian(&mut bytes);
        // Unwrap: 256 bits is much less than BigInt::MAX_BITS
        BigInt::from_unsigned_bytes_le(&bytes).unwrap()
    }

    pub fn from_signed_u256(n: &U256) -> Self {
        let mut bytes: [u8; 32] = [0; 32];
        n.to_little_endian(&mut bytes);
        BigInt::from_signed_bytes_le(&bytes).unwrap()
    }

    pub fn to_signed_u256(&self) -> U256 {
        let bytes = self.to_signed_bytes_le();
        if self < &BigInt::from(0) {
            assert!(
                bytes.len() <= 32,
                "BigInt value does not fit into signed U256"
            );
            let mut i_bytes: [u8; 32] = [255; 32];
            i_bytes[..bytes.len()].copy_from_slice(&bytes);
            U256::from_little_endian(&i_bytes)
        } else {
            U256::from_little_endian(&bytes)
        }
    }

    pub fn to_unsigned_u256(&self) -> U256 {
        let (sign, bytes) = self.to_bytes_le();
        assert!(
            sign == BigIntSign::NoSign || sign == BigIntSign::Plus,
            "negative value encountered for U256: {}",
            self
        );
        U256::from_little_endian(&bytes)
    }

    pub fn pow(self, exponent: u8) -> Result<BigInt, anyhow::Error> {
        use num_traits::pow::Pow;

        BigInt::new(self.inner().pow(&exponent))
    }
}

impl From<i32> for BigInt {
    fn from(i: i32) -> BigInt {
        BigInt::unchecked_new(i.into())
    }
}

impl From<u64> for BigInt {
    fn from(i: u64) -> BigInt {
        BigInt::unchecked_new(i.into())
    }
}

impl From<i64> for BigInt {
    fn from(i: i64) -> BigInt {
        BigInt::unchecked_new(i.into())
    }
}

impl From<U64> for BigInt {
    /// This implementation assumes that U64 represents an unsigned U64,
    /// and not a signed U64 (aka int64 in Solidity). Right now, this is
    /// all we need (for block numbers). If it ever becomes necessary to
    /// handle signed U64s, we should add the same
    /// `{to,from}_{signed,unsigned}_u64` methods that we have for U64.
    fn from(n: U64) -> BigInt {
        BigInt::from(n.as_u64())
    }
}

impl FromStr for BigInt {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<BigInt, Self::Err> {
        num_bigint::BigInt::from_str(s)
            .map_err(anyhow::Error::from)
            .and_then(BigInt::new)
    }
}

impl Serialize for BigInt {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BigInt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let decimal_string = <String>::deserialize(deserializer)?;
        BigInt::from_str(&decimal_string).map_err(D::Error::custom)
    }
}

impl Add for BigInt {
    type Output = BigInt;

    fn add(self, other: BigInt) -> BigInt {
        BigInt::unchecked_new(self.inner().add(other.inner()))
    }
}

impl Sub for BigInt {
    type Output = BigInt;

    fn sub(self, other: BigInt) -> BigInt {
        BigInt::unchecked_new(self.inner().sub(other.inner()))
    }
}

impl Mul for BigInt {
    type Output = BigInt;

    fn mul(self, other: BigInt) -> BigInt {
        BigInt::unchecked_new(self.inner().mul(other.inner()))
    }
}

impl Div for BigInt {
    type Output = BigInt;

    fn div(self, other: BigInt) -> BigInt {
        if other == BigInt::from(0) {
            panic!("Cannot divide by zero-valued `BigInt`!")
        }

        BigInt::unchecked_new(self.inner().div(other.inner()))
    }
}

impl Rem for BigInt {
    type Output = BigInt;

    fn rem(self, other: BigInt) -> BigInt {
        BigInt::unchecked_new(self.inner().rem(other.inner()))
    }
}

impl BitOr for BigInt {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        BigInt::unchecked_new(self.inner().bitor(other.inner()))
    }
}

impl BitAnd for BigInt {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        BigInt::unchecked_new(self.inner().bitand(other.inner()))
    }
}

impl Shl<u8> for BigInt {
    type Output = Self;

    fn shl(self, bits: u8) -> Self {
        BigInt::unchecked_new(self.inner().shl(bits.into()))
    }
}

impl Shr<u8> for BigInt {
    type Output = Self;

    fn shr(self, bits: u8) -> Self {
        BigInt::unchecked_new(self.inner().shr(bits.into()))
    }
}

impl GasSizeOf for BigInt {
    fn gas_size_of(&self) -> Gas {
        // Add one to always have an upper bound on the number of bytes required to represent the
        // number, and so that `0` has a size of 1.
        let n_bytes = self.bits() / 8 + 1;
        n_bytes.saturating_into()
    }
}

/// A byte array that's serialized as a hex string prefixed by `0x`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bytes(Box<[u8]>);

impl Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Bytes(0x{})", hex::encode(&self.0))
    }
}

impl_stable_hash!(Bytes(transparent: AsBytes));

impl Bytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Display for Bytes {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl FromStr for Bytes {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Bytes, Self::Err> {
        hex::decode(s.trim_start_matches("0x")).map(|x| Bytes(x.into()))
    }
}

impl<'a> From<&'a [u8]> for Bytes {
    fn from(array: &[u8]) -> Self {
        Bytes(array.into())
    }
}

impl From<Address> for Bytes {
    fn from(address: Address) -> Bytes {
        Bytes::from(address.as_ref())
    }
}

impl From<web3::types::Bytes> for Bytes {
    fn from(bytes: web3::types::Bytes) -> Bytes {
        Bytes::from(bytes.0.as_slice())
    }
}

impl From<BlockHash> for Bytes {
    fn from(hash: BlockHash) -> Self {
        Bytes(hash.0)
    }
}

impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let hex_string = <String>::deserialize(deserializer)?;
        Bytes::from_str(&hex_string).map_err(D::Error::custom)
    }
}

impl<const N: usize> From<[u8; N]> for Bytes {
    fn from(array: [u8; N]) -> Bytes {
        Bytes(array.into())
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(vec: Vec<u8>) -> Self {
        Bytes(vec.into())
    }
}

impl ToSql<diesel::sql_types::Binary, diesel::pg::Pg> for Bytes {
    fn to_sql<W: Write>(
        &self,
        out: &mut diesel::serialize::Output<W, diesel::pg::Pg>,
    ) -> diesel::serialize::Result {
        <_ as ToSql<diesel::sql_types::Binary, _>>::to_sql(self.as_slice(), out)
    }
}

#[cfg(test)]
mod test {
    use super::{BigDecimal, BigInt, Bytes};
    use stable_hash_legacy::crypto::SetHasher;
    use stable_hash_legacy::prelude::*;
    use stable_hash_legacy::utils::stable_hash;
    use std::str::FromStr;
    use web3::types::U64;

    #[test]
    fn bigint_to_from_u64() {
        for n in 0..100 {
            let u = U64::from(n);
            let bn = BigInt::from(u);
            assert_eq!(n, bn.to_u64());
        }
    }

    fn crypto_stable_hash(value: impl StableHash) -> <SetHasher as StableHasher>::Out {
        stable_hash::<SetHasher, _>(&value)
    }

    fn same_stable_hash(left: impl StableHash, right: impl StableHash) {
        let left = crypto_stable_hash(left);
        let right = crypto_stable_hash(right);
        assert_eq!(left, right);
    }

    #[test]
    fn big_int_stable_hash_same_as_int() {
        same_stable_hash(0, BigInt::from(0u64));
        same_stable_hash(1, BigInt::from(1u64));
        same_stable_hash(1u64 << 20, BigInt::from(1u64 << 20));

        same_stable_hash(
            -1,
            BigInt::from_signed_bytes_le(&(-1i32).to_le_bytes()).unwrap(),
        );
    }

    #[test]
    fn big_decimal_stable_hash_same_as_uint() {
        same_stable_hash(0, BigDecimal::from(0u64));
        same_stable_hash(4, BigDecimal::from(4i64));
        same_stable_hash(1u64 << 21, BigDecimal::from(1u64 << 21));
    }

    #[test]
    fn big_decimal_stable() {
        let cases = vec![
            (
                "28b09c9c3f3e2fe037631b7fbccdf65c37594073016d8bf4bb0708b3fda8066a",
                "0.1",
            ),
            (
                "74fb39f038d2f1c8975740bf2651a5ac0403330ee7e9367f9563cbd7d21086bd",
                "-0.1",
            ),
            (
                "1d79e0476bc5d6fe6074fb54636b04fd3bc207053c767d9cb5e710ba5f002441",
                "198.98765544",
            ),
            (
                "e63f6ad2c65f193aa9eba18dd7e1043faa2d6183597ba84c67765aaa95c95351",
                "0.00000093937698",
            ),
            (
                "6b06b34cc714810072988dc46c493c66a6b6c2c2dd0030271aa3adf3b3f21c20",
                "98765587998098786876.0",
            ),
        ];
        for (hash, s) in cases.iter() {
            let dec = BigDecimal::from_str(s).unwrap();
            assert_eq!(*hash, hex::encode(crypto_stable_hash(dec)));
        }
    }

    #[test]
    fn test_normalize() {
        let vals = vec![
            (
                BigDecimal::new(BigInt::from(10), -2),
                BigDecimal(bigdecimal::BigDecimal::new(1.into(), 1)),
                "0.1",
            ),
            (
                BigDecimal::new(BigInt::from(132400), 4),
                BigDecimal(bigdecimal::BigDecimal::new(1324.into(), -6)),
                "1324000000",
            ),
            (
                BigDecimal::new(BigInt::from(1_900_000), -3),
                BigDecimal(bigdecimal::BigDecimal::new(19.into(), -2)),
                "1900",
            ),
            (BigDecimal::new(0.into(), 3), BigDecimal::zero(), "0"),
            (BigDecimal::new(0.into(), -5), BigDecimal::zero(), "0"),
        ];

        for (not_normalized, normalized, string) in vals {
            assert_eq!(not_normalized.normalized(), normalized);
            assert_eq!(not_normalized.normalized().to_string(), string);
            assert_eq!(normalized.to_string(), string);
        }
    }

    #[test]
    fn fmt_debug() {
        let bi = BigInt::from(-17);
        let bd = BigDecimal::new(bi.clone(), -2);
        let bytes = Bytes::from([222, 173, 190, 239].as_slice());
        assert_eq!("BigInt(-17)", format!("{:?}", bi));
        assert_eq!("BigDecimal(-0.17)", format!("{:?}", bd));
        assert_eq!("Bytes(0xdeadbeef)", format!("{:?}", bytes));
    }
}
