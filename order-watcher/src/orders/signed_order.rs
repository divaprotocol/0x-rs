use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use web3::types::{Address, Recovery, RecoveryMessage, H256};

use super::{Error, LimitOrder, SignatureType};
use crate::{ethereum::ChainInfo, require, utils::recover};

const ETH_SIGN_PREFIX: &[u8] = b"\x19Ethereum Signed Message:\n32";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub signature_type: SignatureType,
    pub v:              u8,
    pub r:              H256,
    pub s:              H256,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedOrder {
    #[serde(flatten)]
    pub order:     LimitOrder,
    pub signature: Signature,
}

impl Signature {
    /// Recover the signer from a signature
    /// See <https://github.com/0xProject/protocol/blob/835ee4e8/contracts/zero-ex/contracts/src/features/libs/LibSignature.sol#L67>
    pub fn recover(&self, hash: &H256) -> Option<Address> {
        let hash = match self.signature_type {
            SignatureType::EIP712 => *hash,
            SignatureType::EthSign => {
                let mut hasher = Keccak256::new();
                hasher.update(ETH_SIGN_PREFIX);
                hasher.update(hash);
                H256::from(<[u8; 32]>::from(hasher.finalize()))
            }
        };
        let recovery = Recovery {
            message: RecoveryMessage::Hash(hash),
            v:       self.v.into(),
            r:       self.r,
            s:       self.s,
        };
        let (signature, recovery_id) = recovery.as_signature()?;
        recover(hash.as_bytes(), &signature, recovery_id).ok()
    }
}

impl SignedOrder {
    #[allow(dead_code)]
    pub fn hash(&self) -> H256 {
        self.order.hash()
    }

    pub fn validate(&self, chain: &ChainInfo) -> Result<(), Error> {
        self.order.validate(chain)?;
        self.validate_signature()?;
        Ok(())
    }

    pub fn validate_signature(&self) -> Result<(), Error> {
        let hash = self.order.hash();
        let maker = self
            .signature
            .recover(&hash)
            .ok_or(Error::InvalidSignature)?;
        require!(self.order.maker == maker, Error::InvalidSignature);
        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    use serde_json::{from_value, json};

    use super::*;

    #[test]
    fn test_json_order() {
        // Example from <https://0x.org/docs/api#request-6>
        let json = json!({
            "makerToken": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "takerToken": "0xe41d2489571d322189246dafa5ebde1f4699f498",
            "makerAmount": "100000000000000",
            "takerAmount": "2000000000000000000000",
            "maker": "0x56EB0aD2dC746540Fab5C02478B31e2AA9DdC38C",
            "taker": "0x0000000000000000000000000000000000000000",
            "pool": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "expiry": "1614956256",
            "salt": "2752094376750492926844965905320507011598275560670346196138937898764349624882",
            "chainId": 1,
            "verifyingContract": "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
            "takerTokenFeeAmount": "0",
            "sender": "0x0000000000000000000000000000000000000000",
            "feeRecipient": "0x0000000000000000000000000000000000000000",
            "signature": {
                "v": 27,
                "r": "0x983a8a8dad663124a52609fe9aa82737f7f02d12ed951785f36b50906041794d",
                "s": "0x5f18ae837be4732bcb3dd019104cf775f92b8740b275be510462a7aa62cdf252",
                "signatureType": 3
            }
        });
        let signed_order = from_value::<SignedOrder>(json).unwrap();
        signed_order.validate(&ChainInfo::default()).unwrap();
    }
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::{black_box, Criterion};
    use serde_json::{from_value, json};

    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn group(criterion: &mut Criterion) {
        bench_validate(criterion);
        bench_validate_signature(criterion);
        bench_recover(criterion);
    }

    fn example_chain() -> ChainInfo {
        ChainInfo {
            ..ChainInfo::default()
        }
    }

    fn example_order() -> SignedOrder {
        let json = json!({
            "makerToken": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "takerToken": "0xe41d2489571d322189246dafa5ebde1f4699f498",
            "makerAmount": "100000000000000",
            "takerAmount": "2000000000000000000000",
            "maker": "0x56EB0aD2dC746540Fab5C02478B31e2AA9DdC38C",
            "taker": "0x0000000000000000000000000000000000000000",
            "pool": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "expiry": "1614956256",
            "salt": "2752094376750492926844965905320507011598275560670346196138937898764349624882",
            "chainId": 1,
            "verifyingContract": "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
            "takerTokenFeeAmount": "0",
            "sender": "0x0000000000000000000000000000000000000000",
            "feeRecipient": "0x0000000000000000000000000000000000000000",
            "signature": {
                "v": 27,
                "r": "0x983a8a8dad663124a52609fe9aa82737f7f02d12ed951785f36b50906041794d",
                "s": "0x5f18ae837be4732bcb3dd019104cf775f92b8740b275be510462a7aa62cdf252",
                "signatureType": 3
            }
        });
        from_value::<SignedOrder>(json).unwrap()
    }

    fn bench_validate(criterion: &mut Criterion) {
        let chain = example_chain();
        let order = example_order();
        criterion.bench_function("signed_order_validate", move |bencher| {
            bencher.iter(|| black_box(black_box(order).validate(&chain)));
        });
    }

    fn bench_validate_signature(criterion: &mut Criterion) {
        let order = example_order();
        criterion.bench_function("signed_order_validate_signature", move |bencher| {
            bencher.iter(|| black_box(black_box(order).validate_signature()));
        });
    }

    fn bench_recover(criterion: &mut Criterion) {
        let order = example_order();
        let signature = order.signature;
        let hash = order.hash();
        criterion.bench_function("signature_recover", move |bencher| {
            bencher.iter(|| black_box(black_box(signature).recover(&hash)));
        });
    }
}
