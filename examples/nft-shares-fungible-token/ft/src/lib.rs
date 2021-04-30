use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    env, log, near_bindgen, AccountId, Balance, BorshStorageKey, PanicOnDefault, PromiseOrValue,
};
mod shares_metadata;
use shares_metadata::{SharesMetadata, SharesMetadataProvider, SHARES_FT_METADATA_SPEC};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    metadata: LazyOption<SharesMetadata>
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    FungibleToken,
    Metadata,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn create(nft_contract_address: AccountId, nft_token_id: String, owner_id: ValidAccountId, shares_count: U128, decimals: u8, share_price: U128) -> Self {
        Self::new(
            owner_id,
            shares_count,
            SharesMetadata {
                spec: SHARES_FT_METADATA_SPEC.to_string(),
                name: "Example NEAR fungible token".to_string(),
                symbol: "EXAMPLE".to_string(),
                icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                reference: None,
                reference_hash: None,
                decimals,

                // Shares FT specific metadata
                nft_contract_address,
                nft_token_id,
                share_price,
                released: false
            },
        )
        // TODO emit event
    }

    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.

    fn new(
        owner_id: ValidAccountId,
        total_supply: U128,
        metadata: shares_metadata::SharesMetadata,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
        };
        this.token.internal_register_account(owner_id.as_ref());
        this.token.internal_deposit(owner_id.as_ref(), total_supply.0);
        this
    }

    /// Exit price in Near to redeem underlying NFT
    pub fn exit_price(&self) -> U128 {
        (self.ft_total_supply().0 * self.ft_metadata().share_price.0).into()
    }

    /// Near tokens required by a user in addition to held shares to redeem NFT
    pub fn redeem_amount_of(&self, from: ValidAccountId) -> U128 {
        let SharesMetadata { released, share_price, .. } = self.ft_metadata();
        assert!(!released, "token already redeemed");

        let user_shares = self.ft_balance_of(from);

        (self.exit_price().0 - user_shares.0 * share_price.0).into()
    }

    /// Returns balance Near tokens in vault
    /// NFTs can be redeemed by paying Near. These tokens are the new backing for shares
    pub fn vault_balance(&self) -> U128 {
        let SharesMetadata { released, share_price, .. } = self.ft_metadata();
        let balance = if !released {
            0
        } else {
            self.ft_total_supply().0 * share_price.0
        };

        balance.into()
    }

    /// Once NFT is redeemed by paying exit price, remaining shareholders get a
    /// share of the deposited Near tokens in proportion of their owned shares
    pub fn vault_balance_of(&self, from: ValidAccountId) -> U128 {
        let SharesMetadata { released, share_price, .. } = self.ft_metadata();
        let balance = if !released {
            0
        } else {
            let user_shares = self.ft_balance_of(from);
            user_shares.0 * share_price.0
        };

        balance.into()
    }

    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }
}

near_contract_standards::impl_fungible_token_core!(Contract, token, on_tokens_burned);
near_contract_standards::impl_fungible_token_storage!(Contract, token, on_account_closed);

#[near_bindgen]
impl SharesMetadataProvider for Contract {
    fn ft_metadata(&self) -> SharesMetadata {
        self.metadata.get().unwrap()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance};

    use super::*;

    const TOTAL_SUPPLY: Balance = 1_000_000_000_000_000;
    const NFT_CONTRACT_ADDRESS: &'static str = "nft.near";
    const NFT_TOKEN_ID: &'static str = "0";
    const DECIMALS: u8 = 8;
    const SHARE_PRICE: u128 = 100000;

    fn get_context(predecessor_account_id: ValidAccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    fn test_new() {

        let mut context = get_context(accounts(1));
        testing_env!(context.build());

        let contract = Contract::create(
            NFT_CONTRACT_ADDRESS.into(), NFT_TOKEN_ID.into(), accounts(0), TOTAL_SUPPLY.into(), DECIMALS, SHARE_PRICE.into()
        );
        testing_env!(context.is_view(true).build());

        assert_eq!(contract.ft_total_supply().0, TOTAL_SUPPLY);
        assert_eq!(contract.ft_balance_of(accounts(0)).0, TOTAL_SUPPLY);

        let expected_exit_price = TOTAL_SUPPLY * SHARE_PRICE;
        assert_eq!(contract.exit_price().0, expected_exit_price);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_transfer() {
        let mut context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = Contract::create(
            NFT_CONTRACT_ADDRESS.into(), NFT_TOKEN_ID.into(), accounts(2), TOTAL_SUPPLY.into(), DECIMALS, SHARE_PRICE.into()
        );
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(contract.storage_balance_bounds().min.into())
            .predecessor_account_id(accounts(1))
            .build());
        // Paying for account registration, aka storage deposit
        contract.storage_deposit(None, None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        let transfer_amount = TOTAL_SUPPLY / 3;
        contract.ft_transfer(accounts(1), transfer_amount.into(), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert_eq!(contract.ft_balance_of(accounts(2)).0, (TOTAL_SUPPLY - transfer_amount));
        assert_eq!(contract.ft_balance_of(accounts(1)).0, transfer_amount);
    }
}
