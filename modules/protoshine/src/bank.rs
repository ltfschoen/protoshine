use super::*;

use codec::{Decode, Encode};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use signal::ShareBank;
use sp_runtime::{ModuleId, RuntimeDebug};
use sp_std::prelude::*;

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// Profile for existing share obligations
/// - this prevents members from signalling with shares already served for ongoing sponsorships/votes
pub struct ShareProfile {
    pub(crate) reserved_shares: Shares,
    pub(crate) total_shares: Shares,
}

impl ShareProfile {
    pub(crate) fn can_reserve(&self, amount: Shares) -> bool {
        amount >= self.total_shares - self.reserved_shares
    }
}

/// Single bank owner (for now)
pub const BANK_ID: ModuleId = ModuleId(*b"protoshi");

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug)]
/// Owner of an bank
/// - eventually should add multiple `AccountId`s
pub enum Owner<AccountId> {
    /// No owner.
    None,
    /// Owned by an AccountId
    Owned(AccountId),
}

impl<AccountId> Owner<AccountId> {
    pub(crate) fn inner(self) -> Option<AccountId> {
        if let Owner::Owned(account) = self {
            Some(account)
        } else {
            None
        }
    }
}

/// Bank Object
/// relevant when
/// - shares are issued (for membership)
/// - shares are burned (for membership exits, taxes)
/// - spends are executed (for membership exits, grants)
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// There's only one bank instance now but add a `group_id` field...
pub struct Bank<AccountId> {
    /// The account_id represented by the bank
    /// TODO: multi-account management and/or rotating accounts (do keys already rotate?)
    pub joint_account: Owner<AccountId>,
    /// Total number of shares backing for an org
    pub shares: Shares,
}

// Default Bank, never use these parameters, just here to store as storage value
impl<AccountId> Default for Bank<AccountId> {
    fn default() -> Self {
        Self {
            joint_account: Owner::None,
            shares: 0,
        }
    }
}

/// WARNING: these methods only work if we restrict where this can be called in
/// the runtime to places that satisfy constraints such as
/// - `new` cannot be called unless the Owner::Address(AccountId) stakes some
/// minimum amount of capital; this is a runtime method's constraints
impl<AccountId> Bank<AccountId> {
    pub fn new(owner: Owner<AccountId>, initial_shares: Shares) -> Bank<AccountId> {
        Self {
            joint_account: owner,
            shares: initial_shares,
        }
    }
}

/// All access to shares goes through these commands
/// => all calls must be from authorized callers with prerequisite conditions satisfied
/// ( these methods not be callable from _anywhere_ )
impl<AccountId> ShareBank for Bank<AccountId> {
    /// TODO: build Shares type like other generic asset impls?
    type Shares = u32;

    fn issue(&mut self, amount: Self::Shares) -> Self::Shares {
        self.shares += amount;
        self.shares
    }

    fn buyback(&mut self, amount: Self::Shares) -> Self::Shares {
        self.shares -= amount;
        self.shares
    }
}
