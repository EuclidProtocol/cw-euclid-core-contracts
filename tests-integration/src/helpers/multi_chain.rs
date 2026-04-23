#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;

use super::app::EuclidApp;

/// Replaces `MockInterchainEnv`. Holds one `EuclidApp` per chain (keyed by chain ID).
pub struct MultiChainEnv {
    chains: HashMap<String, EuclidApp>,
}

impl MultiChainEnv {
    /// `chains` is a list of `(chain_id, sender_name)` pairs.
    pub fn new(chains: Vec<(&str, &str)>) -> Self {
        let map = chains
            .into_iter()
            .map(|(chain_id, sender)| (chain_id.to_string(), EuclidApp::new(chain_id, sender)))
            .collect();
        Self { chains: map }
    }

    pub fn chain(&self, chain_id: &str) -> &EuclidApp {
        self.chains
            .get(chain_id)
            .unwrap_or_else(|| panic!("chain '{}' not found in MultiChainEnv", chain_id))
    }

    pub fn chain_mut(&mut self, chain_id: &str) -> &mut EuclidApp {
        self.chains
            .get_mut(chain_id)
            .unwrap_or_else(|| panic!("chain '{}' not found in MultiChainEnv", chain_id))
    }
}
