use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, DepsMut, Env, HexBinary, MessageInfo, QueryRequest,
    Response, Timestamp, Uint128, WasmMsg, WasmQuery,
};
use euclid::chain::ChainType;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::{
    MetaTransaction, MetaTransactionData, State, UpdateAdminMsg, UpdateStateMsg,
};
use euclid::msgs::router::{self, ChainResponse};
use relayer::verify::{
    cosmos_address_from_pubkey, eth_address_from_pubkey, verify_keccak256_signature,
    verify_signature, MsgSignData,
};

use crate::state::{AUTHORIZED_ADDRESSES, NONCES, STATE};

pub fn execute_update_state(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateStateMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut response = Response::new();

    if let Some(authorized_addresses) = msg.authorized_addresses {
        AUTHORIZED_ADDRESSES.save(deps.storage, &authorized_addresses)?;
        response = response.add_attribute("updated_authorized_addresses", "true");
    }

    STATE.save(deps.storage, &state)?;
    Ok(response)
}

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

/// Helper function to process a single meta transaction and return the WasmMsg to execute
fn process_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    state: &State,
    msg: &MetaTransaction,
) -> Result<(WasmMsg, MetaTransactionData), ContractError> {
    // First, we need to parse the data to get the chain_uid to determine chain type
    // Try to parse as MsgSignData first (Cosmos format), if that fails try direct parsing (EVM format)
    let data_binary = Binary::from(msg.data.as_bytes());
    let (meta_transaction, data_for_verification) =
        if let Ok(signed_data) = from_json::<MsgSignData>(&data_binary) {
            // Cosmos/IBC format: data is wrapped in MsgSignData
            let first_msg = signed_data
                .msgs
                .first()
                .ok_or(ContractError::new("No messages found"))?
                .clone()
                .value;
            let meta_transaction: MetaTransactionData = from_json(first_msg.data.clone())?;
            (meta_transaction, msg.data.clone())
        } else {
            // EVM format: data is direct MetaTransactionData JSON string
            let meta_transaction: MetaTransactionData = from_json(&data_binary).map_err(|e| {
                ContractError::new(&format!("Failed to parse meta transaction data: {}", e))
            })?;
            (meta_transaction, msg.data.clone())
        };

    // Create sender key: chainuid:address
    let sender_key = format!(
        "{}:{}",
        meta_transaction.chain_uid_src_chain.as_str(),
        meta_transaction.signer_address_src_chain
    );

    // Ensure the nonce is not used for this sender
    ensure!(
        !NONCES.has(
            deps.storage,
            (sender_key.clone(), meta_transaction.nonce.clone())
        ),
        ContractError::new(
            format!(
                "Nonce already used for sender {}: {}",
                sender_key, meta_transaction.nonce
            )
            .as_str()
        )
    );
    // Save the nonce for this sender
    NONCES.save(
        deps.storage,
        (sender_key.clone(), meta_transaction.nonce.clone()),
        &Uint128::from(env.block.height),
    )?;

    // Ensure the timestamp is not exceeded
    ensure!(
        env.block.time <= Timestamp::from_seconds(meta_transaction.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    // Get chain type from router
    let chain_type = deps
        .querier
        .query::<ChainResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: state.router_contract.to_string(),
            msg: to_json_binary(&euclid::msgs::router::QueryMsg::GetChain {
                chain_uid: meta_transaction.chain_uid_src_chain.clone(),
            })?,
        }))?
        .chain
        .chain_type;

    // Verify signature based on chain type
    let signature_valid = match &chain_type {
        ChainType::Evm(_) => {
            // For EVM: use Keccak256 with Ethereum signed message format
            let prefix = "\x19Ethereum Signed Message:\n";
            let msg_length = data_for_verification.len().to_string();
            let combined_msg = format!("{}{}{}", prefix, msg_length, data_for_verification);

            // Convert Binary to HexBinary for EVM verification
            let signature_hex = HexBinary::from(msg.signature.as_slice());
            let pubkey_hex = HexBinary::from(meta_transaction.pubkey_singer.as_slice());

            verify_keccak256_signature(deps.as_ref(), &combined_msg, &signature_hex, &pubkey_hex)?
        }
        ChainType::Ibc(_) | ChainType::Native {} => {
            // For Cosmos: use SHA256
            verify_signature(
                deps.as_ref(),
                &data_for_verification,
                &msg.signature,
                &meta_transaction.pubkey_singer,
            )?
        }
        ChainType::Solana(_) => {
            return Err(ContractError::new(
                "Solana chain type not yet supported for meta transactions",
            ));
        }
    };

    ensure!(signature_valid, ContractError::new("Invalid signature"));

    // Derive address from public key and verify it matches the claimed address
    let derived_address = match chain_type {
        ChainType::Ibc(_) | ChainType::Native {} => {
            // Extract bech32 prefix from the provided address
            let prefix = meta_transaction
                .signer_address_src_chain
                .split('1')
                .next()
                .ok_or_else(|| ContractError::new("Invalid bech32 address format"))?;

            cosmos_address_from_pubkey(&meta_transaction.pubkey_singer, prefix).map_err(|e| {
                ContractError::new(&format!("Failed to derive cosmos address: {}", e))
            })?
        }
        ChainType::Evm(_) => eth_address_from_pubkey(&meta_transaction.pubkey_singer)
            .map_err(|e| ContractError::new(&format!("Failed to derive EVM address: {}", e)))?,
        ChainType::Solana(_) => {
            return Err(ContractError::new(
                "Solana chain type not yet supported for meta transactions",
            ));
        }
    };

    // Compare derived address with claimed address (case-insensitive for EVM)
    let addresses_match = if matches!(chain_type, ChainType::Evm(_)) {
        derived_address.to_lowercase() == meta_transaction.signer_address_src_chain.to_lowercase()
    } else {
        derived_address == meta_transaction.signer_address_src_chain
    };

    ensure!(
        addresses_match,
        ContractError::new(&format!(
            "Address mismatch: derived '{}' does not match claimed '{}'",
            derived_address, meta_transaction.signer_address_src_chain
        ))
    );

    let router_execute_msg: router::ExecuteMsg =
        cosmwasm_std::from_json(&meta_transaction.call_data)
            .map_err(|_| ContractError::new("call_data is not a valid Router ExecuteMsg"))?;

    // This contract can only call voucher related messages on the router contract
    match router_execute_msg {
        router::ExecuteMsg::WithdrawVoucher { .. } => {}
        _ => {
            return Err(ContractError::Generic {
                err: "Invalid router execute message".to_string(),
            });
        }
    };

    let relay_msg = WasmMsg::Execute {
        contract_addr: state.router_contract.to_string(),
        msg: meta_transaction.call_data.clone(),
        funds: vec![],
    };

    Ok((relay_msg, meta_transaction))
}

pub fn execute_execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: MetaTransaction,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let (relay_msg, meta_transaction) = process_meta_transaction(deps, env, &state, &msg)?;

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("relayer_nonce", meta_transaction.nonce)
        .add_attribute("relayer_sender", info.sender.to_string()))
}

pub fn execute_execute_meta_transaction_batch(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    transactions: Vec<MetaTransaction>,
) -> Result<Response, ContractError> {
    ensure!(
        !transactions.is_empty(),
        ContractError::new("Batch cannot be empty")
    );

    let state = STATE.load(deps.storage)?;
    let mut response = Response::new();
    let mut nonces = Vec::new();

    for (idx, transaction) in transactions.iter().enumerate() {
        let (relay_msg, meta_transaction) =
            process_meta_transaction(deps, env, &state, transaction).map_err(|e| {
                ContractError::new(&format!("Failed to process transaction {}: {}", idx, e))
            })?;

        response = response.add_message(relay_msg);
        nonces.push(meta_transaction.nonce);
    }

    Ok(response
        .add_attribute("relayer_nonces", nonces.join(","))
        .add_attribute("relayer_sender", info.sender.to_string())
        .add_attribute("batch_size", transactions.len().to_string()))
}
