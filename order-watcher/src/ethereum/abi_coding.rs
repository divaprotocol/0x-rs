use ethabi::Token;
use web3::{
    contract::{
        tokens::{Detokenize, Tokenizable, Tokenize},
        Error,
    },
    types::{H256, U128},
};

use crate::orders::{OrderStatus, SignatureType, SignedOrder, SignedOrderState};

#[derive(Debug, Clone)]
pub struct Input(Vec<SignedOrder>);

#[derive(Debug, Clone)]
pub struct Output(Vec<SignedOrderState>);

impl From<Vec<SignedOrder>> for Input {
    fn from(signed_orders: Vec<SignedOrder>) -> Self {
        Self(signed_orders)
    }
}

impl From<Output> for Vec<SignedOrderState> {
    fn from(output: Output) -> Self {
        output.0
    }
}

impl Tokenize for Input {
    fn into_tokens(self) -> Vec<Token> {
        let orders: Vec<_> = self
            .0
            .iter()
            .map(|signed_order| {
                let order = signed_order.order;
                Token::Tuple(vec![
                    Token::Address(order.maker_token),
                    Token::Address(order.taker_token),
                    Token::Uint(order.maker_amount.into()),
                    Token::Uint(order.taker_amount.into()),
                    Token::Uint(order.taker_token_fee_amount.into()),
                    Token::Address(order.maker),
                    Token::Address(order.taker),
                    Token::Address(order.sender),
                    Token::Address(order.fee_recipient),
                    Token::FixedBytes(order.pool.to_fixed_bytes().to_vec()),
                    Token::Uint(order.expiry.into()),
                    Token::Uint(order.salt),
                ])
            })
            .collect();
        let signatures: Vec<_> = self
            .0
            .iter()
            .map(|signed_order| {
                let signature = signed_order.signature;
                Token::Tuple(vec![
                    Token::Uint(
                        match signature.signature_type {
                            SignatureType::EIP712 => 2,
                            SignatureType::EthSign => 3,
                        }
                        .into(),
                    ),
                    Token::Uint(signature.v.into()),
                    Token::FixedBytes(signature.r.to_fixed_bytes().to_vec()),
                    Token::FixedBytes(signature.s.to_fixed_bytes().to_vec()),
                ])
            })
            .collect();
        vec![Token::Array(orders), Token::Array(signatures)]
    }
}

impl Detokenize for Output {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        let (order_infos, fillable_amounts, is_signature_valids) = match tokens.as_slice() {
            [order_info_tokens, fillable_amounts_tokens, is_signature_valids_tokens] => {
                let order_infos: Vec<_> = order_info_tokens
                    .clone()
                    .into_array()
                    .unwrap()
                    .iter()
                    .map(|order_info| {
                        let (hash, status, amount_filled) = match order_info {
                            Token::Tuple(tokens) => {
                                match tokens.as_slice() {
                                    [hash_token, status_token, amount_filled_token] => {
                                        (
                                            hash_token.clone(),
                                            status_token.clone(),
                                            amount_filled_token.clone(),
                                        )
                                    }
                                    _ => panic!(),
                                }
                            }
                            _ => panic!(),
                        };
                        (
                            H256::from_token(hash).unwrap(),
                            u8::from_token(status).unwrap(),
                            U128::from_token(amount_filled).unwrap(),
                        )
                    })
                    .collect();
                let fillable_amounts: Vec<U128> = fillable_amounts_tokens
                    .clone()
                    .into_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(U128::from_token)
                    .map(Result::unwrap)
                    .collect();
                let is_signature_valids: Vec<bool> = is_signature_valids_tokens
                    .clone()
                    .into_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(Token::into_bool)
                    .map(Option::unwrap)
                    .collect();
                Ok((order_infos, fillable_amounts, is_signature_valids))
            }
            _ => {
                Err(Error::InvalidOutputType(format!(
                    "Did not get three elements: {:?}",
                    tokens
                )))
            }
        }?;

        let mut result = vec![];
        for ((order_info, fillable_amount), is_signature_valid) in order_infos
            .iter()
            .zip(&fillable_amounts)
            .zip(&is_signature_valids)
        {
            result.push(SignedOrderState {
                hash: order_info.0,
                status: decode_status(order_info.1)?,
                taker_asset_filled_amount: order_info.2,
                taker_asset_fillable_amount: *fillable_amount,
                is_signature_valid: *is_signature_valid,
            });
        }
        Ok(Self(result))
    }
}

fn decode_status(status: u8) -> Result<OrderStatus, Error> {
    match status {
        0 => Ok(OrderStatus::Invalid),
        1 => Ok(OrderStatus::Fillable),
        2 => Ok(OrderStatus::FullyFilled),
        3 => Ok(OrderStatus::Cancelled),
        4 => Ok(OrderStatus::Expired),
        _ => {
            Err(Error::InvalidOutputType(format!(
                "Got {:?} for status",
                status
            )))
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_input_encoding() -> Result<(), Error> {
        let abi = ethabi::Contract::load(&include_bytes!("../../ethereum-abis/exchange.json")[..])?;
        let batch_validate = abi.function("batchGetLimitOrderRelevantStates")?;
        let input_types: Vec<_> = batch_validate
            .inputs
            .iter()
            .map(|p| p.kind.clone())
            .collect();

        let input_tokens = Input(vec![SignedOrder::default()]).into_tokens();

        assert!(Token::types_check(&input_tokens, &input_types));
        Ok(())
    }

    #[test]
    fn test_output_encoding() -> Result<(), Error> {
        let abi = ethabi::Contract::load(&include_bytes!("../../ethereum-abis/exchange.json")[..])?;
        let batch_validate = abi.function("batchGetLimitOrderRelevantStates")?;

        let raw_output = hex::decode(    "000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000002200000000000000000000000000000000000000000000000000000000000000003c367864df0e1ee0b1524137cc13a1da0b69a6861eda9a7f2235c7acf3db04d66000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000009b244ca76f05c148a70c4ac843aebf3e3f2bb2971630512696dd511eb259c92400000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000ac10221df8c400eca1b1f5b4f0c6f12cda1718d89490e43f908f70174383245900000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000038d7ea4c680000000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001").unwrap();

        batch_validate.decode_output(&raw_output)?;
        Ok(())
    }
}
