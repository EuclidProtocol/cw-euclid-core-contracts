use crate::{
    ibc,
    query::get_chain_type,
    relay_state::{map_encoding_err, IN_FLIGHT_RECEIVE},
    state::{
        PENDING_DEPOSIT_TOKEN, POSITION_TOKEN_CONTRACT, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
    },
};
use cosmwasm_std::{from_json, DepsMut, Env, Reply, Response, SubMsgResult};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data};
use euclid::{
    chain::ChainType,
    error::ContractError,
    events::{
        simple_event, write_acknowledgement_encoded_event,
        EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT,
    },
    msgs::pool_factory::PoolFactoryReply,
};
use euclid_encoding::{Encoding, PROTOCOL_VERSION};
use euclid_ibc::{
    state::{
        NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE, NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER,
    },
    wire::envelope::{make_ack_fail, router::RouterReceiveMsg},
    wire::transcode,
};
use function_name::named;

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1;
pub const LP_INSTANTIATE_REPLY_ID: u64 = 4;
pub const RELEASE_ESCROW_REPLY_ID: u64 = 5;

pub const CROSS_CHAIN_RECEIVE_REPLY_ID: u64 = 6;
pub const POSITION_TOKEN_INSTANTIATE_REPLY_ID: u64 = 7;

/// Reply id used by main factory to consume `pool_factory`'s typed
/// `PoolFactoryReply::SendPacket` payload and run the existing
/// `execute_send_packet` flow on its behalf. Pool factory does not call
/// back into main factory to request an outbound packet; instead it
/// returns the request as `Response::data` from the `On*` handler main
/// factory dispatched as `SubMsg::reply_on_success`.
pub const POOL_FACTORY_DELEGATE_REPLY_ID: u64 = 8;

#[named]
pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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

#[named]
pub fn on_position_token_instantiate_reply(
    deps: DepsMut,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let position_token_address =
                deps.api.addr_validate(&instantiate_data.contract_address)?;

            if POSITION_TOKEN_CONTRACT.may_load(deps.storage)?.is_some() {
                return Err(ContractError::Generic {
                    err: "position token contract already registered".to_string(),
                });
            }
            POSITION_TOKEN_CONTRACT.save(deps.storage, &position_token_address)?;

            Ok(Response::new()
                .add_attribute("action", "reply_position_token_instantiate")
                .add_attribute("position_token_contract", position_token_address))
        }
    }
}

#[named]
pub fn on_lp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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

/// Consumes a `PoolFactoryReply::SendPacket` payload set on the data of a
/// successful `On*` handler invocation against pool_factory, and runs the
/// same outbound dispatch path `execute_send_packet` runs for user-driven
/// flows.
///
/// Authorisation is structural: CosmWasm guarantees this reply only fires
/// for a submsg main factory itself dispatched, so there is no public
/// surface an attacker can use to inject reply data. The handler still
/// performs a defence-in-depth check on the decoded packet: any
/// `RouterReceiveMsg` variant that is not a pool variant is
/// rejected, preventing a pool_factory bug from emitting an arbitrary
/// cross-chain message (e.g. `Swap`, `TransferVoucher`, `RegisterDenom`).
#[named]
pub fn on_pool_factory_delegate_reply(
    mut deps: DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let result = msg
        .result
        .into_result()
        .map_err(|err| ContractError::Reply {
            action: function_name!().to_string(),
            err,
        })?;

    // SubMsg::reply_on_success delivers the called contract's
    // `Response::data` wrapped in a protobuf `MsgExecuteContractResponse`
    // envelope. Unwrap it the same way `on_release_escrow_reply` does
    // before decoding the inner JSON payload.
    #[allow(deprecated)]
    let envelope = result.data.ok_or_else(|| ContractError::Reply {
        action: function_name!().to_string(),
        err: "pool_factory delegate reply missing data".to_string(),
    })?;

    let inner = parse_execute_response_data(&envelope)
        .map_err(|err| ContractError::Reply {
            action: function_name!().to_string(),
            err: format!("failed to parse execute response envelope: {err}"),
        })?
        .data
        .ok_or_else(|| ContractError::Reply {
            action: function_name!().to_string(),
            err: "pool_factory delegate reply envelope carried no data".to_string(),
        })?;

    let reply_payload: PoolFactoryReply =
        from_json(&inner).map_err(|err| ContractError::Reply {
            action: function_name!().to_string(),
            err: format!("failed to decode PoolFactoryReply: {err}"),
        })?;

    match reply_payload {
        PoolFactoryReply::SendPacket {
            msg: packet,
            timeout,
            ack_response,
            sender,
        } => {
            let router_msg: RouterReceiveMsg =
                from_json(&packet).map_err(|err| ContractError::Reply {
                    action: function_name!().to_string(),
                    err: format!("failed to decode RouterReceiveMsg: {err}"),
                })?;
            if !router_msg.is_pool_variant() {
                return Err(ContractError::Reply {
                    action: function_name!().to_string(),
                    err: "pool_factory returned a non-pool RouterReceiveMsg variant".to_string(),
                });
            }
            let tx_id = router_msg.get_tx_id();

            let state = STATE.load(deps.storage)?;
            let chain_type: ChainType = get_chain_type(deps.as_ref(), &env)?;

            let submsg = match chain_type {
                ChainType::Native {} | ChainType::Cosmos(_) => router_msg.to_msg(
                    &mut deps,
                    &env,
                    state.router_contract.clone(),
                    sender,
                    state.chain_uid.clone(),
                    chain_type,
                    timeout,
                    ack_response,
                )?,
                _ => {
                    return Err(ContractError::new(
                        "Pool factory delegate reply only supports cosmos/native chain types",
                    ));
                }
            };

            Ok(Response::new()
                .add_attribute("method", "on_pool_factory_delegate_reply")
                .add_attribute("tx_id", tx_id)
                .add_submessage(submsg))
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
    let original_msg: RouterReceiveMsg = from_json(original_msg.original_msg)?;
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

pub fn on_cross_chain_receive_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    // Consume the receive context saved by execute_receive_packet in this same
    // transaction; it keys the ack transcode onto the leg encoding.
    let ctx = IN_FLIGHT_RECEIVE.load(deps.storage)?;
    IN_FLIGHT_RECEIVE.remove(deps.storage);
    let encoding = Encoding::from_u8(ctx.encoding).map_err(map_encoding_err)?;

    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let ack_json = make_ack_fail(err.clone())?;
            // The Error arm is tag independent; the same transcode applies.
            let wire_ack = transcode::factory_ack_json_to_wire(ctx.wire_tag, &ack_json, encoding)
                .map_err(map_encoding_err)?;

            let euclid_event = simple_event().add_attribute("action", "cross-chain-receive");

            // The single complete acknowledgement event. Ports are emitted
            // swapped versus the incoming ReceivePacket: the acknowledging
            // side is the event's source_port. The ack's destination is
            // always the hub, hence the constant chain type.
            let write_acknowledge_event = write_acknowledgement_encoded_event(
                &euclid_encoding::repr::to_transport_string(&ctx.msg, encoding)
                    .map_err(map_encoding_err)?,
                ctx.sequence,
                &ctx.destination_port,
                &ctx.source_port,
                &euclid_encoding::repr::to_transport_string(&wire_ack, encoding)
                    .map_err(map_encoding_err)?,
                "cosmos",
                "error",
                PROTOCOL_VERSION,
                encoding.as_u8(),
            );

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

            // The reply data is the internal AcknowledgementMsg JSON; re-encode
            // it onto the wire keyed by the originating send tag.
            let wire_ack = transcode::factory_ack_json_to_wire(ctx.wire_tag, &data, encoding)
                .map_err(map_encoding_err)?;

            let euclid_event =
                simple_event().add_attribute("action", EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT);

            // The single complete acknowledgement event, ports swapped as in
            // the error branch.
            let write_acknowledge_event = write_acknowledgement_encoded_event(
                &euclid_encoding::repr::to_transport_string(&ctx.msg, encoding)
                    .map_err(map_encoding_err)?,
                ctx.sequence,
                &ctx.destination_port,
                &ctx.source_port,
                &euclid_encoding::repr::to_transport_string(&wire_ack, encoding)
                    .map_err(map_encoding_err)?,
                "cosmos",
                "success",
                PROTOCOL_VERSION,
                encoding.as_u8(),
            );

            // set_data keeps the internal JSON form (internal contract data
            // contract is unchanged).
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
        testing::{mock_dependencies, mock_env},
        Binary, Reply, SubMsgResponse, SubMsgResult,
    };
    use euclid::{
        msgs::escrow::{Cw20InstantiateResponse, EscrowInstantiateResponse},
        token::{Pair, Token, TokenType, TokenWithDenomAndAmount},
    };
    use euclid_ibc::wire::envelope::make_ack_fail;
    use euclid_ibc::wire::msgs::RequestPoolCreationSendMsg;

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
            ContractError::Reply {
                action: "on_escrow_instantiate_reply".to_string(),
                err: "escrow init failed".to_string(),
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
        let stored = TOKEN_TO_ESCROW.load(&deps.storage, token.clone()).unwrap();
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
                        decimals: None,
                    },
                    amount: cosmwasm_std::Uint256::from(500u128),
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
            ContractError::Reply {
                action: "on_lp_instantiate_reply".to_string(),
                err: "lp init failed".to_string(),
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

    /// Ports and msg the seeded context stores, as received in ReceivePacket.
    const IN_FLIGHT_SOURCE_PORT: &str = "vsl.router1";
    const IN_FLIGHT_DESTINATION_PORT: &str = "testchain.factory1";
    const IN_FLIGHT_MSG: &[u8] = b"in-flight-wire-msg";

    fn seed_in_flight_receive(
        deps: &mut crate::testing::helpers::MockDeps,
        sequence: u128,
        encoding: u8,
        wire_tag: u8,
    ) {
        use crate::relay_state::{InFlightReceive, IN_FLIGHT_RECEIVE};
        IN_FLIGHT_RECEIVE
            .save(
                deps.as_mut().storage,
                &InFlightReceive {
                    source_port: IN_FLIGHT_SOURCE_PORT.to_string(),
                    destination_port: IN_FLIGHT_DESTINATION_PORT.to_string(),
                    msg: Binary::from(IN_FLIGHT_MSG.to_vec()),
                    sequence,
                    encoding,
                    wire_tag,
                },
            )
            .unwrap();
    }

    fn ack_event_attr(event: &cosmwasm_std::Event, key: &str) -> String {
        event
            .attributes
            .iter()
            .find(|a| a.key == key)
            .unwrap_or_else(|| panic!("missing attribute {key}"))
            .value
            .clone()
    }

    #[test]
    fn test_on_cross_chain_receive_reply_ok_emits_payload_event() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_in_flight_receive(&mut deps, 7, 0, 1);

        // Provide some arbitrary ack data wrapped in execute-response encoding
        // (a Json leg passes ack bytes through untouched).
        let ack_bytes = b"ack_payload";
        let proto = encode_execute_response(ack_bytes);
        let msg = ok_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, proto);

        let res = on_cross_chain_receive_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(res.attributes[0].key, "reply_on_receive_processing");
        assert_eq!(res.attributes[0].value, "success");

        // Two events emitted; exactly one is the complete acknowledgement event.
        assert_eq!(res.events.len(), 2);
        let write_ack_events: Vec<_> = res
            .events
            .iter()
            .filter(|e| e.ty == euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT)
            .collect();
        assert_eq!(write_ack_events.len(), 1);
        let write_ack_event = write_ack_events[0];

        // Nine attributes in the canonical cross-VM order.
        let keys: Vec<&str> = write_ack_event
            .attributes
            .iter()
            .map(|a| a.key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "msg",
                "sequence",
                "source_port",
                "destination_port",
                "ack",
                "destination_chain_type",
                "ack_type",
                "version",
                "encoding"
            ]
        );
        // Ports are swapped versus the incoming ReceivePacket: the
        // acknowledging side is the event's source_port.
        assert_eq!(
            ack_event_attr(write_ack_event, "source_port"),
            IN_FLIGHT_DESTINATION_PORT
        );
        assert_eq!(
            ack_event_attr(write_ack_event, "destination_port"),
            IN_FLIGHT_SOURCE_PORT
        );
        // Json leg: raw text, not base64.
        assert_eq!(
            ack_event_attr(write_ack_event, "msg"),
            String::from_utf8(IN_FLIGHT_MSG.to_vec()).unwrap()
        );
        assert_eq!(ack_event_attr(write_ack_event, "sequence"), "7");
        // The ack's destination is always the hub.
        assert_eq!(
            ack_event_attr(write_ack_event, "destination_chain_type"),
            "cosmos"
        );
        assert_eq!(ack_event_attr(write_ack_event, "ack"), "ack_payload");
        assert_eq!(ack_event_attr(write_ack_event, "ack_type"), "success");
        assert_eq!(
            ack_event_attr(write_ack_event, "version"),
            euclid_encoding::PROTOCOL_VERSION
        );
        assert_eq!(ack_event_attr(write_ack_event, "encoding"), "0");

        // set_data keeps the internal JSON form.
        assert_eq!(res.data, Some(Binary::from(ack_bytes.as_slice())));

        // The in-flight context is consumed by the reply.
        assert!(crate::relay_state::IN_FLIGHT_RECEIVE
            .may_load(&deps.storage)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_on_cross_chain_receive_reply_ok_abi_leg_transcodes_ack() {
        use euclid_encoding::Encoding;
        use euclid_ibc::wire::envelope::AcknowledgementMsg;
        use euclid_ibc::wire::msgs::ReleaseEscrowAckMsg;
        use euclid_ibc::wire::transcode;

        let mut deps = mock_dependencies();
        init(&mut deps);
        // wire_tag 1 = TAG_RELEASE_ESCROW, encoding 1 = Abi.
        seed_in_flight_receive(&mut deps, 11, 1, 1);

        let ack_json = cosmwasm_std::to_json_binary(&AcknowledgementMsg::Ok(ReleaseEscrowAckMsg {
            amount: cosmwasm_std::Uint256::from(800u128),
            to_address: "cosmos1recipient".to_string(),
            escrow_balance: cosmwasm_std::Uint256::zero(),
        }))
        .unwrap();
        let proto = encode_execute_response(&ack_json);
        let msg = ok_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, proto);

        let res = on_cross_chain_receive_reply(deps.as_mut(), msg).unwrap();

        let write_ack_event = &res.events[1];
        assert_eq!(
            write_ack_event.ty,
            euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT
        );
        assert_eq!(ack_event_attr(write_ack_event, "sequence"), "11");
        assert_eq!(ack_event_attr(write_ack_event, "ack_type"), "success");
        assert_eq!(ack_event_attr(write_ack_event, "encoding"), "1");
        // Ports swapped versus the incoming ReceivePacket.
        assert_eq!(
            ack_event_attr(write_ack_event, "source_port"),
            IN_FLIGHT_DESTINATION_PORT
        );
        assert_eq!(
            ack_event_attr(write_ack_event, "destination_port"),
            IN_FLIGHT_SOURCE_PORT
        );
        // Abi leg: the msg attribute is 0x lowercase hex of the stored wire
        // bytes.
        assert_eq!(
            ack_event_attr(write_ack_event, "msg"),
            euclid_encoding::repr::to_transport_string(IN_FLIGHT_MSG, Encoding::Abi).unwrap()
        );

        // The ack attribute is 0x hex wire bytes that transcode back to the
        // internal JSON form.
        let ack_attr = ack_event_attr(write_ack_event, "ack");
        assert!(ack_attr.starts_with("0x"), "ack must be 0x hex");
        let wire = Binary::new(
            euclid_encoding::repr::from_transport_string(&ack_attr, Encoding::Abi)
                .expect("ack must be 0x hex"),
        );
        let back = transcode::factory_ack_wire_to_json(1, &wire, Encoding::Abi).unwrap();
        assert_eq!(back, ack_json);

        // set_data stays on the JSON form.
        assert_eq!(res.data, Some(ack_json));
    }

    #[test]
    fn test_on_cross_chain_receive_reply_err_emits_error_payload_event() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_in_flight_receive(&mut deps, 9, 0, 1);

        let err_str = "receive processing error";
        let msg = err_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, err_str);
        let res = on_cross_chain_receive_reply(deps.as_mut(), msg).unwrap();

        assert_eq!(res.attributes[0].key, "reply_on_receive_processing");
        assert_eq!(res.attributes[0].value, "error");
        assert_eq!(res.attributes[1].key, "error");
        assert_eq!(res.attributes[1].value, err_str);

        // Two events: euclid event + the single complete acknowledgement event
        assert_eq!(res.events.len(), 2);
        let write_ack_event = &res.events[1];
        assert_eq!(
            write_ack_event.ty,
            euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT
        );
        let keys: Vec<&str> = write_ack_event
            .attributes
            .iter()
            .map(|a| a.key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec![
                "msg",
                "sequence",
                "source_port",
                "destination_port",
                "ack",
                "destination_chain_type",
                "ack_type",
                "version",
                "encoding"
            ]
        );
        // Ports swapped versus the incoming ReceivePacket.
        assert_eq!(
            ack_event_attr(write_ack_event, "source_port"),
            IN_FLIGHT_DESTINATION_PORT
        );
        assert_eq!(
            ack_event_attr(write_ack_event, "destination_port"),
            IN_FLIGHT_SOURCE_PORT
        );
        assert_eq!(ack_event_attr(write_ack_event, "sequence"), "9");
        let expected_fail = make_ack_fail(err_str.to_string()).unwrap();
        assert_eq!(
            ack_event_attr(write_ack_event, "ack"),
            String::from_utf8(expected_fail.to_vec()).unwrap()
        );
        assert_eq!(ack_event_attr(write_ack_event, "ack_type"), "error");
        assert_eq!(ack_event_attr(write_ack_event, "encoding"), "0");

        assert!(crate::relay_state::IN_FLIGHT_RECEIVE
            .may_load(&deps.storage)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_on_cross_chain_receive_reply_without_in_flight_context_errors() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let proto = encode_execute_response(b"ack_payload");
        let msg = ok_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, proto);
        assert!(on_cross_chain_receive_reply(deps.as_mut(), msg).is_err());
    }

    // -----------------------------------------------------------------------
    // on_pool_factory_delegate_reply
    // -----------------------------------------------------------------------

    fn cross_chain_user(addr: &str) -> euclid::cross_chain_user::CrossChainUser {
        euclid::cross_chain_user::CrossChainUser {
            chain_uid: euclid::chain::ChainUid::create(
                crate::testing::helpers::TEST_CHAIN_UID.to_string(),
            )
            .unwrap(),
            address: addr.to_string(),
        }
    }

    fn pool_creation_packet(sender_addr: &str) -> Binary {
        let pair = euclid::token::PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
        };
        cosmwasm_std::to_json_binary(&RouterReceiveMsg::RequestPoolCreation(
            RequestPoolCreationSendMsg {
                sender: cross_chain_user(sender_addr),
                tx_id: "tx_test".to_string(),
                pair,
                pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
                slippage_tolerance_bps: 50,
            },
        ))
        .unwrap()
    }

    fn swap_packet(sender_addr: &str) -> Binary {
        let token = Token::create("aaa".to_string()).unwrap();
        cosmwasm_std::to_json_binary(&RouterReceiveMsg::Swap(
            euclid_ibc::wire::msgs::SwapSendMsg {
                sender: cross_chain_user(sender_addr),
                tx_id: "tx_swap".to_string(),
                asset_in: euclid::token::TokenWithDenom {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uaaa".to_string(),
                        decimals: None,
                    },
                },
                amount_in: cosmwasm_std::Uint256::from(1u128),
                asset_out: token,
                min_amount_out: cosmwasm_std::Uint256::from(1u128),
                swaps: vec![],
                recipients: vec![],
                partner_fee_amount: cosmwasm_std::Uint256::zero(),
                partner_fee_recipient: cross_chain_user(sender_addr),
            },
        ))
        .unwrap()
    }

    /// Wraps an inner payload in the protobuf MsgExecuteContractResponse
    /// envelope that a real SubMsg::reply_on_success would deliver. The
    /// `encode_execute_response` helper at the top of this test module
    /// produces the same encoding the CosmWasm VM emits for a successful
    /// execute SubMsg.
    fn reply_with_data(inner: Binary) -> Reply {
        let envelope = Binary::from(encode_execute_response(inner.as_slice()));
        Reply {
            id: POOL_FACTORY_DELEGATE_REPLY_ID,
            payload: Binary::default(),
            #[allow(deprecated)]
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                #[allow(deprecated)]
                data: Some(envelope),
                msg_responses: vec![],
            }),
        }
    }

    #[test]
    fn test_on_pool_factory_delegate_reply_happy_path_emits_submsg() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let user = deps.api.addr_make("user");
        let payload = cosmwasm_std::to_json_binary(
            &euclid::msgs::pool_factory::PoolFactoryReply::SendPacket {
                msg: pool_creation_packet(user.as_str()),
                timeout: None,
                ack_response: None,
                sender: user.clone(),
            },
        )
        .unwrap();

        let res =
            on_pool_factory_delegate_reply(deps.as_mut(), mock_env(), reply_with_data(payload))
                .unwrap();

        // One submsg emitted (the to_msg() output for the outbound packet).
        assert_eq!(res.messages.len(), 1);
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "on_pool_factory_delegate_reply"));
        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "tx_id" && a.value == "tx_test"));
    }

    #[test]
    fn test_on_pool_factory_delegate_reply_missing_data_errors() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let reply = Reply {
            id: POOL_FACTORY_DELEGATE_REPLY_ID,
            payload: Binary::default(),
            #[allow(deprecated)]
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                #[allow(deprecated)]
                data: None,
                msg_responses: vec![],
            }),
        };

        let err = on_pool_factory_delegate_reply(deps.as_mut(), mock_env(), reply).unwrap_err();
        match err {
            ContractError::Reply { action, err } => {
                assert_eq!(action, "on_pool_factory_delegate_reply");
                assert!(err.contains("missing data"), "unexpected err: {err}");
            }
            other => panic!("expected ContractError::Reply, got {other:?}"),
        }
    }

    #[test]
    fn test_on_pool_factory_delegate_reply_undecodable_data_errors() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        // Bytes that are not a valid PoolFactoryReply.
        let garbage = Binary::from(b"not-json-and-not-pool-factory-reply".as_slice());

        let err =
            on_pool_factory_delegate_reply(deps.as_mut(), mock_env(), reply_with_data(garbage))
                .unwrap_err();
        match err {
            ContractError::Reply { action, err } => {
                assert_eq!(action, "on_pool_factory_delegate_reply");
                assert!(
                    err.contains("failed to decode PoolFactoryReply"),
                    "unexpected err: {err}"
                );
            }
            other => panic!("expected ContractError::Reply, got {other:?}"),
        }
    }

    #[test]
    fn test_on_pool_factory_delegate_reply_non_pool_variant_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let user = deps.api.addr_make("user");
        let payload = cosmwasm_std::to_json_binary(
            &euclid::msgs::pool_factory::PoolFactoryReply::SendPacket {
                msg: swap_packet(user.as_str()),
                timeout: None,
                ack_response: None,
                sender: user,
            },
        )
        .unwrap();

        let err =
            on_pool_factory_delegate_reply(deps.as_mut(), mock_env(), reply_with_data(payload))
                .unwrap_err();
        match err {
            ContractError::Reply { action, err } => {
                assert_eq!(action, "on_pool_factory_delegate_reply");
                assert!(err.contains("non-pool"), "unexpected err: {err}");
            }
            other => panic!("expected ContractError::Reply, got {other:?}"),
        }
    }
}
