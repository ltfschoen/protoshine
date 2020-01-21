// bond.rs
use codec::{Decode, Encode};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{Permill, RuntimeDebug, traits::SimpleArithmetic};
use sp_std::prelude::*;
// use signal::ShareBank; // add functionality to `ShareBank`

/// Open to new names on this method!
///
/// Wrapper around Permill for `Share : Capital` ratio comparisons
/// - it is required because `from_ration_approximation(p, q)` 
/// does not support instances in which `p > q`
/// - upon second thought, this is basically a type for negotiation over conversion
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ShareParity {
    // stake_promised == shares_requested
    Equal,
    // stake_promised < shares_requested
    CapitalOverShare(Permill),
    // shares_requested < stake_promised
    ShareOverCapital(Permill),
    // TODO: add `MoneyOverBitches` variant for weezy protocol
}

// impl ShareParity {
//     pub(crate) fn same_variant(&self, other: &ShareParity) -> bool {
//         match (self, other) {
//             (&ShareParity::Equal, &ShareParity::Equal) => true,
//             (&ShareParity::CapitalOverShare(_), &ShareParity::CapitalOverShare(_)) => true,
//             (&ShareParity::ShareOverCapital(_), &ShareParity::ShareOverCapital(_)) => true,
//             _ => false,
//         }
//     }
// }

/// Intended for implementation by the module
pub trait BondHelper {
    type Shares: SimpleArithmetic;
    type Capital: SimpleArithmetic;
    fn share_parity_calculator(shares: Self::Shares, capital: Self::Capital) -> ShareParity;
}

// design a trait for this module's runtime to implement for collateral-oriented calculations