use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw_utils::parse_execute_response_data;
use euclid::{error::ContractError, msgs::vlp::base::VlpSwapResponse};
use function_name::named;

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::mock_dependencies, Binary, Reply, SubMsgResponse, SubMsgResult, Uint128,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        msgs::vlp::base::{VlpSwapResponse, NEXT_SWAP_REPLY_ID},
        token::Token,
    };

    fn make_execute_response_data(inner: &[u8]) -> Binary {
        assert!(
            inner.len() < 128,
            "test payload must fit in a single varint byte"
        );
        let mut out = vec![0x0a]; // field=1, wire_type=2 → (1<<3)|2
        out.push(inner.len() as u8);
        out.extend_from_slice(inner);
        Binary::new(out)
    }

    #[test]
    fn test_on_next_swap_reply_success_attributes() {
        let mut deps = mock_dependencies();

        let swap_response = VlpSwapResponse {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
            tx_id: "tx1".to_string(),
            asset_out: Token::create("token2".to_string()).unwrap(),
            amount_out: Uint128::new(9_000),
        };

        let inner = to_json_binary(&swap_response).unwrap();
        let reply_data = make_execute_response_data(inner.as_slice());

        #[allow(deprecated)]
        let msg = Reply {
            id: NEXT_SWAP_REPLY_ID,
            payload: Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                data: Some(reply_data),
                msg_responses: vec![],
            }),
        };

        let res = on_next_swap_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "action")
                .unwrap()
                .value,
            "reply_next_swap"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "swap_id")
                .unwrap()
                .value,
            "tx1"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "amount_out")
                .unwrap()
                .value,
            "9000"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "asset_out")
                .unwrap()
                .value,
            "token2"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "sender")
                .unwrap()
                .value,
            "chain1:user1"
        );
    }
}

#[named]
pub fn on_next_swap_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let swap_response: VlpSwapResponse = from_json(execute_data.data.unwrap_or_default())?;

            Ok(Response::new()
                .add_attribute("action", "reply_next_swap")
                .add_attribute("swap_id", swap_response.tx_id.clone())
                .add_attribute("amount_out", swap_response.amount_out)
                .add_attribute("asset_out", swap_response.asset_out.to_string())
                .add_attribute("sender", swap_response.sender.to_sender_string())
                .set_data(to_json_binary(&swap_response)?))
        }
    }
}
