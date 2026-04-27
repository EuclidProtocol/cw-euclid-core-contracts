#[cfg(test)]
pub mod test_fixtures {
    use crate::state::VALIDATORS;
    use crate::testing::helpers::{init, make_validator, test_chain_uid, MockDeps};
    use cosmwasm_std::testing::mock_dependencies;
    use rstest::fixture;

    /// A fresh `MockDeps` with the contract instantiated.
    #[fixture]
    pub fn initialized_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps
    }

    /// A `MockDeps` with the contract instantiated and one validator registered.
    #[fixture]
    pub fn deps_with_validator() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let (validator, _sk) =
            make_validator("2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369");
        let chain_uid = test_chain_uid();
        VALIDATORS
            .save(deps.as_mut().storage, chain_uid, &vec![validator])
            .unwrap();
        deps
    }
}
