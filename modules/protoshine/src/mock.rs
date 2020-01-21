// One Mock Runtime
// - todo: _generate_ more
use super::*;

use frame_support::{assert_noop, assert_ok, impl_outer_origin, parameter_types, weights::Weight};
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};

impl_outer_origin! {
    pub enum Origin for Test where system = frame_system {}
}

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::one();
}
impl frame_system::Trait for Test {
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Call = ();
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = ();
    type BlockHashCount = BlockHashCount;
    type MaximumBlockWeight = MaximumBlockWeight;
    type AvailableBlockRatio = AvailableBlockRatio;
    type MaximumBlockLength = MaximumBlockLength;
    type Version = ();
    type ModuleToIndex = ();
}
parameter_types! {
    pub const ExistentialDeposit: u64 = 1;
    pub const TransferFee: u64 = 0;
    pub const CreationFee: u64 = 0;
}
impl pallet_balances::Trait for Test {
    type Balance = u64;
    type OnNewAccount = ();
    type OnFreeBalanceZero = ();
    type OnReapAccount = System;
    type Event = ();
    type TransferPayment = ();
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type TransferFee = TransferFee;
    type CreationFee = CreationFee;
}
parameter_types! {
    pub const MembershipProposalBond: u64 = 2;
    pub const MembershipSponsorBond: u32 = 3;
    pub const MembershipVoteBond: u32 = 1;
    pub const MaximumShareIssuance: Permill = Permill::from_percent(50);
    pub const MembershipConsensusThreshold: Permill = Permill::from_percent(67);
    pub const BatchPeriod: u64 = 2;
}
impl Trait for Test {
    type Currency = pallet_balances::Module<Test>;
    // not testing event emission in this runtime or using it?
    type Event = ();
    type MembershipProposalBond = MembershipProposalBond;
    type MembershipSponsorBond = MembershipSponsorBond;
    type MembershipVoteBond = MembershipVoteBond;
    type MaximumShareIssuance = MaximumShareIssuance;
    type MembershipConsensusThreshold = MembershipConsensusThreshold;
    type BatchPeriod = BatchPeriod;
}
pub type System = frame_system::Module<Test>;
pub type Balances = pallet_balances::Module<Test>;
pub type Protoshine = Module<Test>;

// useful for getting the bank_balance when you need to calculate the bank's collateralization ratio
// - see ../collateral
impl<AccountId> Owner<AccountId> {
    pub(crate) fn inner(self) -> Option<AccountId> {
        if let Owner::Owned(account) = self {
            Some(account)
        } else {
            None
        }
    }
}

use sp_runtime::traits::Saturating;
impl<T: Trait> Module<T> {
    /// Return the amount in the bank (in T::Currency denomination)
    pub fn bank_balance(bank: Bank<T::AccountId>) -> Result<BalanceOf<T>, Error<T>> {
        let account = bank.joint_account.inner().ok_or(Error::<T>::NoBankOwner)?;
        let balance = T::Currency::free_balance(&account)
            // TODO: ponder whether this should be here (not if I don't follow the same existential
            // deposit system as polkadot...)
            // Must never be less than 0 but better be safe.
            .saturating_sub(T::Currency::minimum_balance());
        Ok(balance)
    }

    /// Calculate the shares to capital ratio
    /// TODO: is this type conversion safe?
    /// ...I just want to use `Permill::from_rational_approximation` which requires inputs two of
    /// the same type
    pub fn shares_to_capital_ratio(shares: Shares, capital: BalanceOf<T>) -> Permill {
        let shares_as_balance = BalanceOf::<T>::from(shares);
        Permill::from_rational_approximation(shares_as_balance, capital)
    }

    /// Ratio of the `bank.balance` to `bank.shares`
    /// - this value may be interpreted as `currency_per_share` by UIs, but that would assume
    /// immediate liquidity which is false
    pub fn collateralization_ratio(bank: Bank<T::AccountId>) -> Result<Permill, Error<T>> {
        let most_recent_balance = Self::bank_balance(bank.clone())?;
        Ok(Self::shares_to_capital_ratio(
            bank.shares,
            most_recent_balance,
        ))
    }
}
