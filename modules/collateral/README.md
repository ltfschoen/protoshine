# collateral abstractions


## dynamic collateral requirements

* inspired by Balances paper and nudl's mathematica impl

Before recently, I was using the traits in this module to implement the following methods in the next module

```rust
/// The required application bond for membership
/// TODO: change logic herein to calculate bond based on ratio of `stake_promised` to
/// `shares_requested` relative to existing parameterization
fn calculate_member_application_bond(
    stake_promised: BalanceOf<T>,
    shares_requested: Shares,
) -> Result<BalanceOf<T>, Error<T>> {
    // get proposed membership ratio
    let ratio: Permill = Self::shares_to_capital_ratio(shares_requested, stake_promised);

    // call the bank
    let bank = Self::bank_account();
    // check the bank's ratio (TODO: allow banks to embed their criteria)
    let banks_ratio: Permill = Self::collateralization_ratio(bank)?;

    // compare ratio and multiply proposal bond (much room for improvement here)
    match (banks_ratio, ratio) {
        // minimum bond amount because improves share value if accepted
        (banks_ratio, ratio) if ratio < banks_ratio => Ok(T::MembershipProposalBond::get()),
        // standard bond amount because no changes to share value if accepted
        (banks_ratio, ratio) if ratio == banks_ratio => {
            Ok(T::MembershipProposalBond::get() * 2.into())
        }
        // dilutive proposal because decreases share value if accepted
        _ => Ok(T::MembershipProposalBond::get() * 4.into()),
    }
}

/// The required sponsorship bond for membership proposals
/// TODO: abstract method body into an outside method called in both of these methods
/// - make an issue
fn calculate_membership_sponsor_bond(
    stake_promised: BalanceOf<T>,
    shares_requested: Shares,
) -> Result<Shares, Error<T>> {
    // get proposed membership ratio
    let ratio: Permill = Self::shares_to_capital_ratio(shares_requested, stake_promised);

    // call the bank
    let bank = Self::bank_account();
    // check the bank's ratio (TODO: allow banks to embed their criteria)
    let banks_ratio: Permill = Self::collateralization_ratio(bank)?;

    // compare ratio and multiply proposal bond (much room for improvement here)
    match (banks_ratio, ratio) {
        // minimum bond amount because improves share value if accepted
        (banks_ratio, ratio) if ratio < banks_ratio => Ok(T::MembershipSponsorBond::get()),
        // standard bond amount because no changes to share value if accepted
        (banks_ratio, ratio) if ratio == banks_ratio => {
            Ok(T::MembershipSponsorBond::get() * 2u32)
        }
        // dilutive proposal because decreases share value if accepted
        _ => Ok(T::MembershipSponsorBond::get() * 4u32),
    }
}
```

The logic can be shared if we add another input

```rust
#[non_exhaustive]
pub enum BondType {
    Applicant(BalanceOf<T>),
    Sponsor(Shares),
    Vote(Shares),
}
```

And the output type implementation depends on this enum and maybe the type that it wraps? 



## old module impl

```rust
impl<T: Trait> ActionBond for Module<T> {
    type Shares = Shares;
    type Capital = BalanceOf<T>;

    fn conversion_comparison(shares: Shares, capital: BalanceOf<T>) -> ConversionRate {
        let shares_as_balance = BalanceOf::<T>::from(shares);
        match (shares_as_balance, capital) {
            (a, b) if a > b => {
                let permill_approximate =
                    Permill::from_rational_approximation(capital, shares_as_balance);
                ConversionRate::CapitalOverShare(permill_approximate)
            }
            (a, b) if a < b => {
                let permill_approximate =
                    Permill::from_rational_approximation(shares_as_balance, capital);
                ConversionRate::ShareOverCapital(permill_approximate)
            }
            _ => ConversionRate::Parity,
        }
    }
}
```