#[cfg(test)]
pub mod fixtures {
    use cosmwasm_std::testing::mock_dependencies;

    use crate::testing::helpers::{init, MockDeps};

    #[allow(dead_code)]
    pub fn initialized_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps
    }
}
