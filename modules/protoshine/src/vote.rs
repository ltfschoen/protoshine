// Voting thresholds.

use codec::{Decode, Encode};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::traits::{IntegerSquareRoot, Zero};
use sp_std::ops::{Div, Mul, Rem};

use super::*;

/// A means of determining if a vote is past pass threshold.
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, sp_runtime::RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[non_exhaustive]
/// More thresholds can be added but this fucks with `Approved` a bit
pub enum VoteThreshold {
    /// A supermajority of approvals is needed to pass this vote.
    SuperMajorityApprove,
    /// A supermajority of rejects is needed to fail this vote.
    SuperMajorityAgainst,
    /// A simple majority of approvals is needed to pass this vote.
    SimpleMajority,
    // SimpleBFT
    // unanimous approval
    // 1 approving member
    // 2 approving members
} // TODO: simple bft, configurable thresholds, multisig requiremets instead like a simple `in_favor` minimum

pub trait Approved {
    /// Given `approve` votes for and `against` votes against from a total electorate size of
    /// `electorate` (`electorate - (approve + against)` are abstainers), then returns true if the
    /// overall outcome is in favor of approval.
    fn approved(&self) -> bool;
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// The state of each proposal's ongoing voting
/// - kept minimal to perform lazy computation to calculate if threshold requirements
/// are met at any time
pub struct MembershipVotingState {
    /// Total shares in favor
    pub in_favor: Shares,
    /// Total shares against
    pub against: Shares,
    /// All shares that can vote (TODO: add voter registration for more friction, configuration at least?)
    pub all_voters: Shares,
    /// Threshold for vote passage
    pub threshold: VoteThreshold,
}

impl Approved for MembershipVotingState {
    /// Given `approve` votes for and `against` votes against from a total electorate size of
    /// `electorate` of whom `voters` voted (`electorate - voters` are abstainers) then returns true if the
    /// overall outcome is in favor of approval.
    ///
    /// We assume each *voter* may cast more than one *vote*, hence `voters` is not necessarily equal to
    /// `approve + against`.
    fn approved(&self) -> bool {
        let total_voters = self.in_favor + self.against;
        let sqrt_voters = total_voters.integer_sqrt();
        let sqrt_electorate = self.all_voters.integer_sqrt();
        if sqrt_voters.is_zero() {
            return false;
        }
        match self.threshold {
            VoteThreshold::SuperMajorityApprove => {
                compare_rationals(self.against, sqrt_voters, self.in_favor, sqrt_electorate)
            }
            VoteThreshold::SuperMajorityAgainst => {
                compare_rationals(self.against, sqrt_electorate, self.in_favor, sqrt_voters)
            }
            VoteThreshold::SimpleMajority => self.in_favor > self.against,
        }
    }
}

/// Return `true` iff `n1 / d1 < n2 / d2`. `d1` and `d2` may not be zero.
fn compare_rationals<
    T: Zero + Mul<T, Output = T> + Div<T, Output = T> + Rem<T, Output = T> + Ord + Copy,
>(
    mut n1: T,
    mut d1: T,
    mut n2: T,
    mut d2: T,
) -> bool {
    // Uses a continued fractional representation for a non-overflowing compare.
    // Detailed at https://janmr.com/blog/2014/05/comparing-rational-numbers-without-overflow/.
    loop {
        let q1 = n1 / d1;
        let q2 = n2 / d2;
        if q1 < q2 {
            return true;
        }
        if q2 < q1 {
            return false;
        }
        let r1 = n1 % d1;
        let r2 = n2 % d2;
        if r2.is_zero() {
            return false;
        }
        if r1.is_zero() {
            return true;
        }
        n1 = d2;
        n2 = d1;
        d1 = r2;
        d2 = r1;
    }
}
