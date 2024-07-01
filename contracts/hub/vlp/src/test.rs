#[cfg(test)]
mod tests {

    use crate::contract::instantiate;

    use cosmwasm_std::coins;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use euclid::fee::Fee;
    use euclid::msgs::vlp::InstantiateMsg;
    use euclid::token::{Pair, Token};

    #[test]
    // Write a test for instantiation
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: "router".to_string(),
            vcoin: "vcoin".to_string(),
            cw20_code_id: 1,
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
            execute: None,
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }
}
