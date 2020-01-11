use frame_support::traits::ReservableCurrency;
use sp_runtime::traits::{MaybeSerializeDeserialize, SimpleArithmetic};
use sp_std::fmt::Debug;
use codec::FullCodec;


/// Signal is used by members to influence collective action. It can be used to
/// - sponsor proposals (from themselves or for outside applications)
/// - propose edits to proposals in screening
/// - vote on proposals
/// - 
pub trait Signal<AccountId> {
    /// The equivalent of the `Balances` type
    type Shares: SimpleArithmetic + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;
    
    /// The total number of shares in circulation
    fn total_issuance() -> Self::Shares;

    /// Increase issuance when membership approved
    /// - add a runtime hook for when membership is approved and place this logic therein
    /// - this fails if the value overflows
    fn issue(amount: Self::Shares) -> bool;

    /// Decrease issuance when shares burned
    /// - add a runtime hook for when membership is approved and place this logic therein
    /// - this cannot fail, but it should be zero-bounded if it isn't already
    fn burn(amount: Self::Shares);
}
// could have collateral as an associated type but not for minimal version

// in the module, shares are used for 
// - sponsoring proposals
// - voting on proposals
// - voting on rules, targets (meta)
// - weight in automatic preference aggregation (later)