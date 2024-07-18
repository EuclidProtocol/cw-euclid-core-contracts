#[cfg(test)]
mod tests {

    use crate::contract::instantiate;

    use cosmwasm_std::coins;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use euclid::chain::{ChainUid, CrossChainUser};
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
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            fee: Fee {
                lp_fee_bps: 10,
                euclid_fee_bps: 10,
                recipient: CrossChainUser {
                    address: info.sender.to_string(),
                    chain_uid: ChainUid::vsl_chain_uid().unwrap(),
                },
            },
            admin: info.sender.to_string(),
            execute: None,
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }
}
