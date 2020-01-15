use super::*;

#[cfg(feature = "std")]
use serde::{Serialize, Deserialize};
use sp_std::prelude::*;
use codec::{Encode, Decode};
use sp_runtime::{RuntimeDebug, ModuleId};

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// There's only one bank instance now but add a `group_id` field...
pub struct Bank<AccountId>  {
    /// The account_id represented by the bank
    /// TODO: multi-account management and/or rotating accounts (do keys already rotate?)
	account: Owner<AccountId>,
	/// Total number of shares backing for an org
	shares: Shares,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug)]
/// Owner of an bank
/// - eventually should add multiple `AccountId`s
pub enum Owner<AccountId> {
	/// No owner.
	None,
	/// Owned by an AccountId
    Address(AccountId),
}

/// Default module_id
pub const DEFAULT_ID: ModuleId = ModuleId(*b"protoshi");

// Default Bank, never use these parameters
impl<AccountId> Default for Bank<AccountId> {
	fn default() -> Self {
		Self {
			account: Owner::None,
			shares: 0,
		}
	}
}

// impl<BalanceOf> Bank<BalanceOf> {
// 	// can I return module errors here?
// 	fn fun_init(balance: BalanceOf, shares_to_issue: Shares) -> Result<Bank<BalanceOf>, Error> {
// 		// stake balance
// 		// generate account_id
//      // move balance into account
//      // issue shares and associate shares with this group's identifier
// 	}
// }