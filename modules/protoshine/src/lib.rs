#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod bank;
use bank::{Bank, BANK_ID};

#[cfg(feature = "std")]
use serde::{Serialize, Deserialize};
use sp_std::prelude::*;
use frame_support::{decl_module, decl_storage, decl_event, ensure, print, decl_error};
use frame_support::traits::{
	Currency, ExistenceRequirement, Get, Imbalance, OnUnbalanced,
	ReservableCurrency, WithdrawReason
};
use sp_runtime::{Permill, RuntimeDebug, ModuleId, DispatchResult};
use sp_runtime::traits::{Zero, EnsureOrigin, StaticLookup, AccountIdConversion, Saturating};
use frame_support::weights::SimpleDispatchInfo;
use codec::{Encode, Decode};
use frame_system::{self as system, ensure_signed};

type ProposalIndex = u32;
type Shares = u32;
type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

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
    /// TODO: make issue for why this should be made more complex eventually s.t. amount staked only is applied once approved and reserved changes based on prob(passage)
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
pub struct MembershipVote<AccountId> {
	/// (Total number of voting members, total shares)
	total_turnout: (u32, Shares),
	/// Yes votes in favor of the proposal
	yes: Vec<(AccountId, Shares)>,
	/// No votes against the proposal
	no: Vec<(AccountId, Shares)>,
}

pub trait Trait: frame_system::Trait {
	/// The staking balance.
	type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

	/// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    
    /// Percentage of `stake_promised` that is required for membership application's proposal bond
    /// - accepted gets it returned, rejected does not
    /// TODO: in the future, make this more accessible because this values security over accessibility
	type MembershipProposalBond: Get<Permill>;

    /// Minimum amount of funds that should be placed in a deposit for making a membership proposal
    /// - once again, for security reasons
    type MembershipProposalBondMinimum: Get<BalanceOf<Self>>;
    
    //// Maximum percentage of existing shares that can be issued in a BatchPeriod
    type MaximumShareIssuance: Get<Permill>;

	/// Batched membership changes
	type BatchPeriod: Get<Self::BlockNumber>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;
		fn deposit_event() = default;

		/// Fraction of a proposal's value that should be bonded in order to place the proposal.
		/// An accepted proposal gets these back. A rejected proposal does not.
		const MembershipProposalBond: Permill = T::MembershipProposalBond::get();

		/// Minimum amount of funds that should be placed in a deposit for making a proposal.
        const MembershipProposalBondMinimum: BalanceOf<T> = T::MembershipProposalBondMinimum::get();
        
        /// Maximum number of shares issued in a batch period
        const MaximumShareIssuance: Permill = T::MaximumShareIssuance::get();

		/// Period between successive batched membership changes
		const BatchPeriod: T::BlockNumber = T::BatchPeriod::get();
		
		/// Anyone can apply to exchange shares for capital
		/// - any punishment if the application fails and does this depend on how it fails?
		/// - 
		fn membership_application(origin, stake_promised: BalanceOf<T>, shares_requested: Shares) -> DispatchResult {
            let applicant = ensure_signed(origin)?;
            // don't restrict to non-members because this doubles for members requesting new share amounts
            // - they can apply for a grant to avoid collateral requirements (which should be provided somehow)

			let collateral = Self::calculate_member_application_bond(stake_promised.clone(), shares_requested.clone());
			T::Currency::reserve(&applicant, collateral)
				.map_err(|_| Error::<T>::InsufficientMembershipApplicantCollateral)?;
			let c = Self::membership_application_count() + 1;
			MembershipApplicationCount::put(c);
			let now = <system::Module<T>>::block_number();
            let membership_app = MembershipProposal {
                index: c,
                who: applicant.clone(),
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
		fn sponsor_membership_application(origin, max_share_bond: Shares, index: ProposalIndex) -> DispatchResult {
			let sponsor = ensure_signed(origin)?;
			ensure!(Self::is_member(&sponsor), Error::<T>::NotAMember);

			// (1)
			let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
			ensure!(wrapped_membership_proposal.is_some(), Error::<T>::IndexWithNoAssociatedMembershipProposal);
			let membership_proposal = wrapped_membership_proposal.expect("just checked above; qed");

			// (2) should be calculated by UI ahead of time and calculated, but this structure fosters dynamic collateral pricing
			let sponsor_bond = Self::calculate_membership_sponsor_bond(membership_proposal.stake_promised.clone(), membership_proposal.shares_requested.clone());
			ensure!(sponsor_bond <= max_share_bond, Error::<T>::SponsorBondExceedsExpectations);

			// (3) check if the sponsor has enough to afford the sponsor_bond
			let (reserved_shares, total_shares) = <MembershipShares<T>>::get(&sponsor).expect("invariant i: all members must have some shares and therefore some item in the shares map");
			// TODO: add overflow check here and resolution
			let new_reserved = reserved_shares + sponsor_bond;
			// check if the sponsor has enough free shares to afford the sponsor_bond
			ensure!(total_shares >= new_reserved, Error::<T>::InsufficientMembershipSponsorCollateral);

			// Enforce reservation of sponsor bond via permissioned access to runtime storage items
			let wrapped_member_signals = <OutstandingMemberSignals<T>>::get(&sponsor);
			let new_item = (membership_proposal.index, sponsor_bond);
			if wrapped_member_signals.is_none() {
				let new_signal: Vec<(ProposalIndex, Shares)> = [new_item].to_vec();
				<OutstandingMemberSignals<T>>::insert(&sponsor, new_signal);
			} else {
				// wrapped_member_signals.is_some()
				if let Some(mut signals) = wrapped_member_signals {
					// TODO: replace with `insert` because `mutate` this duplicates the first call to this storage item (`get`)
					// - `insert` isn't working instead because of some `EncodeLike` error
					<OutstandingMemberSignals<T>>::mutate(&sponsor, |s| signals.push(new_item));
				}
			}
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

		fn vote_on_membership(origin, index: ProposalIndex, direction: bool, magnitude: Shares) -> DispatchResult {
			let voter = ensure_signed(origin)?;
			ensure!(Self::is_member(&voter), Error::<T>::NotAMember);

			let wrapped_membership_proposal = <MembershipApplications<T>>::get(index);
			ensure!(wrapped_membership_proposal.is_some(), Error::<T>::IndexWithNoAssociatedMembershipProposal);
			let membership_proposal = wrapped_membership_proposal.expect("just checked above; qed");

			// check some global metric for how many proposals are being voted on right now (to limit spam scenarios)

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
		MembershipVotes get(fn membership_votes): map ProposalIndex => Option<MembershipVote<T::AccountId>>;
		/// Membership proposal indices that have been approved but not yet absorbed.
		MembershipApprovals get(fn membership_approvals): Vec<ProposalIndex>;

		/// Members should be replaced by group scaling logic
		Members get(fn members): Vec<T::AccountId>;
		/// Should be changed to `bank_accounts` when we scale this logic for sunshine
		BankAccount get(fn bank_account): Bank<T::AccountId>;
		/// Share amounts maps to (shares_reserved, total_shares) s.t. shares_reserved are reserved for votes or sponsorships
		MembershipShares get(fn membership_shares): map T::AccountId => Option<(Shares, Shares)>;
		/// Sponsorships, votes and all other outstanding signalling by members
		/// - This should be made into a more descriptive collateralization struct to enable selective rehypothecation for certain internal actions (like sponsorships sometimes)
		OutstandingMemberSignals get(fn outstanding_member_signals): map T::AccountId => Option<Vec<(ProposalIndex, Shares)>>;
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
		/// An application was sponsored by a member on-chain with some of their `Shares` at least equal to the `sponsor_quota` (metaparameter)
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
		/// Applicant can't cover collateral requirement for membership application
		InsufficientMembershipApplicantCollateral,
		/// Index doesn't haven associated membership proposal
		IndexWithNoAssociatedMembershipProposal,
		/// Required sponsorship bond exceeds upper bound inputted by user
		SponsorBondExceedsExpectations,
		/// Sponsor doesn't have enough shares to cover sponsor_quota requirement for membership application
		InsufficientMembershipSponsorCollateral,
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
	/// TODO: change logic herein to calculate bond based on ratio of `stake_promised` to `shares_requested` relative to existing parameterization
	fn calculate_member_application_bond(stake_promised: BalanceOf<T>, shares_requested: Shares) -> BalanceOf<T> {
		// calculate ratio of shares_requested to stake_promised
		// compare the ratio of the applicant to the ratio of the current group

		// old implementation - to be deleted
		T::MembershipProposalBondMinimum::get().max(T::MembershipProposalBond::get() * stake_promised)
	}

	/// The required sponsorship bond for membership proposals
	/// TODO: "" same as above
	fn calculate_membership_sponsor_bond(stake_promised: BalanceOf<T>, shares_requested: Shares) -> Shares {
		shares_requested
	}

// 	// Spend some money!
// 	// fn spend_funds() {
// 		// let mut budget_remaining = Self::pot();
// 		// Self::deposit_event(RawEvent::Spending(budget_remaining));

// 		// let mut missed_any = false;
// 		// let mut imbalance = <PositiveImbalanceOf<T>>::zero();
// 		// Approvals::mutate(|v| {
// 		// 	v.retain(|&index| {
// 		// 		// Should always be true, but shouldn't panic if false or we're screwed.
// 		// 		if let Some(p) = Self::proposals(index) {
// 		// 			if p.value <= budget_remaining {
// 		// 				budget_remaining -= p.value;
// 		// 				<Proposals<T>>::remove(index);

// 		// 				// return their deposit.
// 		// 				let _ = T::Currency::unreserve(&p.proposer, p.bond);

// 		// 				// provide the allocation.
// 		// 				imbalance.subsume(T::Currency::deposit_creating(&p.beneficiary, p.value));

// 		// 				Self::deposit_event(RawEvent::Awarded(index, p.value, p.beneficiary));
// 		// 				false
// 		// 			} else {
// 		// 				missed_any = true;
// 		// 				true
// 		// 			}
// 		// 		} else {
// 		// 			false
// 		// 		}
// 		// 	});
// 		// });

// 		// if !missed_any {
// 		// 	// burn some proportion of the remaining budget if we run a surplus.
// 		// 	let burn = (T::Burn::get() * budget_remaining).min(budget_remaining);
// 		// 	budget_remaining -= burn;
// 		// 	imbalance.subsume(T::Currency::burn(burn));
// 		// 	Self::deposit_event(RawEvent::Burnt(burn))
// 		// }

// 		// // Must never be an error, but better to be safe.
// 		// // proof: budget_remaining is account free balance minus ED;
// 		// // Thus we can't spend more than account free balance minus ED;
// 		// // Thus account is kept alive; qed;
// 		// if let Err(problem) = T::Currency::settle(
// 		// 	&Self::account_id(),
// 		// 	imbalance,
// 		// 	WithdrawReason::Transfer.into(),
// 		// 	ExistenceRequirement::KeepAlive
// 		// ) {
// 		// 	print("Inconsistent state - couldn't settle imbalance for funds spent by treasury");
// 		// 	// Nothing else to do here.
// 		// 	drop(problem);
// 		// }

// 		// Self::deposit_event(RawEvent::Rollover(budget_remaining));
// 	//}

	// -- MAKE BELOW METHODS SPECIFIC TO SOME TRAIT `impl BANKACCOUNT<T::ACCOUNTID> for Module<T>` --
	pub fn account_id() -> T::AccountId {
		BANK_ID.into_account()
	}

	/// Return the amount in the bank (in T::Currency denomination)
	fn bank_balance(bank: Bank<T::AccountId>) -> Result<BalanceOf<T>, Error<T>> {
		let account = bank.account.inner().ok_or(Error::<T>::NoBankOwner)?;
		let balance = T::Currency::free_balance(&account)
						// TODO: ponder whether this should be here (not if I don't follow the same existential deposit system as polkadot...)
						// Must never be less than 0 but better be safe.
						.saturating_sub(T::Currency::minimum_balance());
		Ok(balance)
	}

	/// Ratio of the `bank.balance` to `bank.shares`
	/// -this value may be interpreted as `currency_per_share` by UIs, but that would assume immediate liquidity which is false
	fn collateralization_ratio(bank: Bank<T::AccountId>) -> Result<Permill, Error<T>> {
		let most_recent_balance = Self::bank_balance(bank.clone())?;
		let share_count = BalanceOf::<T>::from(bank.shares);
		// TODO: #make_issue for calculating this?
		let ratio = Permill::from_rational_approximation(most_recent_balance, share_count);
		Ok(ratio)
	}
}