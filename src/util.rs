
// define a share type which is similar to currency but which has the desired features
// - should be documentation on the undefined behavior of wrapping for Shares `=>` at some point, it should start storing a tuple
// (u64, u64) and continue extending this data structure for as large as the number gets
// - 
pub trait Power<AccountId> {
    /// The equivalent of the `Balances` type
    type Shares: SimpleArithmetic + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;

    /// The collateral backing this `Power`
    /// TODO: more generic type than `Lockable` here, but impl for `LockableCurrency` and `ReservableCurrency` or other asset types
    /// - in the future, I want this parameter to governable by some origin (like mcd)
    type Collateral: LockableCurrency<Self::AccountId>;

    /// The total number of shares controlled by `who`
    fn total_shares(who: &AccountId) -> Self::Shares;
    
    /// The total number of (`Shares`, Collateral) in circulation
    fn total_issuance() -> (Self::Shares, Self::Collateral);
}

/// another one
pub trait BurnablePower<AccountId>: Power<AccountId> {
    /// the order in which dilution occurs upon mass exit (from lowest to highest)
    /// - should make this 
    type DilutionTicket: SimpleArithmetic + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;

    fn request_burn(amount: Self::Shares) -> Self::DilutionTicket;
}

///
pub trait RehypothecatedPower<AccountId>: Power<AccountId> {
    // I want to be able to reserve it for an unlimited number of actions but check which actions it's reserved for each time

    // so I want to define a class of methods
}



// in the module, shares are used for 
// - sponsoring proposals
// - voting on proposals
// - voting on rules, targets (meta)
// - weight in automatic preference aggregation (later)