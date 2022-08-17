use hex_literal::hex;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use web3::types::{Address, H256, U128, U256};

use super::Error;
use crate::{
    ethereum::ChainInfo,
    require,
    utils::serde::{u128_dec, u256_dec, u64_dec},
};

// See tests for the pre-images
const DOMAIN_SEPARATOR_TYPE_HASH: [u8; 32] =
    hex!("8b73c3c69bb8fe3d512ecc4cf759cc79239f7b179b0ffacaa9a75d522b39400f");
const NAME_HASH: [u8; 32] =
    hex!("9e5dae0addaf20578aeb5d70341d092b53b4e14480ac5726438fd436df7ba427");
const VERSION_HASH: [u8; 32] =
    hex!("06c015bd22b4c69690933c1058878ebdfef31f9aaae40bbe86d8a09fe1b2972c");
const TYPE_HASH: [u8; 32] =
    hex!("ce918627cb55462ddbb85e73de69a8b322f2bc88f4507c52fcad6d4c33c29d49");

pub struct BigEndian([u8; 32]);

impl AsRef<[u8]> for BigEndian {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<&U256> for BigEndian {
    fn from(value: &U256) -> Self {
        let mut result = [0; 32];
        value.to_big_endian(&mut result);
        Self(result)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitOrder {
    pub maker:                  Address,
    pub taker:                  Address,
    pub maker_token:            Address,
    pub taker_token:            Address,
    #[serde(with = "u128_dec")]
    pub maker_amount:           U128,
    #[serde(with = "u128_dec")]
    pub taker_amount:           U128,
    #[serde(with = "u64_dec")]
    pub expiry:                 u64,
    #[serde(with = "u256_dec")]
    pub salt:                   U256,
    pub fee_recipient:          Address,
    pub pool:                   H256,
    #[serde(with = "u128_dec")]
    pub taker_token_fee_amount: U128,
    pub sender:                 Address,
    pub verifying_contract:     Address,
    pub chain_id:               u64,
}

impl LimitOrder {
    pub fn validate(&self, chain: &ChainInfo) -> Result<(), Error> {
        require!(!self.maker_amount.is_zero(), Error::ZeroMakerAmount);
        require!(!self.taker_amount.is_zero(), Error::ZeroTakerAmount);
        require!(!self.maker.is_zero(), Error::InvalidMakerAddress);
        // require!(!self.taker.is_zero(), Error::InvalidTakerAddress);
        require!(self.taker != chain.flash_wallet, Error::InvalidTakerAddress);
        require!(
            U256::from(self.chain_id) == chain.chain_id,
            Error::InvalidVerifyingContract
        );
        require!(
            self.verifying_contract == chain.exchange,
            Error::InvalidVerifyingContract
        );
        Ok(())
    }

    pub fn hash(&self) -> H256 {
        let mut hasher = Keccak256::new();
        hasher.update(hex!("1901"));
        hasher.update(self.domain_hash());
        hasher.update(self.struct_hash());
        H256::from(<[u8; 32]>::from(hasher.finalize()))
    }

    fn domain_hash(&self) -> H256 {
        let mut hasher = Keccak256::new();
        hasher.update(DOMAIN_SEPARATOR_TYPE_HASH);
        hasher.update(NAME_HASH);
        hasher.update(VERSION_HASH);
        hasher.update(BigEndian::from(&U256::from(self.chain_id)));
        hasher.update(H256::from(self.verifying_contract));
        H256::from(<[u8; 32]>::from(hasher.finalize()))
    }

    /// Compute the EIP712 hash of the order struct.
    /// See <https://github.com/0xProject/protocol/blob/835ee4e8/contracts/zero-ex/contracts/src/features/libs/LibNativeOrder.sol#L158>
    fn struct_hash(&self) -> H256 {
        let mut hasher = Keccak256::new();
        hasher.update(TYPE_HASH);
        hasher.update(H256::from(self.maker_token));
        hasher.update(H256::from(self.taker_token));
        hasher.update(BigEndian::from(&self.maker_amount.into()));
        hasher.update(BigEndian::from(&self.taker_amount.into()));
        hasher.update(BigEndian::from(&self.taker_token_fee_amount.into()));
        hasher.update(H256::from(self.maker));
        hasher.update(H256::from(self.taker));
        hasher.update(H256::from(self.sender));
        hasher.update(H256::from(self.fee_recipient));
        hasher.update(self.pool);
        hasher.update(BigEndian::from(&self.expiry.into()));
        hasher.update(BigEndian::from(&self.salt));
        H256::from(<[u8; 32]>::from(hasher.finalize()))
    }
}

#[cfg(test)]
pub mod test {
    use serde_json::{from_value, json};

    use super::*;

    #[track_caller]
    fn assert_hex_eq<const N: usize>(value: [u8; N], expected: [u8; N]) {
        assert_eq!(hex::encode(value), hex::encode(expected));
    }

    fn hash(bytes: &[u8]) -> [u8; 32] {
        let mut hasher = Keccak256::new();
        hasher.update(bytes);
        <[u8; 32]>::from(hasher.finalize())
    }

    #[test]
    fn test_domain_separator_type_hash() {
        assert_hex_eq(DOMAIN_SEPARATOR_TYPE_HASH, hash(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"));
    }

    #[test]
    fn test_type_hash() {
        assert_hex_eq(TYPE_HASH, hash(b"LimitOrder(address makerToken,address takerToken,uint128 makerAmount,uint128 takerAmount,uint128 takerTokenFeeAmount,address maker,address taker,address sender,address feeRecipient,bytes32 pool,uint64 expiry,uint256 salt)"));
    }

    #[test]
    fn test_name_hash() {
        assert_hex_eq(NAME_HASH, hash(b"ZeroEx"));
    }

    #[test]
    fn test_version_hash() {
        assert_hex_eq(VERSION_HASH, hash(b"1.0.0"));
    }

    #[test]
    fn test_limit_order_hash() {
        // Example from <https://github.com/0xProject/protocol/blob/main/packages/protocol-utils/test/orders_test.ts#L23>
        let order = LimitOrder {
            maker_token:            Address::from(hex!("349e8d89e8b37214d9ce3949fc5754152c525bc3")),
            taker_token:            Address::from(hex!("83c62b2e67dea0df2a27be0def7a22bd7102642c")),
            maker_amount:           1234.into(),
            taker_amount:           5678.into(),
            taker_token_fee_amount: 9_101_112.into(),
            maker:                  Address::from(hex!("8d5e5b5b5d187bdce2e0143eb6b3cc44eef3c0cb")),
            taker:                  Address::from(hex!("615312fb74c31303eab07dea520019bb23f4c6c2")),
            sender:                 Address::from(hex!("70f2d6c7acd257a6700d745b76c602ceefeb8e20")),
            fee_recipient:          Address::from(hex!("cc3c7ea403427154ec908203ba6c418bd699f7ce")),
            pool:                   H256::from(hex!(
                "0bbff69b85a87da39511aefc3211cb9aff00e1a1779dc35b8f3635d8b5ea2680"
            )),
            expiry:                 1001_u64,
            salt:                   2001.into(),
            chain_id:               8008_u64,
            verifying_contract:     Address::from(hex!("6701704d2421c64ee9aa93ec7f96ede81c4be77d")),
        };

        assert_eq!(
            order.hash(),
            H256::from(hex!(
                "8bb1f6e880b3b4f91a901897c4b914ec606dc3b8b59f64983e1638a45bdf3116"
            ))
        );
    }

    #[test]
    fn test_order_with_default_fields() {
        let order = from_value::<LimitOrder>(json!({
          "makerToken": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
          "takerToken": "0xe41d2489571d322189246dafa5ebde1f4699f498",
          "makerAmount": "1",
          "takerAmount": "1000000000000000",
          "maker": "0x56eb0ad2dc746540fab5c02478b31e2aa9ddc38c",
          "taker": "0x0000000000000000000000000000000000000000",
          "pool": "0x0000000000000000000000000000000000000000000000000000000000000000",
          "expiry": "1624656574",
          "salt": "30852468424416577873871693760685064833150201451345818452120166031897122109527",
          "chainId": 1,
          "verifyingContract": "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
          "takerTokenFeeAmount": "0",
          "sender": "0x0000000000000000000000000000000000000000",
          "feeRecipient": "0x0000000000000000000000000000000000000000"}))
        .unwrap();

        assert_eq!(
            order.hash(),
            H256::from(hex!(
                "9edd32a0a6545a0372734e83b433aac169974cd2bdbd5d91d9af1064e47bd1dc"
            ))
        );
    }
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::{black_box, Criterion};
    use serde_json::{from_value, json};

    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn group(criterion: &mut Criterion) {
        bench_hash(criterion);
        bench_validate(criterion);
    }

    fn example_chain() -> ChainInfo {
        ChainInfo {
            ..ChainInfo::default()
        }
    }

    fn example_order() -> LimitOrder {
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
        });
        from_value(json).unwrap()
    }

    fn bench_hash(criterion: &mut Criterion) {
        let order = example_order();
        criterion.bench_function("limit_order_hash", move |bencher| {
            bencher.iter(|| black_box(black_box(order).hash()));
        });
    }

    fn bench_validate(criterion: &mut Criterion) {
        let order = example_order();
        let chain = example_chain();
        criterion.bench_function("limit_order_validate", move |bencher| {
            bencher.iter(|| black_box(black_box(order).validate(&chain)));
        });
    }
}
