use cosmwasm_schema::cw_serde;

// Token asset that represents an identifier for a token
#[cw_serde]
pub struct Token {
    pub id: String,   
}

// A pair is a set of two tokens
#[cw_serde]
pub struct Pair {
    pub token_1: Token,
    pub token_2: Token,
}

// TokenInfo stores the native or smart contract token information from incoming chain
#[cw_serde]
pub enum TokenInfo {
    Native {
        denom: String,
    },
    Smart {
        contract_address: String,
    },
    
}

// PairInfo stores the pair information of two tokens
#[cw_serde]
pub struct PairInfo {
    pub token_1: TokenInfo,
    pub token_2: TokenInfo,
}