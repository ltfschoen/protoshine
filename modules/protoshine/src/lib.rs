#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod bank;
use bank::{Bank, BANK_ID};

use codec::{Decode, Encode};
use frame_support::traits::{Currency, Get, ReservableCurrency};
use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure};
use frame_system::{self as system, ensure_signed};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::traits::{AccountIdConversion, Saturating};
use sp_runtime::{DispatchResult, Permill, RuntimeDebug};
use sp_std::prelude::*;

type ProposalIndex = u32;
type Shares = u32;
type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
#[non_exhaustive]
/// Proposal stages
pub enum ProposalStage {
    /// Applied but not sponsored
    Application,
    /// Sponsored and open to voting by members
    Voting,
    /// Passed but not executed
    Passed,
    /// Already executed
    Law,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// Proposal for membership changes to the LLC
pub struct MembershipProposal<AccountId, BalanceOf, BlockNumber> {
    /// Unique proposal index
    index: ProposalIndex,
    /// The applicant
    who: AccountId,
    /// The collateral promised and slowly staked over the duration of the proposal process
    /// TODO: make issue for why this should be made more complex eventually s.t. amount staked
    /// only is applied once approved and reserved changes based on prob(passage)
    stake_promised: BalanceOf,
    /// The reward that the bidder has requested for successfully joining the society.
    shares_requested: Shares,
    /// Current stage of the proposal
    stage: ProposalStage,
    /// if `ApplicationTimeLimit` is exceeded past this time_proposed, the application is removed
    /// - TODO: use #7 to get rid of this or wrap it in `ProposalStage::Application` if possible
    time_proposed: BlockNumber,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
/// The state of each proposal's ongoing voting
/// - kept minimal to perform lazy computation to calculate if threshold requirements are met at any time
pub struct MinimalMembershipVoteState {
    /// total shares in favor
    in_favor: Shares,
    /// total turnout
    turnout: Shares,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
#[non_exhaustive]
/// Votes submitted by voting members
/// - could add more explicit commands here like `change_vote` to remove ambiguity of #18
pub enum Vote {
    InFavor(Shares),
    Against(Shares),
}

// not sure if this is necessary or if I can check the form another way
impl Vote {
    fn is_in_favor(&self) -> bool {
        match self {
            Vote::InFavor(shares) => true,
            _ => false,
        }
    }
    fn inner(&self) -> Shares {
        match self {
            Vote::InFavor(shares) => *shares,
            Vote::Against(shares) => *shares,
        }
    }
}

pub trait Trait: frame_system::Trait {
    /// The staking balance.
    type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    /// Minimum amount of funds that should be placed in a deposit for making a membership proposal
    type MembershipProposalBond: Get<BalanceOf<Self>>;

    /// Minimum amount of shares that should be locked for sponsoring a membership proposal
    type MembershipSponsorBond: Get<Shares>;

    /// Uniform voting bond
    /// TODO: reuse dynamic collateral logic from the two bonds above when further along (see `calculate_bonds`)
    type MembershipVoteBond: Get<Shares>;

    //// Maximum percentage of existing shares that can be issued in a BatchPeriod
    type MaximumShareIssuance: Get<Permill>;

    /// Minimum threshold to pass membership proposals
    /// - TODO: should depend on turnout and use `signal::Threshold` to foster modular runtime configuration
    type MembershipConsensusThreshold: Get<Permill>;

    /// Batched membership changes
    type BatchPeriod: Get<Self::BlockNumber>;
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;
        fn deposit_event() = default;

        /// Minimum proposal bond
        const MembershipProposalBond: BalanceOf<T> = T::MembershipProposalBond::get();

        /// Minimum sponsor bond
        const MembershipSponsorBond: Shares = T::MembershipSponsorBond::get();

        /// Uniform voting bond
        const MembershipVoteBond: Shares = T::MembershipVoteBond::get();

        /// Maximum number of shares issued in a batch period
        const MaximumShareIssuance: Permill = T::MaximumShareIssuance::get();

        /// Threshold requirement for membership consensus decisions (uniform for now)
        const MembershipConsensusThreshold: Permill = T::MembershipConsensusThreshold::get();

        /// Period between successive batched membership changes
        const BatchPeriod: T::BlockNumber = T::BatchPeriod::get();

        /// Anyone can apply to exchange shares for capital
        /// - any punishment if the application fails and does this depend on how it fails?
        /// -
        fn membership_application(
            origin,
            stake_promised: BalanceOf<T>,
            shares_requested: Shares,
        ) -> DispatchResult {
            let applicant = ensure_signed(origin)?;
            // these are the requirements for MEMBERSHIP applications (grant applications are
            // different, unlike in moloch)
            let shares_as_balance = BalanceOf::<T>::from(shares_requested);
            ensure!(
                stake_promised > T::Currency::minimum_balance() &&
                stake_promised > shares_as_balance,
                Error::<T>::InvalidMembershipApplication,
            );

            let collateral = Self::calculate_member_application_bond(
                stake_promised,
                shares_requested,
            )?;
            T::Currency::reserve(&applicant, collateral)
                .map_err(|_| Error::<T>::InsufficientMembershipApplicantCollateral)?;
            let c = Self::membership_application_count() + 1;
            MembershipApplicationCount::put(c);
            let now = <system::Module<T>>::block_number();
            let membership_app = MembershipProposal {
                index: c,
                who: applicant,
                stake_promised,
                shares_requested,
                stage: ProposalStage::Application,
                time_proposed: now,
            };
            // Deprecated until #7 is pursued
            // <MembershipApplicationQ<T>>::mutate(|v| v.push(membership_app.clone()));
            <MembershipApplications<T>>::insert(c, membership_app);

            Self::deposit_event(RawEvent::MembershipApplicationProposed(c, stake_promised, shares_requested, now));
            Ok(())
        }

        /// Members escalate applications to be voted on
        /// - UI should make sure the member knows how many shares they are used to sponsor and the associated risk
        ///		- `max_share_bond` exists so that UI's estimate isn't too wrong and it fucks over sponsors
        ///		- any punishment if the sponsored proposal is rejected?
        /// - note that someone could sponsor their own application
        /// - (1), (2) and (3) should be reordered s.t. the first check panics the most often, thereby limiting computational costs in the event of panics
        fn sponsor_membership_application(origin, index: ProposalIndex) -> DispatchResult {
            let sponsor = ensure_signed(origin)?;
            ensure!(Self::is_member(&sponsor), Error::<T>::NotAMember);

            // (1)
            let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
            ensure!(wrapped_membership_proposal.is_some(), Error::<T>::IndexWithNoAssociatedMembershipProposal);
            let membership_proposal = wrapped_membership_proposal.expect("just checked above; qed");
            ensure!(membership_proposal.stage == ProposalStage::Application, Error::<T>::RequestInWrongStage);

            // (2) should be calculated by UI ahead of time and calculated, but this structure fosters dynamic collateral pricing
            let sponsor_bond = Self::calculate_membership_sponsor_bond(membership_proposal.stake_promised.clone(), membership_proposal.shares_requested.clone())?;

            // (3) check if the sponsor has enough to afford the sponsor_bond
            let (reserved_shares, total_shares) = <MembershipShares<T>>::get(&sponsor).expect("invariant i: all members must have some shares and therefore some item in the shares map");
            // TODO: add overflow check here and resolution
            let new_reserved = reserved_shares + sponsor_bond;
            // check if the sponsor has enough free shares to afford the sponsor_bond
            ensure!(total_shares >= new_reserved, Error::<T>::InsufficientMembershipSponsorCollateral);

            /// Sponsorship is default treated like a vote in the amount of `sponsor_bond` (up for discussion)
            let sponsor_vote_in_favor = Vote::InFavor(sponsor_bond);
            <VotesByMembers<T>>::insert(index, &sponsor, sponsor_vote_in_favor);
            /// Share reservation data is updated in `MembershipShares` map
            <MembershipShares<T>>::insert(&sponsor, (new_reserved, total_shares));

            /// Adjust the membership proposal in `MemberApplication`s so it isn't purged
            let voting_membership_proposal = MembershipProposal {
                stage: ProposalStage::Voting,
                ..membership_proposal
            };
            <MembershipApplications<T>>::insert(membership_proposal.index, voting_membership_proposal);

            // notably, sponsorship is separate from voting (this is a choice we make on behalf of users and can adjust based on user research, which is more intuitive)

            Self::deposit_event(RawEvent::MembershipApplicationSponsored(index, sponsor_bond, membership_proposal.stake_promised, membership_proposal.shares_requested));
            Ok(())
        }

        /// Voting Method
        /// - add docs based on issues #17 and #18
        fn vote_on_membership(origin, index: ProposalIndex, vote: Vote) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            ensure!(Self::is_member(&voter), Error::<T>::NotAMember);

            let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
            ensure!(wrapped_membership_proposal.is_some(), Error::<T>::IndexWithNoAssociatedMembershipProposal);
            let membership_proposal = wrapped_membership_proposal.expect("just checked inner existence above; qed");
            ensure!(membership_proposal.stage == ProposalStage::Voting, Error::<T>::RequestInWrongStage);

            let direction = vote.is_in_favor();
            let magnitude = vote.inner();
            // the vote bond's is `T::MembershipVoteBond` but it reserves the magnitude of the vote (=> the minimum vote amount is `T::MembershipVoteBond`)
            ensure!(magnitude >= T::MembershipVoteBond::get(), Error::<T>::VoteMagnitudeBelowMinimumVoteBond);

            // get the `reserved_shares` so it can be updated based on the path that follows
            let (reserved_shares, total_shares) = <MembershipShares<T>>::get(&voter).expect("invariant i: all members must have some shares and therefore some item in the shares map");
            // Update Membership Voting State (this code should be refactored)
            let wrapped_vote_by_member = <VotesByMembers<T>>::get(index, &voter);

            // check if member can afford vote
            let (reserved_shares, total_shares) = <MembershipShares<T>>::get(&voter).expect("invariant i: all members must have some shares and therefore some item in the shares map");
            // should have been initialized upon sponsorship
            let old_vote_state = <MembershipVoteStates>::get(index).ok_or(Error::<T>::VoteStateUninitialized)?;

            let (mut new, mut same, mut different) = (false, false, false);
            let mut new_reserved = reserved_shares;
            let mut new_magnitude = magnitude;
            let mut new_voting_option: bool = false;
            // all of these variable initializations are designed to be overshadowed
            let mut shares_imbalance_sign: bool = false;
            let mut shares_imbalance: Shares = 0;
            let mut less_shares_reserved: bool = false;
            let mut difference: Shares = 0;
            match wrapped_vote_by_member {
                // (1) there does not already exist a vote by this member on this proposal
                None => new = true,
                // (2) vote exists and it's the same direction as the new vote
                Some(Vote::InFavor(shares)) if direction => {
                    same = true;
                    // must aggregate here because we don't have access to shares outside of this match statement
                    new_magnitude += shares;
                },
                Some(Vote::Against(shares)) if !direction => {
                    same = true;
                    // must aggregate here because we don't have access to shares outside of this match statement
                    new_magnitude += shares;
                },
                // (3)vote exists and it's the opposite direction as the new vote
                /// (Some(Vote::InFavor(shares)) if !direction || (2) Some(Vote::Against(shares)) if direction)
                Some(Vote::InFavor(shares)) if !direction => {
                    different = true;
                    // increment negative shares imbalance (and decrease from <MembershipVote<T>>::get(account).in_favor)
                    // false for negative sign (negative share imbalance)
                    shares_imbalance_sign = false;
                    // add the shares amount to the imbalance
                    shares_imbalance = shares;

                    less_shares_reserved = new_magnitude <= shares;
                    if less_shares_reserved {
                        difference = shares - new_magnitude;
                    } else {
                        difference = new_magnitude - shares;
                    }
                    // change total_turnout based on difference
                },
                Some(Vote::Against(shares)) if direction => {
                    different = true;
                    // increment negative shares imbalance (and decrease from <MembershipVote<T>>::get(account).in_favor)
                    // true for positive sign (positive share imbalance)
                    shares_imbalance_sign = true;
                    // add the shares amount to the imbalance
                    shares_imbalance = shares;

                    less_shares_reserved = new_magnitude <= shares;
                    if less_shares_reserved {
                        difference = shares - new_magnitude;
                    } else {
                        difference = new_magnitude - shares;
                    }
                    // change total_turnout based on difference
                },
                // not using this anywhere, but required for `non_exhaustive` on `Vot`
                Some(_) => new_voting_option = true,
            }
            let mut new_vote_state = old_vote_state;
            if new {
                // no existing votes for this proposal from this member
                new_reserved += magnitude;
                if direction {
                    new_vote_state.in_favor += magnitude;
                    new_vote_state.turnout += magnitude;
                } else {
                    new_vote_state.turnout += magnitude;
                } // TODO: update storgae item!
                // check if the sponsor has enough free shares to afford the sponsor_bond
                ensure!(total_shares >= new_reserved, Error::<T>::InsufficientMembershipVoteCollateral);
                <VotesByMembers<T>>::insert(index, &voter, vote.clone());
                <MembershipShares<T>>::insert(&voter, (new_reserved, total_shares));
            }
            if same {
                // there is an existing vote in the same direction (so aggregate new_magnitude and old_magnitude in match statement)
                new_reserved += magnitude;
                let new_vote: Vote;
                if direction {
                    new_vote_state.in_favor += magnitude;
                    new_vote_state.turnout += magnitude;
                    new_vote = Vote::InFavor(new_magnitude);
                } else {
                    new_vote_state.turnout += magnitude;
                    new_vote = Vote::Against(new_magnitude);
                } // TODO: update storage items!
                // check if the sponsor has enough free shares to afford the sponsor_bond
                ensure!(total_shares >= new_reserved, Error::<T>::InsufficientMembershipVoteCollateral);
                <VotesByMembers<T>>::insert(index, &voter, new_vote);
                <MembershipShares<T>>::insert(&voter, (new_reserved, total_shares));
            }
            if different {
                if shares_imbalance_sign {
                    // positive => increase in_favor by magnitude
                    new_vote_state.in_favor += magnitude;
                } else {
                    // negative => decrease in_favor by shares_imbalance
                    new_vote_state.in_favor -= shares_imbalance;
                }
                if less_shares_reserved {
                    // less shares are reserved in new vote so subtract how much larger the last vote was relative to the new one
                    new_vote_state.turnout -= difference;
                    new_reserved -= difference;
                } else {
                    // more shares are reserved in new vote so add how much larger the new vote is relative to the last one
                    new_vote_state.turnout += difference;
                    new_reserved += difference;
                    ensure!(total_shares >= new_reserved, Error::<T>::InsufficientMembershipVoteCollateral);
                }
                <MembershipShares<T>>::insert(&voter, (new_reserved, total_shares));
                <VotesByMembers<T>>::insert(index, &voter, vote);
            }

            // check if vote_state surpasses some threshold
            // - implemented by threshold or something?

            // update vote state
            <MembershipVoteStates>::insert(index, new_vote_state);
            // emit voted event

            Ok(())
        }
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Protoshine {
        /// DEPRECATED UNTIL #7 is implemented and then this will be useful for iterating over all proposals to purge old ones
        // MembershipApplicationQ get(fn membership_application_q): Vec<MembershipProposal<T::AccountId, BalanceOf<T>, T::BlockNumber>>;

        /// Applications for membership into the organization
        MembershipApplications get(fn membership_applications): map ProposalIndex => Option<MembershipProposal<T::AccountId, BalanceOf<T>, T::BlockNumber>>;
        /// Number of proposals that have been made.
        MembershipApplicationCount get(fn membership_application_count): ProposalIndex;
        /// Membership proposal voting state
        MembershipVoteStates get(fn membership_vote_states): map ProposalIndex => Option<MinimalMembershipVoteState>;
        /// Membership proposal indices that have been approved but not yet absorbed.
        MembershipApprovals get(fn membership_approvals): Vec<ProposalIndex>;

        /// Members should be replaced by group scaling logic
        Members get(fn members): Vec<T::AccountId>;
        /// TODO: Should be changed to `bank_accounts` when we scale this logic for sunshine
        BankAccount get(fn bank_account): Bank<T::AccountId>;
        /// Share amounts maps to (shares_reserved, total_shares) s.t. shares_reserved are reserved for votes or sponsorships
        MembershipShares get(fn membership_shares): map T::AccountId => Option<(Shares, Shares)>;
        /// Double Map from ProposalIndex => AccountId => Maybe(Vote)
        VotesByMembers get(fn votes_by_members): double_map ProposalIndex, hasher(twox_64_concat) T::AccountId => Option<Vote>;
    }
    add_extra_genesis {
        build(|_config| {
            // Create Single Org Account
            let _ = T::Currency::make_free_balance_be(
                &<Module<T>>::account_id(),
                T::Currency::minimum_balance(),
            );
        });
    }
}

decl_event!(
    pub enum Event<T>
    where
        Balance = BalanceOf<T>,
        <T as frame_system::Trait>::BlockNumber,
    {
        MembershipApplicationProposed(ProposalIndex, Balance, Shares, BlockNumber),
	/// An application was sponsored by a member on-chain with some of their `Shares` at least equal
	/// to the `sponsor_quota` (metaparameter).
	/// (index of proposal, sponsor quota for sponsorship, stake promised, shares requested)
        MembershipApplicationSponsored(ProposalIndex, Shares, Balance, Shares),
    }
);

decl_error! {
    /// Metadata for cleanly handling error paths
    /// TODO: pass in metadata into variants if possible (and it is!)
    pub enum Error for Module<T: Trait> {
        /// Not a member of the collective for which the runtime method is permissioned
        NotAMember,
        /// Poorly formed membership application because stake_promised <= shares_requested or
        /// stake_promised == 0
        InvalidMembershipApplication,
        /// Applicant can't cover collateral requirement for membership application
        InsufficientMembershipApplicantCollateral,
        /// Index doesn't haven associated membership proposal
        IndexWithNoAssociatedMembershipProposal,
        /// Required sponsorship bond exceeds upper bound inputted by user
        SponsorBondExceedsExpectations,
        /// Sponsor doesn't have enough shares to sponsor membership app
        InsufficientMembershipSponsorCollateral,
        /// Sponsor doesn't have enough shares to vote on membership app
        InsufficientMembershipVoteCollateral,
        /// Every vote inputs a magnitude and this must be above the minimum vote bond (it is expressed in shares)
        VoteMagnitudeBelowMinimumVoteBond,
        /// The vote state was never sponsored correctly so its vote state was not initialized
        VoteStateUninitialized,
        /// Could split this into at least `SponsorRequestForNonApplication` and `VoteOnNonVotingProposal`
        RequestInWrongStage,
        /// There is no owner of the bank
        NoBankOwner,
    }
}

impl<T: Trait> Module<T> {
    /// Membership checking supporting a single member
    pub fn is_member(who: &T::AccountId) -> bool {
        <Members<T>>::get().contains(who)
    }

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
            (banks_ratio, ratio) if ratio > banks_ratio => Ok(T::MembershipProposalBond::get()),
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
            (banks_ratio, ratio) if ratio > banks_ratio => Ok(T::MembershipSponsorBond::get()),
            // standard bond amount because no changes to share value if accepted
            (banks_ratio, ratio) if ratio == banks_ratio => {
                Ok(T::MembershipSponsorBond::get() * 2u32)
            }
            // dilutive proposal because decreases share value if accepted
            _ => Ok(T::MembershipSponsorBond::get() * 4u32),
        }
    }

    // -- MAKE BELOW METHODS SPECIFIC TO SOME TRAIT `impl BANKACCOUNT<T::ACCOUNTID> for Module<T>` --
    pub fn account_id() -> T::AccountId {
        //  TODO: in the multi-org version, the `ModuleId` is passed in and this still returns `T::AccountId`
        BANK_ID.into_account()
    }

    /// Return the amount in the bank (in T::Currency denomination)
    fn bank_balance(bank: Bank<T::AccountId>) -> Result<BalanceOf<T>, Error<T>> {
        let account = bank.account.inner().ok_or(Error::<T>::NoBankOwner)?;
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
    fn collateralization_ratio(bank: Bank<T::AccountId>) -> Result<Permill, Error<T>> {
        let most_recent_balance = Self::bank_balance(bank.clone())?;
        Ok(Self::shares_to_capital_ratio(
            bank.shares,
            most_recent_balance,
        ))
    }
}
