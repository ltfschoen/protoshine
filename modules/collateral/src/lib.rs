// bond.rs
use codec::{Decode, Encode};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::SimpleArithmetic, Permill, RuntimeDebug};
use sp_std::prelude::*;
// use signal::ShareBank; // add functionality to `ShareBank`

/// Wrapper around Permill for `Share : Capital` ratio comparisons
/// - it is required because `from_ration_approximation(p, q)`
/// does not support instances in which `p > q`
/// - upon second thought, this is basically a type for negotiation over conversion
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ConversionRate {
    // stake_promised == shares_requested
    Parity,
    // stake_promised < shares_requested
    CapitalOverShare(Permill),
    // shares_requested < stake_promised
    ShareOverCapital(Permill),
    // TODO: add `MoneyOverBitches` variant for weezy protocol
}

// leave out for CI because not immediately used
// impl ConversionRate {
//     pub(crate) fn same_sign(&self, other: &ConversionRate) -> bool {
//         match (self, other) {
//             (&ConversionRate::Parity, &ConversionRate::Parity) => true,
//             (&ConversionRate::CapitalOverShare(_), &ConversionRate::CapitalOverShare(_)) => true,
//             (&ConversionRate::ShareOverCapital(_), &ConversionRate::ShareOverCapital(_)) => true,
//             _ => false,
//         }
//     }
//     pub(crate) fn inner(&self) -> Option<Permill> {
//         match self {
//             &ConversionRate::CapitalOverShare(amount) => Some(amount),
//             &ConversionRate::ShareOverCapital(amount) => Some(amount),
//             _ => None,
//         }
//     }
//     // because membership proposals that ask for more shares than capital might/should be automatically rejected (grants are a separate process)
//     pub(crate) fn is_capital_over_share(&self) -> bool {
//         match self {
//             &ConversionRate::CapitalOverShare(_) => true,
//             _ => false,
//         }
//     }
// }

/// Intended for implementation by the module? for now
///
/// tied to specific actions within the module's incentive system
/// - for 1 organization, many would make this a proper comparison on a trait object with <Shares, Capital>
pub trait ActionBond {
    type Shares: SimpleArithmetic;
    type Capital: SimpleArithmetic;
    fn conversion_comparison(shares: Self::Shares, capital: Self::Capital) -> ConversionRate;
}

// need a configuration for each of these that relates
// to the underlying meta state requirements
pub trait CalculateCollateralReq {
    type Collateral: SimpleArithmetic;
    fn calculate_collateral_req(
        new_parity: ConversionRate,
        existing_parity: ConversionRate,
    ) -> Self::Collateral;
}

// super far out
// UI and/or client is constantly collecting data to predict future bond rates and hedge
