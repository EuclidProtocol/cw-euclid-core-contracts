use cosmwasm_std::{
    ensure, to_json_binary, to_json_string, Binary, DepsMut, Env, HexBinary, MessageInfo,
    QueryRequest, Response, Timestamp, Uint128, WasmMsg, WasmQuery,
};
use euclid::chain::{ChainType, CrossChainUser};
use euclid::error::ContractError;
use euclid::msgs::hook::MetaReceive;
use euclid::msgs::meta_transaction::{MetaTransaction, UpdateAdminMsg};
use euclid::msgs::router;
use relayer::verify::{
    add_eth_prefix, cosmos_address_from_pubkey, eth_address_from_pubkey, msg_to_sign_data,
    verify_keccak256_signature, verify_signature,
};

use crate::state::{NONCES, STATE};

pub fn execute_update_admin(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateAdminMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    // Ensure the sender is the current admin
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    deps.api.addr_validate(msg.new_admin.as_str())?;

    state.admin = msg.new_admin.clone();
    STATE.save(deps.storage, &state)?;
    Ok(Response::new()
        .add_attribute("old_admin", state.admin.to_string())
        .add_attribute("new_admin", msg.new_admin.to_string()))
}

pub fn execute_execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    meta_transaction: MetaTransaction,
) -> Result<Response, ContractError> {
    // Ensure the timestamp is not exceeded
    ensure!(
        env.block.time <= Timestamp::from_seconds(meta_transaction.data.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    let state = STATE.load(deps.storage)?;
    // Get chain type from router
    let chain_type = deps
        .querier
        .query::<router::ChainResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: state.router_contract.to_string(),
            msg: to_json_binary(&euclid::msgs::router::QueryMsg::GetChain {
                chain_uid: meta_transaction.data.signer_chain_uid.clone(),
            })?,
        }))?
        .chain
        .chain_type;

    // Derive address from public key and verify it matches the claimed address
    let derived_address = match chain_type {
        ChainType::Ibc(_) | ChainType::Native {} => {
            let pubkey = Binary::from_base64(meta_transaction.signer_pubkey.as_str())?;
            let bech32 = meta_transaction.data.signer_prefix.clone();

            let data_binary = to_json_binary(&meta_transaction.data)?;
            let msg_sign_data =
                msg_to_sign_data(data_binary, meta_transaction.data.signer_address.clone());
            let msg_sign_data_str = to_json_string(&msg_sign_data)?;

            let verified = verify_signature(
                deps.as_ref(),
                &msg_sign_data_str,
                &Binary::from_base64(meta_transaction.signature.as_str())?,
                &pubkey,
            )?;
            ensure!(verified, ContractError::new("Invalid signature"));

            cosmos_address_from_pubkey(&pubkey, &bech32).map_err(|e| {
                ContractError::new(&format!("Failed to derive cosmos address: {}", e))
            })?
        }
        ChainType::Evm(_) => {
            let pubkey = HexBinary::from_hex(meta_transaction.signer_pubkey.as_str())?;
            let prefixed_msg = add_eth_prefix(&to_json_string(&meta_transaction.data)?);
            let verified = verify_keccak256_signature(
                deps.as_ref(),
                &prefixed_msg,
                &HexBinary::from_hex(meta_transaction.signature.as_str())?,
                &pubkey,
            )?;
            ensure!(verified, ContractError::new("Invalid signature"));
            eth_address_from_pubkey(&pubkey)
                .map_err(|e| ContractError::new(&format!("Failed to derive EVM address: {}", e)))?
        }
        ChainType::Solana(_) => {
            return Err(ContractError::new(
                "Solana chain type not yet supported for meta transactions",
            ));
        }
    };

    ensure!(
        derived_address == meta_transaction.data.signer_address,
        ContractError::new(&format!(
            "Address mismatch: derived '{}' does not signed address '{}'",
            derived_address, meta_transaction.data.signer_address
        ))
    );
    // Create sender key: chainuid:address
    let sender_key = format!(
        "{}:{}",
        meta_transaction.data.signer_chain_uid.as_str(),
        meta_transaction.data.signer_address
    );

    // Ensure the nonce is not used for this sender
    if let Some(blockheight) = NONCES.may_load(
        deps.storage,
        (sender_key.clone(), meta_transaction.data.nonce.clone()),
    )? {
        return Err(ContractError::new(
            format!(
                "Nonce already used for sender {}: {} at block height {}",
                sender_key, meta_transaction.data.nonce, blockheight
            )
            .as_str(),
        ));
    }
    // Save the nonce for this sender
    NONCES.save(
        deps.storage,
        (sender_key.clone(), meta_transaction.data.nonce.clone()),
        &Uint128::from(env.block.height),
    )?;

    let mut response = Response::new()
        .add_attribute("meta_sender_key", sender_key)
        .add_attribute("meta_broadcaster", info.sender.to_string());

    let verified_sender = CrossChainUser::new(
        meta_transaction.data.signer_chain_uid.clone(),
        meta_transaction.data.signer_address.clone(),
    );

    for call_data in meta_transaction.data.call_data {
        let meta_receive = MetaReceive {
            verified_sender: verified_sender.clone(),
            call_data: call_data.call_data.clone(),
        };
        response = response.add_message(WasmMsg::Execute {
            contract_addr: call_data.target.to_string(),
            msg: meta_receive.to_receiver_msg()?,
            funds: vec![],
        });
    }

    Ok(response)
}
