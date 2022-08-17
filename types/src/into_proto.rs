use web3::types::{Address, BlockHeader, H2048, H256, H64, U128, U256};

pub trait IntoProto {
    type Proto;

    fn into_proto(self) -> Self::Proto;
}

impl IntoProto for U128 {
    type Proto = crate::proto::U128;

    fn into_proto(self) -> Self::Proto {
        let [limb_0, limb_1] = self.0;
        Self::Proto { limb_0, limb_1 }
    }
}

impl IntoProto for U256 {
    type Proto = crate::proto::U256;

    fn into_proto(self) -> Self::Proto {
        let [limb_0, limb_1, limb_2, limb_3] = self.0;
        Self::Proto {
            limb_0,
            limb_1,
            limb_2,
            limb_3,
        }
    }
}

impl IntoProto for Address {
    type Proto = crate::proto::Address;

    fn into_proto(self) -> Self::Proto {
        Self::Proto {
            bytes: self.0.into(),
        }
    }
}

impl IntoProto for H256 {
    type Proto = crate::proto::H256;

    fn into_proto(self) -> Self::Proto {
        Self::Proto {
            bytes: self.0.into(),
        }
    }
}

impl IntoProto for H64 {
    type Proto = crate::proto::H64;

    fn into_proto(self) -> Self::Proto {
        Self::Proto {
            bytes: self.0.into(),
        }
    }
}

impl IntoProto for H2048 {
    type Proto = crate::proto::H2048;

    fn into_proto(self) -> Self::Proto {
        Self::Proto {
            bytes: self.0.into(),
        }
    }
}

impl IntoProto for BlockHeader {
    type Proto = crate::proto::BlockHeader;

    fn into_proto(self) -> Self::Proto {
        Self::Proto {
            hash:              self.hash.map(|x| x.into_proto()),
            parent_hash:       Some(self.parent_hash.into_proto()),
            uncles_hash:       Some(self.uncles_hash.into_proto()),
            author:            Some(self.author.into_proto()),
            state_root:        Some(self.state_root.into_proto()),
            transactions_root: Some(self.transactions_root.into_proto()),
            receipts_root:     Some(self.receipts_root.into_proto()),
            number:            self.number.map(|x| x.as_u64()),
            gas_used:          Some(self.gas_used.into_proto()),
            gas_limit:         Some(self.gas_limit.into_proto()),
            base_fee_per_gas:  self.base_fee_per_gas.map(|x| x.into_proto()),
            extra_data:        self.extra_data.0.clone(),
            logs_bloom:        Some(self.logs_bloom.into_proto()),
            timestamp:         Some(self.timestamp.into_proto()),
            difficulty:        Some(self.difficulty.into_proto()),
            mix_hash:          self.mix_hash.map(|x| x.into_proto()),
            nonce:             self.nonce.map(|x| x.into_proto()),
        }
    }
}
