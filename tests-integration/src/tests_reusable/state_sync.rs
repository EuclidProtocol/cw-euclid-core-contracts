#![cfg(not(target_arch = "wasm32"))]

use crate::helpers::chains::get_escrow_addr;
use crate::helpers::multi_chain::MultiChainEnv;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint256};
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::base::GetLiquidityQueryResponse;
use euclid::recipient::Recipient;
use euclid::token::{Pair, Token};
use euclid::voucher::BalanceKey;

#[cw_serde]
pub struct VoucherBalanceState {
    pub recipient: CrossChainUser,
    pub token: Token,
    pub amount: Uint256,
}

#[cw_serde]
pub struct UserFundsState {
    pub chain_uid: ChainUid,
    pub user_addr: String,
    pub denom: String,
    pub amount: Uint256,
}

#[cw_serde]
pub struct EscrowBalanceState {
    pub chain_uid: ChainUid,
    pub token: Token,
    pub factory_escrow_balance: Uint256,
    pub router_escrow_balance: Uint256,
}

#[cw_serde]
pub struct VlpBalanceState {
    pub pair: Pair,
    pub vlp_address: String,
    pub token_1_reserve: Uint256,
    pub token_2_reserve: Uint256,
    pub total_lp_tokens: Uint256,
}

#[cw_serde]
pub struct StateSync {
    pub voucher_balances: Vec<VoucherBalanceState>,
    pub user_funds: Vec<UserFundsState>,
    pub escrow_balances: Vec<EscrowBalanceState>,
    pub vlp_balances: Vec<VlpBalanceState>,
}

#[derive(Clone)]
pub struct UserFundsQuery {
    pub chain_uid: ChainUid,
    pub chain_id: String,
    pub user_addr: String,
    pub denom: String,
}

impl StateSync {
    pub fn voucher_balance(&self, recipient: &CrossChainUser, token: &Token) -> Option<Uint256> {
        self.voucher_balances
            .iter()
            .find(|entry| &entry.recipient == recipient && entry.token == token)
            .map(|entry| entry.amount)
    }

    pub fn user_funds(
        &self,
        chain_uid: &ChainUid,
        user_addr: &str,
        denom: &str,
    ) -> Option<Uint256> {
        self.user_funds
            .iter()
            .find(|entry| {
                &entry.chain_uid == chain_uid
                    && entry.user_addr == user_addr
                    && entry.denom == denom
            })
            .map(|entry| entry.amount)
    }

    pub fn escrow_balance(
        &self,
        chain_uid: &ChainUid,
        token: &Token,
    ) -> Option<&EscrowBalanceState> {
        self.escrow_balances
            .iter()
            .find(|entry| &entry.chain_uid == chain_uid && entry.token == token)
    }
}

pub(crate) fn sync_state(
    factory_chain_id: &str,
    factory_addr: &Addr,
    router_chain_id: &str,
    router_addr: &Addr,
    env: &MultiChainEnv,
    recipients: Vec<Recipient>,
    voucher_tokens: Vec<Token>,
    user_funds_queries: Vec<UserFundsQuery>,
    escrow_tokens: Vec<Token>,
    escrow_chain_uid: ChainUid,
    vlp_pairs: Vec<Pair>,
) -> StateSync {
    let factory_app = env.chain(factory_chain_id);
    let router_app = env.chain(router_chain_id);

    let router_state: euclid::msgs::router::StateResponse =
        router_app.query(router_addr, &euclid::msgs::router::QueryMsg::GetState {});
    let virtual_balance_address = router_state.virtual_balance_address;

    let voucher_balances = recipients
        .iter()
        .filter(|recipient| recipient.denom.is_voucher())
        .flat_map(|recipient| {
            voucher_tokens.iter().map(|token| {
                let balance: euclid::msgs::virtual_balance::GetBalanceResponse = router_app.query(
                    &virtual_balance_address,
                    &euclid::msgs::virtual_balance::QueryMsg::GetBalance {
                        balance_key: BalanceKey {
                            cross_chain_user: recipient.recipient.clone(),
                            token_id: token.to_string(),
                        },
                    },
                );
                VoucherBalanceState {
                    recipient: recipient.recipient.clone(),
                    token: token.clone(),
                    amount: balance.amount,
                }
            })
        })
        .collect();

    let user_funds = user_funds_queries
        .iter()
        .map(|query| UserFundsState {
            chain_uid: query.chain_uid.clone(),
            user_addr: query.user_addr.clone(),
            denom: query.denom.clone(),
            amount: env.chain(&query.chain_id).query_balance(
                &cosmwasm_std::Addr::unchecked(query.user_addr.clone()),
                &query.denom,
            ),
        })
        .collect();

    let escrow_balances = escrow_tokens
        .iter()
        .map(|token| {
            let escrow_addr = get_escrow_addr(factory_app, factory_addr, token.as_str());
            let escrow_state: euclid::msgs::escrow::StateResponse =
                factory_app.query(&escrow_addr, &euclid::msgs::escrow::QueryMsg::State {});
            let factory_escrow_balance = escrow_state.total_amount;

            let router_state: euclid::msgs::router::StateResponse =
                router_app.query(router_addr, &euclid::msgs::router::QueryMsg::GetState {});
            let vb_escrows: euclid::msgs::virtual_balance::GetTokenEscrowsResponse = router_app
                .query(
                    &router_state.virtual_balance_address,
                    &euclid::msgs::virtual_balance::QueryMsg::GetTokenEscrows {
                        token_id: token.to_string(),
                        pagination: None,
                    },
                );
            let router_escrow_balance = vb_escrows
                .escrows
                .iter()
                .find(|entry| entry.chain_uid == escrow_chain_uid)
                .map_or(Uint256::zero(), |entry| entry.balance);

            EscrowBalanceState {
                chain_uid: escrow_chain_uid.clone(),
                token: token.clone(),
                factory_escrow_balance,
                router_escrow_balance,
            }
        })
        .collect();

    let vlp_balances = vlp_pairs
        .iter()
        .map(|pair| {
            let vlp_response: euclid::msgs::router::VlpResponse = router_app.query(
                router_addr,
                &euclid::msgs::router::QueryMsg::GetVlp { pair: pair.clone() },
            );
            let vlp_addr = cosmwasm_std::Addr::unchecked(vlp_response.vlp.clone());
            let liquidity: GetLiquidityQueryResponse =
                router_app.query(&vlp_addr, &euclid::msgs::vlp::cp::QueryMsg::Liquidity {});

            VlpBalanceState {
                pair: pair.clone(),
                vlp_address: vlp_response.vlp,
                token_1_reserve: liquidity.token_1_reserve,
                token_2_reserve: liquidity.token_2_reserve,
                total_lp_tokens: liquidity.total_lp_tokens,
            }
        })
        .collect();

    StateSync {
        voucher_balances,
        user_funds,
        escrow_balances,
        vlp_balances,
    }
}
