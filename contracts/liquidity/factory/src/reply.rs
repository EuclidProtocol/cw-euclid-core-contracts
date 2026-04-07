use crate::{
    ibc,
    state::{PENDING_DEPOSIT_TOKEN, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN},
};
use cosmwasm_std::{from_json, DepsMut, Env, Event, Reply, Response, SubMsgResult};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data};
use euclid::{
    error::ContractError,
    events::{simple_event, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT},
};
use euclid_ibc::{
    ack::make_ack_fail,
    router_ibc::RouterCrossChainExecuteMsg,
    state::{
        NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE, NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER,
    },
};
use function_name::named;

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1;
pub const LP_INSTANTIATE_REPLY_ID: u64 = 4;
pub const RELEASE_ESCROW_REPLY_ID: u64 = 5;

pub const CROSS_CHAIN_RECEIVE_REPLY_ID: u64 = 6;

pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let escrow_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let escrow_data: euclid::msgs::escrow::EscrowInstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            TOKEN_TO_ESCROW.save(deps.storage, escrow_data.token.clone(), &escrow_address)?;

            let mut response = Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("escrow", escrow_address.clone())
                .add_attribute("token_id", escrow_data.token.to_string());

            let pending_deposit_token =
                PENDING_DEPOSIT_TOKEN.may_load(deps.storage, escrow_data.token.clone())?;

            if let Some(token) = pending_deposit_token {
                let deposit_msg = token
                    .token_type
                    .create_escrow_msg(token.amount, escrow_address)?;
                response = response.add_message(deposit_msg);
                PENDING_DEPOSIT_TOKEN.remove(deps.storage, token.token);
            }

            Ok(response)
        }
    }
}

pub fn on_lp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let cw20_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let cw20_data: euclid::msgs::escrow::Cw20InstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            VLP_TO_LP_TOKEN.save(deps.storage, cw20_data.vlp, &cw20_address)?;

            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("cw20", cw20_address))
        }
    }
}

#[named]
pub fn on_release_escrow_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            Ok(Response::new()
                .add_attribute("reply_on_release_escrow_processing", "success")
                .set_data(data))
        }
    }
}

pub fn on_reply_native_ibc_wrapper_call(
    deps: &mut DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let original_msg = NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.load(deps.storage, msg.id)?;
    NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.remove(deps.storage, msg.id);
    let _sender = NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.load(deps.storage, msg.id)?;
    NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.remove(deps.storage, msg.id);
    let original_msg: RouterCrossChainExecuteMsg = from_json(original_msg.original_msg)?;
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let ack = make_ack_fail(err.clone())?;
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_msg,
                ack,
                true,
            )?;
            Ok(response
                .add_attribute("reply_on_native_ibc_wrapper_call_processing", "err")
                .add_attribute("err", err))
        }
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_msg,
                data,
                true,
            )?;
            Ok(response.add_attribute("reply_on_native_ibc_wrapper_call_processing", "success"))
        }
    }
}

pub fn on_cross_chain_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let euclid_event = simple_event().add_attribute("action", "cross-chain-receive");

            let write_acknowledge_event = Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
                .add_attribute("ack", make_ack_fail(err.clone())?.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_receive_processing", "error")
                .add_attribute("error", err.clone())
                .add_event(euclid_event)
                .add_event(write_acknowledge_event))
        }
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let euclid_event =
                simple_event().add_attribute("action", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);

            let write_acknowledge_event = Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
                .add_attribute("ack", data.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_receive_processing", "success")
                .add_event(euclid_event)
                .add_event(write_acknowledge_event)
                .set_data(data))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        state::{PENDING_DEPOSIT_TOKEN, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN},
        testing::helpers::init,
    };
    use cosmwasm_std::{
        testing::mock_dependencies,
        Binary, Reply, SubMsgResponse, SubMsgResult,
    };
    use euclid::{
        msgs::escrow::{Cw20InstantiateResponse, EscrowInstantiateResponse},
        token::{Pair, Token, TokenType, TokenWithDenomAndAmount},
    };
    use euclid_ibc::ack::make_ack_fail;

    // -----------------------------------------------------------------------
    // Proto encoding helpers (mirrors the pattern from router/reply.rs tests)
    // -----------------------------------------------------------------------

    fn encode_varint(mut v: usize, out: &mut Vec<u8>) {
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    fn encode_length_delimited_field(field_number: u8, data: &[u8], out: &mut Vec<u8>) {
        out.push((field_number << 3) | 2u8);
        encode_varint(data.len(), out);
        out.extend_from_slice(data);
    }

    /// Build the raw bytes that `parse_instantiate_response_data` expects:
    ///   field 1: string  = contract_address
    ///   field 2: bytes   = inner_data (optional; skipped when empty)
    fn encode_instantiate_response(contract_address: &str, inner_data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encode_length_delimited_field(1, contract_address.as_bytes(), &mut out);
        if !inner_data.is_empty() {
            encode_length_delimited_field(2, inner_data, &mut out);
        }
        out
    }

    /// Build the raw bytes that `parse_execute_response_data` expects:
    ///   field 1: bytes = inner_data (optional; skipped when empty)
    fn encode_execute_response(inner_data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if !inner_data.is_empty() {
            encode_length_delimited_field(1, inner_data, &mut out);
        }
        out
    }

    fn ok_reply(id: u64, proto_data: Vec<u8>) -> Reply {
        Reply {
            id,
            payload: Binary::default(),
            #[allow(deprecated)]
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                #[allow(deprecated)]
                data: Some(Binary::new(proto_data)),
                msg_responses: vec![],
            }),
        }
    }

    fn ok_reply_no_data(id: u64) -> Reply {
        Reply {
            id,
            payload: Binary::default(),
            #[allow(deprecated)]
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                #[allow(deprecated)]
                data: None,
                msg_responses: vec![],
            }),
        }
    }

    fn err_reply(id: u64, err: &str) -> Reply {
        Reply {
            id,
            payload: Binary::default(),
            #[allow(deprecated)]
            gas_used: 0,
            result: SubMsgResult::Err(err.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // on_escrow_instantiate_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_on_escrow_instantiate_reply_error_returns_pool_instantiate_failed() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let msg = err_reply(ESCROW_INSTANTIATE_REPLY_ID, "escrow init failed");
        let err = on_escrow_instantiate_reply(deps.as_mut(), msg).unwrap_err();

        assert_eq!(
            err,
            ContractError::PoolInstantiateFailed {
                err: "escrow init failed".to_string()
            }
        );
    }

    #[test]
    fn test_on_escrow_instantiate_reply_ok_saves_token_to_escrow_and_returns_attributes() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let token = Token::create("usdc".to_string()).unwrap();
        let escrow_addr = deps.api.addr_make("escrow_contract");

        let inner = cosmwasm_std::to_json_binary(&EscrowInstantiateResponse {
            token: token.clone(),
            address: escrow_addr.to_string(),
        })
        .unwrap();
        let proto = encode_instantiate_response(escrow_addr.as_str(), inner.as_slice());
        let msg = ok_reply(ESCROW_INSTANTIATE_REPLY_ID, proto);

        let res = on_escrow_instantiate_reply(deps.as_mut(), msg).unwrap();

        // Attributes
        assert_eq!(res.attributes[0].key, "action");
        assert_eq!(res.attributes[0].value, "reply_pool_instantiate");
        assert_eq!(res.attributes[1].key, "escrow");
        assert_eq!(res.attributes[1].value, escrow_addr.to_string());
        assert_eq!(res.attributes[2].key, "token_id");
        assert_eq!(res.attributes[2].value, token.to_string());

        // No extra message when there is no pending deposit
        assert!(res.messages.is_empty());

        // State saved
        let stored = TOKEN_TO_ESCROW
            .load(&deps.storage, token.clone())
            .unwrap();
        assert_eq!(stored, escrow_addr);
    }

    #[test]
    fn test_on_escrow_instantiate_reply_ok_with_pending_deposit_adds_message_and_removes_entry() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let token = Token::create("atom".to_string()).unwrap();
        let escrow_addr = deps.api.addr_make("escrow_atom");

        // Seed a pending deposit
        PENDING_DEPOSIT_TOKEN
            .save(
                deps.as_mut().storage,
                token.clone(),
                &TokenWithDenomAndAmount {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uatom".to_string(),
                    },
                    amount: cosmwasm_std::Uint128::new(500),
                },
            )
            .unwrap();

        let inner = cosmwasm_std::to_json_binary(&EscrowInstantiateResponse {
            token: token.clone(),
            address: escrow_addr.to_string(),
        })
        .unwrap();
        let proto = encode_instantiate_response(escrow_addr.as_str(), inner.as_slice());
        let msg = ok_reply(ESCROW_INSTANTIATE_REPLY_ID, proto);

        let res = on_escrow_instantiate_reply(deps.as_mut(), msg).unwrap();

        // An extra message was included for the deposit
        assert_eq!(res.messages.len(), 1);

        // PENDING_DEPOSIT_TOKEN entry removed
        let still_pending = PENDING_DEPOSIT_TOKEN
            .may_load(&deps.storage, token.clone())
            .unwrap();
        assert!(still_pending.is_none());
    }

    #[test]
    fn test_on_escrow_instantiate_reply_ok_without_pending_deposit_no_extra_message() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let token = Token::create("osmo".to_string()).unwrap();
        let escrow_addr = deps.api.addr_make("escrow_osmo");

        let inner = cosmwasm_std::to_json_binary(&EscrowInstantiateResponse {
            token: token.clone(),
            address: escrow_addr.to_string(),
        })
        .unwrap();
        let proto = encode_instantiate_response(escrow_addr.as_str(), inner.as_slice());
        let msg = ok_reply(ESCROW_INSTANTIATE_REPLY_ID, proto);

        let res = on_escrow_instantiate_reply(deps.as_mut(), msg).unwrap();

        assert!(res.messages.is_empty());
    }

    // -----------------------------------------------------------------------
    // on_lp_instantiate_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_on_lp_instantiate_reply_error_returns_pool_instantiate_failed() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let msg = err_reply(LP_INSTANTIATE_REPLY_ID, "lp init failed");
        let err = on_lp_instantiate_reply(deps.as_mut(), msg).unwrap_err();

        assert_eq!(
            err,
            ContractError::PoolInstantiateFailed {
                err: "lp init failed".to_string()
            }
        );
    }

    #[test]
    fn test_on_lp_instantiate_reply_ok_saves_vlp_to_lp_token_and_returns_attributes() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let cw20_addr = deps.api.addr_make("cw20_contract");
        let vlp_addr = deps.api.addr_make("vlp_contract");

        let token_a = Token::create("tokenA".to_string()).unwrap();
        let token_b = Token::create("tokenB".to_string()).unwrap();
        let pair = Pair::new(token_a.clone(), token_b.clone()).unwrap();

        let inner = cosmwasm_std::to_json_binary(&Cw20InstantiateResponse {
            pair: pair.clone(),
            address: cw20_addr.to_string(),
            vlp: vlp_addr.to_string(),
        })
        .unwrap();
        let proto = encode_instantiate_response(cw20_addr.as_str(), inner.as_slice());
        let msg = ok_reply(LP_INSTANTIATE_REPLY_ID, proto);

        let res = on_lp_instantiate_reply(deps.as_mut(), msg).unwrap();

        // Attributes
        assert_eq!(res.attributes[0].key, "action");
        assert_eq!(res.attributes[0].value, "reply_pool_instantiate");
        assert_eq!(res.attributes[1].key, "cw20");
        assert_eq!(res.attributes[1].value, cw20_addr.to_string());

        // State saved
        let stored = VLP_TO_LP_TOKEN
            .load(&deps.storage, vlp_addr.to_string())
            .unwrap();
        assert_eq!(stored, cw20_addr);
    }

    // -----------------------------------------------------------------------
    // on_release_escrow_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_on_release_escrow_reply_error_returns_reply_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let msg = err_reply(RELEASE_ESCROW_REPLY_ID, "release failed");
        let err = on_release_escrow_reply(deps.as_mut(), msg).unwrap_err();

        assert_eq!(
            err,
            ContractError::Reply {
                action: "on_release_escrow_reply".to_string(),
                err: "release failed".to_string(),
            }
        );
    }

    #[test]
    fn test_on_release_escrow_reply_ok_no_data_returns_success_attribute() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let msg = ok_reply_no_data(RELEASE_ESCROW_REPLY_ID);
        let res = on_release_escrow_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(res.attributes[0].key, "reply_on_release_escrow_processing");
        assert_eq!(res.attributes[0].value, "success");
        // data should be empty default
        assert_eq!(res.data, Some(Binary::default()));
    }

    #[test]
    fn test_on_release_escrow_reply_ok_with_execute_response_data_sets_inner_data() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let inner_data = b"result_payload";
        let proto = encode_execute_response(inner_data);
        let msg = ok_reply(RELEASE_ESCROW_REPLY_ID, proto);

        let res = on_release_escrow_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(res.attributes[0].key, "reply_on_release_escrow_processing");
        assert_eq!(res.attributes[0].value, "success");
        assert_eq!(res.data, Some(Binary::from(inner_data.as_slice())));
    }

    // -----------------------------------------------------------------------
    // on_cross_chain_receive_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_on_cross_chain_receive_reply_error_returns_ok_with_error_attributes_and_ack_event() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let err_str = "receive processing error";
        let msg = err_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, err_str);
        let res = on_cross_chain_receive_reply(deps.as_mut(), msg).unwrap();

        // Attributes
        assert_eq!(res.attributes[0].key, "reply_on_receive_processing");
        assert_eq!(res.attributes[0].value, "error");
        assert_eq!(res.attributes[1].key, "error");
        assert_eq!(res.attributes[1].value, err_str);

        // Two events emitted
        assert_eq!(res.events.len(), 2);

        // Second event is EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT with encoded fail ack
        let write_ack_event = &res.events[1];
        assert_eq!(write_ack_event.ty, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);
        let ack_attr = write_ack_event
            .attributes
            .iter()
            .find(|a| a.key == "ack")
            .expect("ack attribute missing");
        let expected_ack = make_ack_fail(err_str.to_string()).unwrap().to_string();
        assert_eq!(ack_attr.value, expected_ack);
    }

    #[test]
    fn test_on_cross_chain_receive_reply_ok_no_data_returns_success_attribute_and_two_events() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let msg = ok_reply_no_data(CROSS_CHAIN_RECEIVE_REPLY_ID);
        let res = on_cross_chain_receive_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(res.attributes[0].key, "reply_on_receive_processing");
        assert_eq!(res.attributes[0].value, "success");

        // Two events emitted
        assert_eq!(res.events.len(), 2);

        // Second event is EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT
        assert_eq!(res.events[1].ty, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);
    }
}
