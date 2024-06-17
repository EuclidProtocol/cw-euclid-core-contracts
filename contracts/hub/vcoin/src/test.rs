#[cfg(test)]
mod tests {

    use crate::contract::instantiate;

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Addr};
    use euclid::msgs::vcoin::InstantiateMsg;

    #[test]
    // Write a test for instantiation
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            admin: None,
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }
}
