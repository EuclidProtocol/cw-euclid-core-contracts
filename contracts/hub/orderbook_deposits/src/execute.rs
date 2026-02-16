use cosmwasm_std::{
    attr, ensure, from_json, to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, Timestamp, Uint128, WasmMsg,
};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    msgs::{
        hook::VoucherReceive,
        virtual_balance::{ExecuteMsg as VirtualBalanceExecuteMsg, ExecuteTransfer},
    },
    token::Token,
};
use relayer::verify::{verify_signature, MsgSignData};
use sha2::{Digest, Sha256};

use crate::{
    error::ContractError,
    msg::{
        ExecuteMsg, MerkleProofStep, Permit, PermitData, ProofPosition,
        VirtualBalanceReceiveHookMsg, WithdrawalLeaf,
    },
    state::{
        AssetTotal, OrderbookDepositsStatus, RootConfig, RootInfo, ASSET_DEPOSITS, CURRENT_ROOT,
        NULLIFIERS, PENDING_ROOT, ROOT_CONFIG, STATE, USED_PERMITS, USER_DEPOSITS,
        WHITELISTED_ASSETS,
    },
};

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SetWhitelist {
            token_id,
            whitelisted,
        } => execute_set_whitelist(deps, info, token_id, whitelisted),

        ExecuteMsg::VoucherReceive(msg) => execute_virtual_balance_receive(deps, env, info, msg),
        ExecuteMsg::UpdateConfig {
            admin,
            status,
            root_challenge_period,
            permit_signer_pubkey,
            permit_signer_address,
            authorized_posters,
        } => execute_update_config(
            deps,
            info,
            admin,
            status,
            root_challenge_period,
            permit_signer_pubkey,
            permit_signer_address,
            authorized_posters,
        ),
        ExecuteMsg::ProposeRoot {
            root_id,
            root_hash,
            per_asset_totals,
            da_hash,
            da_url,
        } => execute_propose_root(
            deps,
            env,
            info,
            root_id,
            root_hash,
            per_asset_totals,
            da_hash,
            da_url,
        ),
        ExecuteMsg::ActivateRoot { root_id } => execute_activate_root(deps, env, info, root_id),
        ExecuteMsg::Withdraw {
            root_id,
            amount,
            nonce,
            leaf,
            proof,
            permit,
            destination_chain_uid,
            destination,
        } => execute_withdraw(
            deps,
            env,
            root_id,
            amount,
            nonce,
            leaf,
            proof,
            permit,
            destination_chain_uid,
            destination,
        ),
    }
}

fn execute_virtual_balance_receive(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    transfer: VoucherReceive,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.virtual_balance,
        ContractError::Unauthorized {}
    );
    ensure!(
        matches!(state.status, OrderbookDepositsStatus::Active),
        ContractError::ContractPaused {}
    );

    let hook: VirtualBalanceReceiveHookMsg = from_json(transfer.msg.clone())?;
    match hook {
        VirtualBalanceReceiveHookMsg::Deposit {} => {
            execute_deposit(deps, transfer.token_id, transfer.amount, transfer.sender)
        }
    }
}

fn execute_deposit(
    deps: DepsMut,
    token_id: String,
    amount: Uint128,
    sender: CrossChainUser,
) -> Result<Response, ContractError> {
    ensure!(!amount.is_zero(), ContractError::InvalidAmount {});

    let is_whitelisted = WHITELISTED_ASSETS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or(false);
    ensure!(is_whitelisted, ContractError::AssetNotWhitelisted {});

    let _ = Token::create(token_id.clone())?;

    let sender_addr = &sender.address;

    // Update aggregate and user-level deposit tracking.
    let new_asset_total = ASSET_DEPOSITS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    ASSET_DEPOSITS.save(deps.storage, token_id.clone(), &new_asset_total)?;

    let user_key = (sender_addr.clone(), token_id.clone());
    let new_user_total = USER_DEPOSITS
        .may_load(deps.storage, user_key.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    USER_DEPOSITS.save(deps.storage, user_key, &new_user_total)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "deposit"),
        attr("token_id", token_id),
        attr("amount", amount.to_string()),
        attr("sender", sender_addr.as_str()),
    ]))
}

fn execute_set_whitelist(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
    whitelisted: bool,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let token = Token::create(token_id.clone())?;
    WHITELISTED_ASSETS.save(deps.storage, token.to_string(), &whitelisted)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "set_whitelist"),
        attr("token_id", token_id),
        attr("whitelisted", whitelisted.to_string()),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    admin: Option<String>,
    status: Option<OrderbookDepositsStatus>,
    root_challenge_period: Option<u64>,
    permit_signer_pubkey: Option<Binary>,
    permit_signer_address: Option<String>,
    authorized_posters: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    if let Some(admin) = admin {
        state.admin = deps.api.addr_validate(&admin)?;
    }
    if let Some(status) = status {
        state.status = status;
    }

    let mut config = ROOT_CONFIG.load(deps.storage)?;
    if let Some(root_challenge_period) = root_challenge_period {
        config.root_challenge_period = root_challenge_period;
    }
    if let Some(pubkey) = permit_signer_pubkey {
        config.permit_signer_pubkey = Some(pubkey);
    }
    if let Some(address) = permit_signer_address {
        config.permit_signer_address = Some(address);
    }
    if let Some(posters) = authorized_posters {
        let posters = posters
            .into_iter()
            .map(|poster| deps.api.addr_validate(&poster))
            .collect::<Result<Vec<_>, _>>()?;
        config.authorized_posters = if posters.is_empty() {
            vec![state.admin.clone()]
        } else {
            posters
        };
    }

    STATE.save(deps.storage, &state)?;
    ROOT_CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

#[allow(clippy::too_many_arguments)]
fn execute_propose_root(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    root_id: String,
    root_hash: Binary,
    per_asset_totals: Vec<AssetTotal>,
    da_hash: Option<Binary>,
    da_url: Option<String>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        matches!(state.status, OrderbookDepositsStatus::Active),
        ContractError::ContractPaused {}
    );
    let config = ROOT_CONFIG.load(deps.storage)?;
    ensure!(
        is_authorized_poster(&state, &config, &info.sender),
        ContractError::Unauthorized {}
    );
    ensure!(root_hash.len() == 32, ContractError::InvalidRootHash {});

    validate_asset_totals(deps.as_ref(), &per_asset_totals)?;

    let root = RootInfo {
        root_id: root_id.clone(),
        root_hash,
        per_asset_totals,
        da_hash,
        da_url,
        proposed_at: env.block.time.seconds(),
    };

    let root_state = if config.root_challenge_period > 0 {
        PENDING_ROOT.save(deps.storage, &root)?;
        "pending"
    } else {
        CURRENT_ROOT.save(deps.storage, &root)?;
        PENDING_ROOT.remove(deps.storage);
        "active"
    };

    Ok(Response::new().add_attributes(vec![
        attr("action", "propose_root"),
        attr("root_id", root_id),
        attr("root_state", root_state),
    ]))
}

fn execute_activate_root(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    root_id: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        matches!(state.status, OrderbookDepositsStatus::Active),
        ContractError::ContractPaused {}
    );
    let config = ROOT_CONFIG.load(deps.storage)?;
    ensure!(
        is_authorized_poster(&state, &config, &info.sender),
        ContractError::Unauthorized {}
    );

    let pending = PENDING_ROOT
        .may_load(deps.storage)?
        .ok_or(ContractError::PendingRootNotFound {})?;
    ensure!(pending.root_id == root_id, ContractError::RootIdMismatch {});

    let ready_at = pending
        .proposed_at
        .saturating_add(config.root_challenge_period);
    ensure!(
        env.block.time.seconds() >= ready_at,
        ContractError::RootNotReady {}
    );

    CURRENT_ROOT.save(deps.storage, &pending)?;
    PENDING_ROOT.remove(deps.storage);

    Ok(Response::new().add_attributes(vec![
        attr("action", "activate_root"),
        attr("root_id", root_id),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    root_id: String,
    amount: Uint128,
    nonce: u64,
    leaf: WithdrawalLeaf,
    proof: Vec<MerkleProofStep>,
    permit: Permit,
    destination_chain_uid: String,
    destination: String,
) -> Result<Response, ContractError> {
    ensure!(!amount.is_zero(), ContractError::InvalidAmount {});
    ensure!(
        !destination_chain_uid.is_empty(),
        ContractError::InvalidDestination {}
    );
    ensure!(
        !destination.is_empty(),
        ContractError::InvalidDestination {}
    );

    let state = STATE.load(deps.storage)?;
    ensure!(
        matches!(state.status, OrderbookDepositsStatus::Active),
        ContractError::ContractPaused {}
    );

    let current_root = CURRENT_ROOT
        .may_load(deps.storage)?
        .ok_or(ContractError::RootNotFound {})?;
    ensure!(
        current_root.root_id == root_id,
        ContractError::RootIdMismatch {}
    );
    ensure!(
        current_root.root_hash.len() == 32,
        ContractError::InvalidRootHash {}
    );

    ensure!(!leaf.user.is_empty(), ContractError::InvalidLeaf {});
    ensure!(!leaf.token_id.is_empty(), ContractError::InvalidLeaf {});

    let is_whitelisted = WHITELISTED_ASSETS
        .may_load(deps.storage, leaf.token_id.clone())?
        .unwrap_or(false);
    ensure!(is_whitelisted, ContractError::AssetNotWhitelisted {});

    Token::create(leaf.token_id.clone())?;

    let config = ROOT_CONFIG.load(deps.storage)?;
    let permit_data = verify_permit(
        deps.as_ref(),
        &env,
        &config,
        &root_id,
        amount,
        nonce,
        &leaf,
        &destination_chain_uid,
        &destination,
        &permit,
    )?;

    let permit_key = permit_id(&permit);
    ensure!(
        !USED_PERMITS
            .may_load(deps.storage, permit_key.clone())?
            .unwrap_or(false),
        ContractError::PermitAlreadyUsed {}
    );
    USED_PERMITS.save(deps.storage, permit_key, &true)?;

    let leaf_hash = hash_leaf(&leaf)?;
    let computed_root = apply_merkle_proof(leaf_hash, &proof)?;
    ensure!(
        computed_root.as_ref() == current_root.root_hash.as_slice(),
        ContractError::InvalidMerkleProof {}
    );

    let nullifier_key = nullifier_key(&root_id, &permit_data.user, &permit_data.token_id, nonce);
    let already_withdrawn = NULLIFIERS
        .may_load(deps.storage, nullifier_key.clone())?
        .unwrap_or_default();
    let remaining = leaf
        .balance
        .checked_sub(already_withdrawn)
        .map_err(|_| ContractError::InsufficientWithdrawableBalance {})?;
    ensure!(
        amount <= remaining,
        ContractError::InsufficientWithdrawableBalance {}
    );
    let new_withdrawn = already_withdrawn
        .checked_add(amount)
        .map_err(StdError::from)?;
    NULLIFIERS.save(deps.storage, nullifier_key, &new_withdrawn)?;

    let asset_total = ASSET_DEPOSITS
        .may_load(deps.storage, permit_data.token_id.clone())?
        .unwrap_or_default();
    let new_asset_total = asset_total
        .checked_sub(amount)
        .map_err(|_| ContractError::InsufficientEscrow {})?;
    if new_asset_total.is_zero() {
        ASSET_DEPOSITS.remove(deps.storage, permit_data.token_id.clone());
    } else {
        ASSET_DEPOSITS.save(deps.storage, permit_data.token_id.clone(), &new_asset_total)?;
    }

    let user_key = (permit_data.user.clone(), permit_data.token_id.clone());
    let user_total = USER_DEPOSITS
        .may_load(deps.storage, user_key.clone())?
        .unwrap_or_default();
    let new_user_total = user_total
        .checked_sub(amount)
        .unwrap_or_else(|_| Uint128::zero());
    if new_user_total.is_zero() {
        USER_DEPOSITS.remove(deps.storage, user_key);
    } else {
        USER_DEPOSITS.save(deps.storage, user_key, &new_user_total)?;
    }

    let destination_chain_uid = ChainUid::create(destination_chain_uid)?;
    let destination_user = CrossChainUser::new(destination_chain_uid, destination.clone());
    let transfer_msg = VirtualBalanceExecuteMsg::Transfer(ExecuteTransfer {
        amount,
        token_id: permit_data.token_id.clone(),
        sender: None,
        to: destination_user,
        from: None,
        msg: None,
    });

    let send_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: state.virtual_balance.to_string(),
        msg: to_json_binary(&transfer_msg)?,
        funds: vec![],
    }
    .into();

    Ok(Response::new().add_message(send_msg).add_attributes(vec![
        attr("action", "withdrawal_completed"),
        attr("root_id", root_id),
        attr("user", permit_data.user),
        attr("token_id", permit_data.token_id),
        attr("amount", amount.to_string()),
        attr("nonce", nonce.to_string()),
        attr("destination_chain_uid", permit_data.destination_chain_uid),
        attr("destination", destination),
    ]))
}

fn is_authorized_poster(
    state: &crate::state::State,
    config: &RootConfig,
    sender: &cosmwasm_std::Addr,
) -> bool {
    sender == state.admin || config.authorized_posters.contains(sender)
}

fn validate_asset_totals(deps: Deps, per_asset_totals: &[AssetTotal]) -> Result<(), ContractError> {
    for total in per_asset_totals {
        let is_whitelisted = WHITELISTED_ASSETS
            .may_load(deps.storage, total.token_id.clone())?
            .unwrap_or(false);
        ensure!(is_whitelisted, ContractError::AssetNotWhitelisted {});

        let on_chain_total = ASSET_DEPOSITS
            .may_load(deps.storage, total.token_id.clone())?
            .unwrap_or_default();
        ensure!(
            on_chain_total >= total.amount,
            ContractError::InsufficientEscrow {}
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn verify_permit(
    deps: Deps,
    env: &Env,
    config: &RootConfig,
    root_id: &str,
    amount: Uint128,
    nonce: u64,
    leaf: &WithdrawalLeaf,
    destination_chain_uid: &str,
    destination: &str,
    permit: &Permit,
) -> Result<PermitData, ContractError> {
    let pubkey = config
        .permit_signer_pubkey
        .as_ref()
        .ok_or(ContractError::PermitSignerNotConfigured {})?;
    let signer_address = config
        .permit_signer_address
        .as_ref()
        .ok_or(ContractError::PermitSignerNotConfigured {})?;

    let signed_data: MsgSignData = from_json(permit.data.clone())?;
    let first_msg = signed_data
        .msgs
        .first()
        .ok_or(ContractError::InvalidPermit {})?
        .clone()
        .value;

    ensure!(
        first_msg.signer == *signer_address,
        ContractError::InvalidPermit {}
    );

    let permit_data: PermitData = from_json(first_msg.data.clone())?;
    ensure!(
        permit_data.root_id == root_id
            && permit_data.user == leaf.user
            && permit_data.token_id == leaf.token_id
            && permit_data.amount == amount
            && permit_data.nonce == nonce
            && permit_data.destination_chain_uid == destination_chain_uid
            && permit_data.destination == destination,
        ContractError::InvalidPermit {}
    );

    ensure!(
        env.block.time <= Timestamp::from_seconds(permit_data.expiry),
        ContractError::PermitExpired {}
    );

    let verified = verify_signature(deps, &permit.data, &permit.signature, pubkey)?;
    ensure!(verified, ContractError::InvalidPermit {});

    Ok(permit_data)
}

fn permit_id(permit: &Permit) -> String {
    let digest: [u8; 32] = Sha256::digest(permit.data.as_bytes()).into();
    to_hex(&digest)
}

fn hash_leaf(leaf: &WithdrawalLeaf) -> Result<[u8; 32], ContractError> {
    let bytes = to_json_binary(leaf)?;
    Ok(hash_bytes(bytes.as_slice()))
}

fn apply_merkle_proof(
    leaf_hash: [u8; 32],
    proof: &[MerkleProofStep],
) -> Result<[u8; 32], ContractError> {
    let mut computed = leaf_hash;
    for step in proof {
        ensure!(step.hash.len() == 32, ContractError::InvalidMerkleProof {});
        let mut data = Vec::with_capacity(64);
        match step.position {
            ProofPosition::Left => {
                data.extend_from_slice(step.hash.as_slice());
                data.extend_from_slice(&computed);
            }
            ProofPosition::Right => {
                data.extend_from_slice(&computed);
                data.extend_from_slice(step.hash.as_slice());
            }
        }
        computed = hash_bytes(&data);
    }
    Ok(computed)
}

fn hash_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn nullifier_key(root_id: &str, user: &str, token_id: &str, nonce: u64) -> String {
    let mut data = Vec::with_capacity(
        root_id.len() + user.len() + token_id.len() + std::mem::size_of::<u64>() + 3,
    );
    data.extend_from_slice(root_id.as_bytes());
    data.push(0);
    data.extend_from_slice(user.as_bytes());
    data.push(0);
    data.extend_from_slice(token_id.as_bytes());
    data.push(0);
    data.extend_from_slice(&nonce.to_be_bytes());
    let digest = hash_bytes(&data);
    to_hex(&digest)
}
