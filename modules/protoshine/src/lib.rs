#![allow(clippy::string_lit_as_bytes)]
#![allow(clippy::redundant_closure_call)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod bank;
use bank::{Bank, Owner, ShareProfile, BANK_ID};
use signal::ShareBank;

mod vote;
use vote::{Approved, MembershipVotingState, Vote, VoteThreshold};

use codec::{Decode, Encode};
use frame_support::traits::{Currency, ExistenceRequirement, Get, ReservableCurrency};
use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure};
use frame_system::{self as system, ensure_signed};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::traits::AccountIdConversion; // Saturating
use sp_runtime::{DispatchResult, Permill, RuntimeDebug};
use sp_std::prelude::*;

// TODO: replace with hashes as per recent issue
type ProposalIndex = u32;
pub type Shares = u32;
pub type BalanceOf<T> =
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

pub trait Trait: frame_system::Trait {
    /// The staking balance.
    type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    /// Minimum amount of funds that should be placed in a deposit for
    /// making a membership proposal
    type MembershipProposalBond: Get<BalanceOf<Self>>;

    /// Minimum amount of shares that should be locked for sponsoring a membership proposal
    type MembershipSponsorBond: Get<Shares>;

    /// Uniform voting bond
    /// TODO: reuse dynamic collateral logic from the two bonds above
    /// when further along (see `calculate_bonds`)
    type MembershipVoteBond: Get<Shares>;

    //// Maximum percentage of existing shares that can be issued in a BatchPeriod
    type MaximumShareIssuance: Get<Permill>;

    /// Minimum threshold to pass membership proposals
    /// - TODO: should depend on turnout and use `signal::Threshold` to foster
    ///  modular runtime configuration
    type MembershipConsensusThreshold: Get<Permill>;

    /// Batched membership changes
    type BatchPeriod: Get<Self::BlockNumber>;
}

decl_event!(
    pub enum Event<T>
    where
        Balance = BalanceOf<T>,
        <T as frame_system::Trait>::BlockNumber,
    {
        MembershipApplicationProposed(ProposalIndex, Balance, Shares, BlockNumber),
    /// An application was sponsored by a member on-chain with some of 
    /// their `Shares` at least equal to the `sponsor_quota` (metaparameter).
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
        /// Every vote inputs a magnitude and this must be above the minimum vote bond
        /// (expressed in shares)
        VoteMagnitudeBelowMinimumVoteBond,
        /// The vote state was never sponsored correctly so its vote state was not initialized
        VoteStateUninitialized,
        /// New voting option is not handled by the big match statement
        NewVotingOptionNotHandled,
        /// Could split this into at least `SponsorRequestForNonApplication`
        /// and `VoteOnNonVotingProposal`
        RequestInWrongStage,
        /// No MembershipShares information
        NoMembershipShareInfo,
        /// There is no owner of the bank
        NoBankOwner,
        /// Paths that are unlikely
        /// - delete all of these and resolve paths before use
        UnlikelyPathToBeDealtWith,
        /// Enforcement of membership criteria standards
        /// i.e. requesting more shares than capital committed
        MembershipApplicationIsRidiculous,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Protoshine {
        // // DEPRECATED UNTIL #7 is implemented and then this will be useful for
        // // iterating over all proposals to purge old ones
        // MembershipApplicationQ get(fn membership_application_q): Vec<MembershipProposal<T::AccountId, BalanceOf<T>, T::BlockNumber>>;

        /// Applications for membership into the organization
        pub MembershipApplications get(fn membership_applications):
            map ProposalIndex => Option<MembershipProposal<T::AccountId, BalanceOf<T>, T::BlockNumber>>;
        /// Number of proposals that have been made.
        pub MembershipApplicationCount get(fn membership_application_count): ProposalIndex;
        /// Membership proposal voting state
        pub MembershipVoteStates get(fn membership_vote_states):
            map ProposalIndex => Option<MembershipVotingState>;
        /// Membership proposal indices that have been approved but not yet absorbed.
        pub MembershipApprovals get(fn membership_approvals): Vec<ProposalIndex>;

        /// Members should be replaced by group scaling logic
        Members get(fn members) build(|config: &GenesisConfig<T>| {
            config.member_buy_in.iter().map(|(who, _, _)| {
                who.clone()
            }).collect::<Vec<_>>()
        }): Vec<T::AccountId>;
        /// TODO: Should be changed to `bank_accounts` when we scale this logic for sunshine
        BankAccount get(fn bank_account) build(|config: &GenesisConfig<T>| {
            let owner = Owner::Owned(<Module<T>>::account_id());
            let mut bank = Bank::new(owner, 0u32);
            for _i in config.member_buy_in.iter() {
                // TODO: this logic needs to move to runtime context or its separately tracked and poorly designed
                bank.issue(10);
            }
            bank
        }): Bank<T::AccountId>;
        /// Share amounts maps to (shares_reserved, total_shares) s.t. shares_reserved are reserved for votes or sponsorships
        pub MembershipShares get(fn membership_shares) build(|config: &GenesisConfig<T>| {
            config.member_buy_in.iter().map(|(who, _, shares_requested)| {
                // TODO: could offer configurability wrt how many shares are granted initially
                // and how shares are frozen or taken away if the balance transfers are not made
                // (make an issue for above initialization user flow)
                let share_profile = ShareProfile {
                    reserved_shares: 0u32,
                    total_shares: *shares_requested,
                };
                (who.clone(), share_profile)
            }).collect::<Vec<_>>()
            // will have to type alias (Shares, Shares) to some struct instead of whatever this is
        }): map T::AccountId => Option<ShareProfile>;
        /// Double Map from ProposalIndex => AccountId => Maybe(Vote)
        VotesByMembers get(fn votes_by_members):
            double_map ProposalIndex, hasher(twox_64_concat) T::AccountId => Option<Vote>;
        // TODO: add recipients vector for scheduled payments in `vote`
    }
    add_extra_genesis {
        config(member_buy_in): Vec<(T::AccountId, BalanceOf<T>, Shares)>;

        build(|config: &GenesisConfig<T>| {
            // This is the minimum amount in the Bank Account
            let _ = T::Currency::make_free_balance_be(
                &<Module<T>>::account_id(),
                T::Currency::minimum_balance(),
            );

            for (new_member, promised_buy_in, _) in &config.member_buy_in {
                // cache the buy-in and have some in-module time limit before which this is paid
                // (could also pay some portion of it right now and some later, could be configurable)
                // (see #25)
                T::Currency::transfer(&new_member, &<Module<T>>::account_id(), *promised_buy_in, ExistenceRequirement::AllowDeath)
                    .expect("See issue #25 for initial configurations discussion which is ongoing");
            }
        });
    }
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
        fn membership_application(
            origin,
            stake_promised: BalanceOf<T>,
            shares_requested: Shares,
        ) -> DispatchResult {
            let applicant = ensure_signed(origin)?;
            // membership criteria (see #27)
            ensure!(
                stake_promised > T::Currency::minimum_balance(),
                Error::<T>::InvalidMembershipApplication,
            );

            // uniform bond until full functionality (see ../collateral for details on future impl)
            let collateral = T::MembershipProposalBond::get();
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

            Self::deposit_event(RawEvent::MembershipApplicationProposed(
                c, stake_promised, shares_requested, now)
            );
            Ok(())
        }

        /// Members escalate applications to be voted on
        /// - UI should make sure the member knows how many shares they are used
        /// to sponsor and the associated risk
        ///		- `max_share_bond` exists so that UI's estimate isn't too wrong and it fucks over sponsors
        ///		- any punishment if the sponsored proposal is rejected?
        /// - note that someone could sponsor their own application
        /// - (1), (2) and (3) should be reordered s.t. the first check panics the most often, thereby
        /// limiting computational costs in the event of panics
        fn sponsor_membership_application(origin, index: ProposalIndex) -> DispatchResult {
            let sponsor = ensure_signed(origin)?;
            ensure!(Self::is_member(&sponsor), Error::<T>::NotAMember);

            // (1)
            let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
            ensure!(wrapped_membership_proposal.is_some(), Error::<T>::IndexWithNoAssociatedMembershipProposal);
            let membership_proposal = wrapped_membership_proposal.expect("just checked above; qed");
            ensure!(membership_proposal.stage == ProposalStage::Application, Error::<T>::RequestInWrongStage);

            let sponsor_bond = T::MembershipSponsorBond::get();

            // (3) check if the sponsor has enough to afford the sponsor bond by using `ShareProfile`
            let sponsor_share_profile = <MembershipShares<T>>::get(&sponsor).expect("invariant i: all members must have some shares and therefore some item in the shares map");
            ensure!(sponsor_share_profile.can_reserve(sponsor_bond), Error::<T>::InsufficientMembershipSponsorCollateral);
            let new_reserved = sponsor_share_profile.reserved_shares + sponsor_bond;
            let new_share_profile = ShareProfile {
                reserved_shares: new_reserved,
                total_shares: sponsor_share_profile.total_shares,
            };
            <MembershipShares<T>>::insert(&sponsor, new_share_profile);

            /// Sponsorship is default treated like a vote in the amount of `sponsor_bond` (up for discussion, see #22)
            let sponsor_vote_in_favor = Vote::InFavor(sponsor_bond);
            <VotesByMembers<T>>::insert(index, &sponsor, sponsor_vote_in_favor);

            // instantiate a membership vote here
            let vote_state = MembershipVotingState {
                in_favor: sponsor_bond,
                against: 0u32,
                // TODO: GET THE SHARE COUNT AND PLACE HERE
                // - ADD NOTE ON VOTER REGISTRATION PROS/CONS AND WHAT IT HAS TO DO WITH SPONSOR BOND QUESTIONS
                all_voters: 1u32,
                // TODO: this should depend on the type of proposal (grant, membership, meta) `=>`
                // ...matters once we bring in `ColoredProposal`s
                threshold: VoteThreshold::SimpleMajority,
            };
            // initialize the membership vote
            <MembershipVoteStates>::insert(index, vote_state);
            /// Share reservation data is updated in `MembershipShares` map

            /// Adjust the membership proposal in `MemberApplication`s so it isn't purged
            let voting_membership_proposal = MembershipProposal {
                stage: ProposalStage::Voting,
                ..membership_proposal
            };
            <MembershipApplications<T>>::insert(
                membership_proposal.index, voting_membership_proposal
            );

            Self::deposit_event(
                RawEvent::MembershipApplicationSponsored(
                    index,
                    sponsor_bond,
                    membership_proposal.stake_promised,
                    membership_proposal.shares_requested
                )
            );
            Ok(())
        }

        /// Voting Method
        /// - add docs based on issues #17 and #18
        fn vote_on_membership(origin, index: ProposalIndex, vote: Vote) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            ensure!(Self::is_member(&voter), Error::<T>::NotAMember);

            let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
            ensure!(
                wrapped_membership_proposal.is_some(),
                Error::<T>::IndexWithNoAssociatedMembershipProposal
            );
            let membership_proposal = wrapped_membership_proposal.expect("just checked inner existence above; qed");
            ensure!(
                membership_proposal.stage == ProposalStage::Voting, Error::<T>::RequestInWrongStage
            );

            let direction = vote.is_in_favor();
            let magnitude = vote.inner();
            // the vote bond's is `T::MembershipVoteBond` but it reserves the magnitude of the vote (=> the minimum vote amount is `T::MembershipVoteBond`)
            ensure!(
                magnitude >= T::MembershipVoteBond::get(), Error::<T>::VoteMagnitudeBelowMinimumVoteBond
            );

            // Get Membership Voting State to verify valid transition before updating it
            let wrapped_vote_by_member = <VotesByMembers<T>>::get(index, &voter);

            // get member share profile { reserved_shares, total_shares }
            let voter_share_profile = <MembershipShares<T>>::get(&voter).ok_or(Error::<T>::NoMembershipShareInfo)?;

            let (mut new, mut same, mut different) = (false, false, false);
            let mut new_reserved = voter_share_profile.reserved_shares;
            let mut new_magnitude = magnitude;
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
                    // must aggregate here because we don't have access
                    // to shares outside of this match statement
                    new_magnitude += shares;
                },
                Some(Vote::Against(shares)) if !direction => {
                    same = true;
                    // must aggregate here because we don't have access to shares
                    // outside of this match statement
                    new_magnitude += shares;
                },
                // (3)vote exists and it's the opposite direction as the new vote
                // (Some(Vote::InFavor(shares)) if !direction || (
                // Some(Vote::Against(shares)) if direction)
                Some(Vote::InFavor(shares)) if !direction => {
                    different = true;
                    // increment negative shares imbalance
                    // (and decrease from <MembershipVote<T>>::get(account).in_favor)
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
                    // increment negative shares imbalance
                    // (and decrease from <MembershipVote<T>>::get(account).in_favor)
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
                // if a voting option is added, a branch must be added here to account for it
                Some(_) => return Err(Error::<T>::NewVotingOptionNotHandled.into()),
            }
            // get current vote state
            let current_vote_state = <MembershipVoteStates>::get(index).ok_or(Error::<T>::VoteStateUninitialized)?;
            let mut new_vote_state = current_vote_state;
            if new {
                // no existing votes for this proposal from this member
                new_reserved += magnitude;
                // check if the sponsor has enough free shares to afford the sponsor_bond
                ensure!(
                    voter_share_profile.total_shares >= new_reserved, Error::<T>::InsufficientMembershipVoteCollateral
                );
                if direction {
                    new_vote_state.in_favor += magnitude;
                } else {
                    new_vote_state.against += magnitude;
                }
                <VotesByMembers<T>>::insert(index, &voter, vote.clone());
                let new_share_profile = ShareProfile {
                    reserved_shares: new_reserved,
                    total_shares: voter_share_profile.total_shares,
                };
                <MembershipShares<T>>::insert(&voter, new_share_profile);
            }
            if same {
                // there is an existing vote in the same direction (so aggregate
                // new_magnitude and old_magnitude in match statement)
                new_reserved += magnitude;
                // check if the sponsor has enough free shares to afford the sponsor_bond
                ensure!(
                    voter_share_profile.total_shares >= new_reserved,
                    Error::<T>::InsufficientMembershipVoteCollateral
                );
                let new_vote = if direction {
                    new_vote_state.in_favor += magnitude;
                    Vote::InFavor(new_magnitude)
                } else {
                    new_vote_state.in_favor += magnitude;
                    Vote::Against(new_magnitude)
                };
                <VotesByMembers<T>>::insert(index, &voter, new_vote);
                let new_share_profile = ShareProfile {
                    reserved_shares: new_reserved,
                    total_shares: voter_share_profile.total_shares,
                };
                <MembershipShares<T>>::insert(&voter, new_share_profile);
            }
            if different {
                if shares_imbalance_sign {
                    // positive => increase in_favor by magnitude
                    new_vote_state.in_favor += magnitude;
                    new_vote_state.against -= shares_imbalance;
                } else {
                    // negative => decrease in_favor by shares_imbalance
                    new_vote_state.in_favor -= shares_imbalance; // this is wrong!
                    new_vote_state.against += magnitude;
                }
                if less_shares_reserved {
                    // less shares are reserved in new vote so subtract how much
                    // larger the last vote was relative to the new one
                    // to be deleted, debugging now: </new_vote_state.turnout += difference;>
                    new_reserved -= difference;
                } else {
                    // more shares are reserved in new vote so add how much
                    // larger the new vote is relative to the last one
                    //  to be deleted, debugging now: </new_vote_state.turnout += difference;>
                    new_reserved += difference;
                    ensure!(
                        voter_share_profile.total_shares >= new_reserved,
                        Error::<T>::InsufficientMembershipVoteCollateral
                    );
                }
                let new_share_profile = ShareProfile {
                    reserved_shares: new_reserved,
                    total_shares: voter_share_profile.total_shares,
                };
                <MembershipShares<T>>::insert(&voter, new_share_profile);
                <VotesByMembers<T>>::insert(index, &voter, vote);
            }

            if new_vote_state.approved() {
                // change proposal state to passed and schedule passage in storage via `on_finalize` calls
                let passed_proposal = MembershipProposal {
                    stage: ProposalStage::Passed,
                    ..membership_proposal
                };
                // change proposal to passed
                <MembershipApplications<T>>::insert(index, passed_proposal);
                // TODO: schedule execution
                // emit voted and passed events with scheduled execution estimate
            } else {
                // update vote state
                <MembershipVoteStates>::insert(index, new_vote_state);
                // emit voted event (TODO: change this to emit based on branches above to inform client
                // of changes to storage
            }
            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    /// Membership checking supporting a single member
    pub fn is_member(who: &T::AccountId) -> bool {
        <Members<T>>::get().contains(who)
    }

    // -- MAKE BELOW METHODS SPECIFIC TO SOME TRAIT
    // `impl BANKACCOUNT<T::ACCOUNTID> for Module<T>` --
    pub fn account_id() -> T::AccountId {
        //  TODO: in the multi-org version, the `ModuleId` is passed in and
        // this still returns `T::AccountId`
        BANK_ID.into_account()
    }
}
