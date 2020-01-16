use codec::FullCodec;
use frame_support::traits::ReservableCurrency;
use sp_runtime::{
    traits::{MaybeSerializeDeserialize, SimpleArithmetic},
    Permill,
};
use sp_std::fmt::Debug;

/// Captures the minimal required behavior for the `Bank` abstraction with respect to share issuance
/// - WARNING: constraints must be satisfied around the logic that calls these methods in order for this to be safe because
/// no checks are done here
pub trait ShareBank {
	type Shares; 

    /// Issuance returns total shares
	fn issue(&mut self, amount: Self::Shares) -> Self::Shares;

    /// Burning shares (_buyback_) returns total shares
	fn buyback(&mut self, amount: Self::Shares) -> Self::Shares;
}

/// Wrapper around Permill for `EnsureShareWeight{AtLeast, MoreThan}`
pub trait Threshold {
    const THRESHOLD: Permill;
}

/// Requires 1/2x + 1 shares in favor for x shares that voted
pub struct _Majority; impl Threshold for _Majority { const THRESHOLD: Permill = Permill::from_percent(51); }
/// Requires 2/3x + 1 shares in favor for x shares that voted
pub struct _BFT_SuperMajority; impl Threshold for _BFT_SuperMajority { const THRESHOLD: Permill = Permill::from_percent(67); }
/// Requires all shares that voted to be in favor
pub struct _Unanimous; impl Threshold for _Unanimous { const THRESHOLD: Permill = Permill::from_percent(100); }

/// Signal is used by members to influence collective action. It can be used to
/// - sponsor proposals (from themselves or for outside applications)
/// - propose edits to proposals in screening
/// - vote on proposals
pub trait Signal<AccountId> {
    /// The equivalent of the `Balances` type
    /// - the `Into<u32>` is limiting and should be removed
    type Shares: SimpleArithmetic + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;
    /// Eventually, should be more easier to vote on what this can be as a non-exhaustive enum
    type Collateral: SimpleArithmetic + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;

    /// The total number of shares in circulation
    fn total_issuance() -> (Self::Shares, Self::Collateral);

    /// Increase issuance when membership approved
    /// - add a runtime hook for when membership is approved and place this logic therein
    /// - this fails if the value overflows
    fn issue_shares(amount: Self::Shares) -> bool;

    /// Decrease issuance when shares burned
    /// - add a runtime hook for when membership is approved and place this logic therein
    /// - this cannot fail, but it should be zero-bounded if it isn't already
    fn burn_shares(amount: Self::Shares);
    
    /// Dilute shares by spending (on grants presumably)
    fn spend_collateral(amount: Self::Collateral);
}
// could have collateral as an associated type but not for minimal version

// GovernanceShares
// sponsoring proposals, voting for proposals, etc
// - bidding on exit priority

// CollateralManagement
// - Collateral should be manageable
// - deciding what to accept for membership applications

// not safe and shouldn't be touched for now
trait FitchRatings<AccountId>: Signal<AccountId>
{
    fn rehypothecate_collateral(amount: Self::Collateral) -> bool;
}

// in the module, shares are used for
// - sponsoring proposals
// - voting on proposals
// - voting on rules, targets (meta)
// - weight in automatic preference aggregation (later)

// // TODO: change this to a trait for calculating vote weight with signal in the runtime
// pub trait InitializeMembers<AccountId> {
// 	/// Initialize the members to the given `members`.
// 	fn initialize_members(members: &[AccountId]);
// }

// impl<T> InitializeMembers<T> for () {
// 	fn initialize_members(_: &[T]) {}
// }
