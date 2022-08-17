//! This is inlined from [`web3`][0] and optimized to benefit from
//! precomputed tables in a static context.
//!
//! See <https://github.com/tomusdrw/rust-web3/issues/534>
//!
//! [0]: https://docs.rs/web3/0.17.0/src/web3/signing.rs.html#123-149

use once_cell::sync::Lazy;
use secp256k1::{
    recovery::{RecoverableSignature, RecoveryId},
    Error, Message, Secp256k1, VerifyOnly,
};
use sha3::{Digest, Keccak256};
use web3::types::Address;

static CONTEXT: Lazy<Secp256k1<VerifyOnly>> = Lazy::new(Secp256k1::verification_only);

pub fn recover(message: &[u8], signature: &[u8], recovery_id: i32) -> Result<Address, Error> {
    // Recover public key
    let message = Message::from_slice(message)?;
    let recovery_id = RecoveryId::from_i32(recovery_id)?;
    let signature = RecoverableSignature::from_compact(signature, recovery_id)?;
    let public_key = CONTEXT.recover(&message, &signature)?;

    // Hash public key into address
    let public_key = public_key.serialize_uncompressed();
    debug_assert_eq!(public_key[0], 0x04);
    let hash = {
        let mut hasher = Keccak256::new();
        hasher.update(&public_key[1..]);
        hasher.finalize()
    };
    let address = Address::from_slice(&hash[12..]);
    Ok(address)
}

#[cfg(test)]
mod test {
    use hex_literal::hex;
    use pretty_assertions::assert_eq;
    use web3::signing::recover as ref_recover;

    use super::*;

    #[test]
    fn test_recover() {
        let message = &hex!("a143f0980eedefd04fbf259f8e0dab07c895d1a228c658db1d34d299cd4f1216");
        let signature = &hex!("983a8a8dad663124a52609fe9aa82737f7f02d12ed951785f36b50906041794d5f18ae837be4732bcb3dd019104cf775f92b8740b275be510462a7aa62cdf252");
        let recovery_id = 0;

        let result = recover(message, signature, recovery_id).unwrap();
        let expected = ref_recover(message, signature, recovery_id).unwrap();
        assert_eq!(result, expected);
    }
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::{black_box, Criterion};
    use hex_literal::hex;

    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn group(criterion: &mut Criterion) {
        bench_recover(criterion);
    }

    fn bench_recover(criterion: &mut Criterion) {
        let message = &hex!("a143f0980eedefd04fbf259f8e0dab07c895d1a228c658db1d34d299cd4f1216");
        let signature = &hex!("983a8a8dad663124a52609fe9aa82737f7f02d12ed951785f36b50906041794d5f18ae837be4732bcb3dd019104cf775f92b8740b275be510462a7aa62cdf252");
        let recovery_id = 0;

        criterion.bench_function("util_recover", move |bencher| {
            bencher.iter(|| {
                black_box(recover(
                    black_box(message),
                    black_box(signature),
                    recovery_id,
                ))
                .unwrap();
            });
        });
        criterion.bench_function("web3_recover", move |bencher| {
            bencher.iter(|| {
                black_box(web3::signing::recover(
                    black_box(message),
                    black_box(signature),
                    recovery_id,
                ))
                .unwrap();
            });
        });
    }
}
