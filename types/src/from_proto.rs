use web3::types::{Address, BlockHeader, H2048, H256, H64, U128, U256, U64};

pub trait FromProto {
    type Proto;

    fn from_proto(p: Self::Proto) -> Self;
}

impl FromProto for U128 {
    type Proto = crate::proto::U128;

    fn from_proto(p: Self::Proto) -> Self {
        Self([p.limb_0, p.limb_1])
    }
}

impl FromProto for U256 {
    type Proto = crate::proto::U256;

    fn from_proto(p: Self::Proto) -> Self {
        Self([p.limb_0, p.limb_1, p.limb_2, p.limb_3])
    }
}

impl FromProto for Address {
    type Proto = crate::proto::Address;

    fn from_proto(p: Self::Proto) -> Self {
        Self::from_slice(&p.bytes)
    }
}

impl FromProto for H256 {
    type Proto = crate::proto::H256;

    fn from_proto(p: Self::Proto) -> Self {
        Self::from_slice(&p.bytes)
    }
}

impl FromProto for H64 {
    type Proto = crate::proto::H64;

    fn from_proto(p: Self::Proto) -> Self {
        Self::from_slice(&p.bytes)
    }
}

impl FromProto for H2048 {
    type Proto = crate::proto::H2048;

    fn from_proto(p: Self::Proto) -> Self {
        Self::from_slice(&p.bytes)
    }
}

// TODO: Derive this? https://docs.rs/syn/1.0.76/syn/index.html#example-of-a-custom-derive
impl FromProto for BlockHeader {
    type Proto = crate::proto::BlockHeader;

    fn from_proto(p: Self::Proto) -> Self {
        Self {
            hash:              p.hash.map(H256::from_proto),
            parent_hash:       p.parent_hash.map(H256::from_proto).unwrap(),
            uncles_hash:       p.uncles_hash.map(H256::from_proto).unwrap(),
            author:            p.author.map(Address::from_proto).unwrap(),
            state_root:        p.state_root.map(H256::from_proto).unwrap(),
            transactions_root: p.transactions_root.map(H256::from_proto).unwrap(),
            receipts_root:     p.receipts_root.map(H256::from_proto).unwrap(),
            number:            p.number.map(U64::from),
            gas_used:          p.gas_used.map(U256::from_proto).unwrap(),
            gas_limit:         p.gas_limit.map(U256::from_proto).unwrap(),
            base_fee_per_gas:  p.base_fee_per_gas.map(U256::from_proto),
            extra_data:        p.extra_data.into(),
            logs_bloom:        p.logs_bloom.map(H2048::from_proto).unwrap(),
            timestamp:         p.timestamp.map(U256::from_proto).unwrap(),
            difficulty:        p.difficulty.map(U256::from_proto).unwrap(),
            mix_hash:          p.mix_hash.map(H256::from_proto),
            nonce:             p.nonce.map(H64::from_proto),
        }
    }
}
