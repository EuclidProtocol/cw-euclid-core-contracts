use cosmwasm_schema::cw_serde;


// Token asset that represents an identifier for a token
#[cw_serde]
pub struct Token {
    pub id: String,   
}

impl Token { 
    pub fn exists(&self, pool: Pair) -> bool {
        if self == &pool.token_1 || self == &pool.token_2 {
            return true
        }
        else {
            return false
        }
    }
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

// Helper to Check if Token is Native or Smart
impl TokenInfo {
    pub fn is_native(&self) -> bool {
        match self {
            TokenInfo::Native { .. } => true,
            TokenInfo::Smart { .. } => false,
        }
    }

    pub fn is_smart(&self) -> bool {
        !self.is_native()
    }

    // Helper to get the denom of a native token
    pub fn get_denom(&self) -> String {
        match self {
            TokenInfo::Native { denom } => denom.to_string(),
            TokenInfo::Smart { .. } => panic!("This is not a native token"),
        }
    }

    // Helper to get the contract address of a smart token
    pub fn get_contract_address(&self) -> String {
        match self {
            TokenInfo::Smart { contract_address } => contract_address.to_string(),
            TokenInfo::Native { .. } => panic!("This is not a smart token"),
        }
    }

    // Check if asset exists in a certain pair
    pub fn exists(&self, pair_info: PairInfo) -> bool {
        if self == &pair_info.token_1 || self == &pair_info.token_2 {
            return true
        } else {
            return false
        }
    }
}

// PairInfo stores the pair information of two tokens
#[cw_serde]
pub struct PairInfo {
    pub token_1: TokenInfo,
    pub token_2: TokenInfo,
}