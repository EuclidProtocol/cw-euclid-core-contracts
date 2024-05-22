#[cfg(test)]
mod tests {

    use crate::contract::instantiate;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Uint128};
    use euclid::fee::Fee;
    use euclid::msgs::vlp::InstantiateMsg;
    use euclid::pool::Pool;
    use euclid::token::{Pair, PairInfo, Token, TokenInfo};

    #[test]
    // Write a test for instantiation
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: "router".to_string(),
            pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            fee: Fee {
                lp_fee: 1,
                treasury_fee: 1,
                staker_fee: 1,
            },
            pool: Pool {
                chain: "chain".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native {
                        denom: "token_1".to_string(),
                        token: Token {
                            id: "token_1".to_string(),
                        },
                    },

                    token_2: TokenInfo::Native {
                        denom: "token_2".to_string(),
                        token: Token {
                            id: "token_2".to_string(),
                        },
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }
}
