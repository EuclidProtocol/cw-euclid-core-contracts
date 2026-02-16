#![cfg(not(target_arch = "wasm32"))]

use crate::helpers::chains::{get_escrow, get_virtual_balance, get_vlp};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_orch::mock::MockBase;
use cw_orch::prelude::{CwOrchQuery, Environment};
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::escrow::QueryMsgFns as EscrowQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::base::GetLiquidityQueryResponse;
use euclid::recipient::Recipient;
use euclid::token::{Pair, Token};
use euclid::utils::pagination::Pagination;
use euclid::voucher::BalanceKey;
use factory::FactoryContract;
use router::RouterContract;

#[cw_serde]
pub struct VoucherBalanceState {
    pub recipient: CrossChainUser,
    pub token: Token,
    pub amount: Uint128,
}

#[cw_serde]
pub struct UserFundsState {
    pub chain_uid: ChainUid,
    pub user_addr: String,
    pub denom: String,
    pub amount: Uint128,
}

#[cw_serde]
pub struct EscrowBalanceState {
    pub chain_uid: ChainUid,
    pub token: Token,
    pub factory_escrow_balance: Uint128,
    pub router_escrow_balance: Uint128,
}

#[cw_serde]
pub struct VlpBalanceState {
    pub pair: Pair,
    pub vlp_address: String,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
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
    pub chain: MockBase,
    pub user_addr: String,
    pub denom: String,
}

impl StateSync {
    pub fn voucher_balance(&self, recipient: &CrossChainUser, token: &Token) -> Option<Uint128> {
        self.voucher_balances
            .iter()
            .find(|entry| &entry.recipient == recipient && &entry.token == token)
            .map(|entry| entry.amount)
    }

    pub fn user_funds(
        &self,
        chain_uid: &ChainUid,
        user_addr: &str,
        denom: &str,
    ) -> Option<Uint128> {
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
            .find(|entry| &entry.chain_uid == chain_uid && &entry.token == token)
    }

    // pub fn vlp_balance(&self, pair: &Pair) -> Option<&VlpBalanceState> {
    //     self.vlp_balances.iter().find(|entry| &entry.pair == pair)
    // }
}

pub(crate) fn sync_state(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    recipients: Vec<Recipient>,
    voucher_tokens: Vec<Token>,
    user_funds_queries: Vec<UserFundsQuery>,
    escrow_tokens: Vec<Token>,
    escrow_chain_uid: ChainUid,
    vlp_pairs: Vec<Pair>,
) -> StateSync {
    let virtual_balance_contract = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );

    let voucher_balances = recipients
        .iter()
        .filter(|recipient| recipient.denom.is_voucher())
        .flat_map(|recipient| {
            voucher_tokens.iter().map(|token| VoucherBalanceState {
                recipient: recipient.recipient.clone(),
                token: token.clone(),
                amount: virtual_balance_contract
                    .get_balance(BalanceKey {
                        cross_chain_user: recipient.recipient.clone(),
                        token_id: token.to_string(),
                    })
                    .unwrap()
                    .amount,
            })
        })
        .collect();

    let user_funds = user_funds_queries
        .iter()
        .map(|query| UserFundsState {
            chain_uid: query.chain_uid.clone(),
            user_addr: query.user_addr.clone(),
            denom: query.denom.clone(),
            amount: query
                .chain
                .query_balance(
                    &Addr::unchecked(query.user_addr.clone()),
                    query.denom.as_str(),
                )
                .unwrap(),
        })
        .collect();

    let escrow_balances = escrow_tokens
        .iter()
        .map(|token| {
            let factory_escrow_balance = get_escrow(factory, token.as_str())
                .state()
                .unwrap()
                .total_amount;
            let router_escrow_balance = router
                .query_token_escrows(
                    Pagination::new(Some(escrow_chain_uid.clone()), None, None, Some(1)),
                    token.clone(),
                )
                .unwrap()
                .chains
                .iter()
                .find(|chain| chain.chain_uid == escrow_chain_uid)
                .map(|chain| chain.balance)
                .unwrap_or(Uint128::zero());

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
            let vlp_response = router.get_vlp(pair.clone()).unwrap();
            let vlp_contract = get_vlp(
                router.environment(),
                &Addr::unchecked(vlp_response.vlp.clone()),
            );
            let liquidity: GetLiquidityQueryResponse = vlp_contract
                .query(&euclid::msgs::vlp::cp::QueryMsg::Liquidity {})
                .unwrap();

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
