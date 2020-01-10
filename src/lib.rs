// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

mod origins;
mod util;
use util::Power;

use rand_chacha::{rand_core::{RngCore, SeedableRng}, ChaChaRng};
use sp_std::prelude::*;
use codec::{Encode, Decode};
use sp_runtime::{Percent, ModuleId, RuntimeDebug,
	traits::{
		StaticLookup, AccountIdConversion, Saturating, Zero, IntegerSquareRoot,
		TrailingZeroInput, CheckedSub, EnsureOrigin
	}
};
use frame_support::{decl_error, decl_module, decl_storage, decl_event, ensure, dispatch::DispatchResult};
use frame_support::weights::SimpleDispatchInfo;
use frame_support::traits::{
	ReservableCurrency, Get, ChangeMembers,
};
use frame_system::{self as system, ensure_signed, ensure_root};

type Shares<T, I> = <<T as Trait<I>>::Signal as Signal<<T as system::Trait>::AccountId>>::Shares;
type BalanceOf<T, I> = <<T as Trait<I>>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

const MODULE_ID: ModuleId = ModuleId(*b"mololoch");

/// The module's configuration trait
pub trait Trait<I=DefaultInstance>: system::Trait {
	/// The overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as system::Trait>::Event>;

    /// The type that corresponds to native signal
    type Signal: Signal<Self::AccountId>;

    /// The type that corresponds to some outside currency
    type Currency: Currency<Self::AccountId>;

    /// The native value standard, corresponding to collateral (TODO: make own)
    type Collateral: ReservableCurrency<Self::AccountId>;

    /// The receiver of the signal for when the members have changed
    /// TODO: this is the hook for which signal's issuance should be triggered
    type MembershipChanged: ChangeMembers<Self::AccountId>;
    
    // TODO: add membership origin(s)

	/// The origin that is allowed to call `found`.
	type FounderOrigin: EnsureOrigin<Self::Origin>;
}

/// An application to join the membership
#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct MemberApplication<AccountId, Currency, Shares> {
	/// The applicant
	who: AccountId,
    /// The collateral promised and slowly staked over the duration of the proposal process
    /// TODO: make issue for why this should be made more complex eventually s.t. amount staked only is applied once approved and reserved changes based on prob(passage)
    collateral: Currency,
    /// The reward that the bidder has requested for successfully joining the society.
    shares_requested: Shares,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct GrantApplication<AccountId, Currency, BlockNumber> {
    /// Identifier for this grant application
    /// - just a nonce for now
    id: u32,
    /// The recipient group
    /// - replace this with a `GroupIdentifier`
    /// - map the `GroupIdentifier` to a new origin generated when this proposal is passed and manages this group's decisions
    who: Vec<AccountId>,
    /// Schedule for payouts
    /// - instead of encoding it like this, it should be encoded as a polynomial...this data structure costs more the longer the proposed duration
    /// - see `VestingSchedule` and staking/inflation curve
    schedule: Vec<(BlockNumber, Currency)>,
}

// This module's storage items.
decl_storage! {
	trait Store for Module<T: Trait<I>, I: Instance=DefaultInstance> as Moloch {
		/// The current set of candidates; bidders that are attempting to become members.
		pub Candidates get(candidates): Vec<Bid<T::AccountId, BalanceOf<T, I>>>;

		/// Upper bound on how much from the treasury can be spent every round (TODO: define round length)
		pub Budget get(fn budget) config(): BalanceOf<T, I>;

		/// The current set of members
		pub Members get(fn members): Vec<(T::AccountId, T::Shares)>;
        
        /// The current membership applications
        MemberApplications: Vec<MemberApplication<T::AccountId, BalanceOf<T, I>, T::BlockNumber>>;

		/// The current grant applications
		GrantApplications: Vec<GrantApplication<T::AccountId, BalanceOf<T, I>, T::BlockNumber>>;

		/// Pending payouts; ordered by block number, with the amount that should be paid out.
		Payouts: map T::AccountId => Vec<(T::BlockNumber, BalanceOf<T, I>)>;
        
        /// Nonce (for grant application)
        Nonce: u32;
    }
    // TODO: add back later and configure members storage item above
	// add_extra_genesis {
	// 	config(members): Vec<(T::AccountId, T::Shares)>;
	// }
}

// The module's dispatchable functions.
decl_module! {
	/// The module declaration.
	pub struct Module<T: Trait<I>, I: Instance=DefaultInstance> for enum Call where origin: T::Origin {
		type Error = Error<T, I>;
		/// The minimum amount of a deposit required for a bid to be made.
		const CandidateDeposit: BalanceOf<T, I> = T::CandidateDeposit::get();

		/// The amount of the unpaid reward that gets deducted in the case that either a skeptic
		/// doesn't vote or someone votes in the wrong way.
		const WrongSideDeduction: BalanceOf<T, I> = T::WrongSideDeduction::get();

		/// The number of times a member may vote the wrong way (or not at all, when they are a skeptic)
		/// before they become suspended.
		const MaxStrikes: u32 = T::MaxStrikes::get();

		/// The amount of incentive paid within each period. Doesn't include VoterTip.
		const PeriodSpend: BalanceOf<T, I> = T::PeriodSpend::get();

		/// The number of blocks between candidate/membership rotation periods.
		const RotationPeriod: T::BlockNumber = T::RotationPeriod::get();

		/// The number of blocks between membership challenges.
		const ChallengePeriod: T::BlockNumber = T::ChallengePeriod::get();

		// Used for handling module events.
		fn deposit_event() = default;

		/// A user outside of the society can make a bid for entry.
		///
		/// Payment: `CandidateDeposit` will be reserved for making a bid. It is returned
		/// when the bid becomes a member, or if the bid calls `unbid`.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// Parameters:
		/// - `value`: A one time payment the bid would like to receive when joining the society.
		///
		/// # <weight>
		/// Key: B (len of bids), C (len of candidates), M (len of members), X (balance reserve)
		/// - Storage Reads:
		/// 	- One storage read to check for suspended candidate. O(1)
		/// 	- One storage read to check for suspended member. O(1)
		/// 	- One storage read to retrieve all current bids. O(B)
		/// 	- One storage read to retrieve all current candidates. O(C)
		/// 	- One storage read to retrieve all members. O(M)
		/// - Storage Writes:
		/// 	- One storage mutate to add a new bid to the vector O(B) (TODO: possible optimization w/ read)
		/// 	- Up to one storage removal if bid.len() > MAX_BID_COUNT. O(1)
		/// - Notable Computation:
		/// 	- O(B + C + log M) search to check user is not already a part of society.
		/// 	- O(log B) search to insert the new bid sorted.
		/// - External Module Operations:
		/// 	- One balance reserve operation. O(X)
		/// 	- Up to one balance unreserve operation if bids.len() > MAX_BID_COUNT.
		/// - Events:
		/// 	- One event for new bid.
		/// 	- Up to one event for AutoUnbid if bid.len() > MAX_BID_COUNT.
		///
		/// Total Complexity: O(M + B + C + logM + logB + X)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(50_000)]
		pub fn bid(origin, value: BalanceOf<T, I>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(!<SuspendedCandidates<T, I>>::exists(&who), Error::<T, I>::Suspended);
			ensure!(!<SuspendedMembers<T, I>>::exists(&who), Error::<T, I>::Suspended);
			let bids = <Bids<T, I>>::get();
			ensure!(!Self::is_bid(&bids, &who), Error::<T, I>::AlreadyBid);
			let candidates = <Candidates<T, I>>::get();
			ensure!(!Self::is_candidate(&candidates, &who), Error::<T, I>::AlreadyCandidate);
			let members = <Members<T, I>>::get();
			ensure!(!Self::is_member(&members ,&who), Error::<T, I>::AlreadyMember);

			let deposit = T::CandidateDeposit::get();
			T::Currency::reserve(&who, deposit)?;

			Self::put_bid(bids, &who, value.clone(), BidKind::Deposit(deposit));
			Self::deposit_event(RawEvent::Bid(who, value));
			Ok(())
		}

		/// A bidder can remove their bid for entry into society.
		/// By doing so, they will have their candidate deposit returned or
		/// they will unvouch their voucher.
		///
		/// Payment: The bid deposit is unreserved if the user made a bid.
		///
		/// The dispatch origin for this call must be _Signed_ and a bidder.
		///
		/// Parameters:
		/// - `pos`: Position in the `Bids` vector of the bid who wants to unbid.
		///
		/// # <weight>
		/// Key: B (len of bids), X (balance unreserve)
		/// - One storage read and write to retrieve and update the bids. O(B)
		/// - Either one unreserve balance action O(X) or one vouching storage removal. O(1)
		/// - One event.
		///
		/// Total Complexity: O(B + X)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(20_000)]
		pub fn unbid(origin, pos: u32) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let pos = pos as usize;
			<Bids<T, I>>::mutate(|b|
				if pos < b.len() && b[pos].who == who {
					// Either unreserve the deposit or free up the vouching member.
					// In neither case can we do much if the action isn't completable, but there's
					// no reason that either should fail.
					match b.remove(pos).kind {
						BidKind::Deposit(deposit) => {
							let _ = T::Currency::unreserve(&who, deposit);
						}
						BidKind::Vouch(voucher, _) => {
							<Vouching<T, I>>::remove(&voucher);
						}
					}
					Self::deposit_event(RawEvent::Unbid(who));
					Ok(())
				} else {
					Err(Error::<T, I>::BadPosition)?
				}
			)
		}

		/// As a member, vouch for someone to join society by placing a bid on their behalf.
		///
		/// There is no deposit required to vouch for a new bid, but a member can only vouch for
		/// one bid at a time. If the bid becomes a suspended candidate and ultimately rejected by
		/// the suspension judgement origin, the member will be banned from vouching again.
		///
		/// As a vouching member, you can claim a tip if the candidate is accepted. This tip will
		/// be paid as a portion of the reward the member will receive for joining the society.
		///
		/// The dispatch origin for this call must be _Signed_ and a member.
		///
		/// Parameters:
		/// - `who`: The user who you would like to vouch for.
		/// - `value`: The total reward to be paid between you and the candidate if they become
		/// a member in the society.
		/// - `tip`: Your cut of the total `value` payout when the candidate is inducted into
		/// the society. Tips larger than `value` will be saturated upon payout.
		///
		/// # <weight>
		/// Key: B (len of bids), C (len of candidates), M (len of members)
		/// - Storage Reads:
		/// 	- One storage read to retrieve all members. O(M)
		/// 	- One storage read to check member is not already vouching. O(1)
		/// 	- One storage read to check for suspended candidate. O(1)
		/// 	- One storage read to check for suspended member. O(1)
		/// 	- One storage read to retrieve all current bids. O(B)
		/// 	- One storage read to retrieve all current candidates. O(C)
		/// - Storage Writes:
		/// 	- One storage write to insert vouching status to the member. O(1)
		/// 	- One storage mutate to add a new bid to the vector O(B) (TODO: possible optimization w/ read)
		/// 	- Up to one storage removal if bid.len() > MAX_BID_COUNT. O(1)
		/// - Notable Computation:
		/// 	- O(log M) search to check sender is a member.
		/// 	- O(B + C + log M) search to check user is not already a part of society.
		/// 	- O(log B) search to insert the new bid sorted.
		/// - External Module Operations:
		/// 	- One balance reserve operation. O(X)
		/// 	- Up to one balance unreserve operation if bids.len() > MAX_BID_COUNT.
		/// - Events:
		/// 	- One event for vouch.
		/// 	- Up to one event for AutoUnbid if bid.len() > MAX_BID_COUNT.
		///
		/// Total Complexity: O(M + B + C + logM + logB + X)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(50_000)]
		pub fn vouch(origin, who: T::AccountId, value: BalanceOf<T, I>, tip: BalanceOf<T, I>) -> DispatchResult {
			let voucher = ensure_signed(origin)?;
			// Check user is not suspended.
			ensure!(!<SuspendedCandidates<T, I>>::exists(&who), Error::<T, I>::Suspended);
			ensure!(!<SuspendedMembers<T, I>>::exists(&who), Error::<T, I>::Suspended);
			// Check user is not a bid or candidate.
			let bids = <Bids<T, I>>::get();
			ensure!(!Self::is_bid(&bids, &who), Error::<T, I>::AlreadyBid);
			let candidates = <Candidates<T, I>>::get();
			ensure!(!Self::is_candidate(&candidates, &who), Error::<T, I>::AlreadyCandidate);
			// Check user is not already a member.
			let members = <Members<T, I>>::get();
			ensure!(!Self::is_member(&members, &who), Error::<T, I>::AlreadyMember);
			// Check sender can vouch.
			ensure!(Self::is_member(&members, &voucher), Error::<T, I>::NotMember);
			ensure!(!<Vouching<T, I>>::exists(&voucher), Error::<T, I>::AlreadyVouching);

			<Vouching<T, I>>::insert(&voucher, VouchingStatus::Vouching);
			Self::put_bid(bids, &who, value.clone(), BidKind::Vouch(voucher.clone(), tip));
			Self::deposit_event(RawEvent::Vouch(who, value, voucher));
			Ok(())
		}

		/// As a vouching member, unvouch a bid. This only works while vouched user is
		/// only a bidder (and not a candidate).
		///
		/// The dispatch origin for this call must be _Signed_ and a vouching member.
		///
		/// Parameters:
		/// - `pos`: Position in the `Bids` vector of the bid who should be unvouched.
		///
		/// # <weight>
		/// Key: B (len of bids)
		/// - One storage read O(1) to check the signer is a vouching member.
		/// - One storage mutate to retrieve and update the bids. O(B)
		/// - One vouching storage removal. O(1)
		/// - One event.
		///
		/// Total Complexity: O(B)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(20_000)]
		pub fn unvouch(origin, pos: u32) -> DispatchResult {
			let voucher = ensure_signed(origin)?;
			ensure!(Self::vouching(&voucher) == Some(VouchingStatus::Vouching), Error::<T, I>::NotVouching);

			let pos = pos as usize;
			<Bids<T, I>>::mutate(|b|
				if pos < b.len() {
					b[pos].kind.check_voucher(&voucher)?;
					<Vouching<T, I>>::remove(&voucher);
					let who = b.remove(pos).who;
					Self::deposit_event(RawEvent::Unvouch(who));
					Ok(())
				} else {
					Err(Error::<T, I>::BadPosition)?
				}
			)
		}

		/// As a member, vote on a candidate.
		///
		/// The dispatch origin for this call must be _Signed_ and a member.
		///
		/// Parameters:
		/// - `candidate`: The candidate that the member would like to bid on.
		/// - `approve`: A boolean which says if the candidate should be
		///              approved (`true`) or rejected (`false`).
		///
		/// # <weight>
		/// Key: C (len of candidates), M (len of members)
		/// - One storage read O(M) and O(log M) search to check user is a member.
		/// - One account lookup.
		/// - One storage read O(C) and O(C) search to check that user is a candidate.
		/// - One storage write to add vote to votes. O(1)
		/// - One event.
		///
		/// Total Complexity: O(M + logM + C)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(30_000)]
		pub fn vote(origin, candidate: <T::Lookup as StaticLookup>::Source, approve: bool) {
			let voter = ensure_signed(origin)?;
			let candidate = T::Lookup::lookup(candidate)?;
			let candidates = <Candidates<T, I>>::get();
			ensure!(Self::is_candidate(&candidates, &candidate), Error::<T, I>::NotCandidate);
			let members = <Members<T, I>>::get();
			ensure!(Self::is_member(&members, &voter), Error::<T, I>::NotMember);

			let vote = if approve { Vote::Approve } else { Vote::Reject };
			<Votes<T, I>>::insert(&candidate, &voter, vote);

			Self::deposit_event(RawEvent::Vote(candidate, voter, approve));
		}

		/// As a member, vote on the defender.
		///
		/// The dispatch origin for this call must be _Signed_ and a member.
		///
		/// Parameters:
		/// - `approve`: A boolean which says if the candidate should be
		/// approved (`true`) or rejected (`false`).
		///
		/// # <weight>
		/// - Key: M (len of members)
		/// - One storage read O(M) and O(log M) search to check user is a member.
		/// - One storage write to add vote to votes. O(1)
		/// - One event.
		///
		/// Total Complexity: O(M + logM)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(20_000)]
		pub fn defender_vote(origin, approve: bool) {
			let voter = ensure_signed(origin)?;
			let members = <Members<T, I>>::get();
			ensure!(Self::is_member(&members, &voter), Error::<T, I>::NotMember);

			let vote = if approve { Vote::Approve } else { Vote::Reject };
			<DefenderVotes<T, I>>::insert(&voter, vote);

			Self::deposit_event(RawEvent::DefenderVote(voter, approve));
		}

		/// Transfer the first matured payout for the sender and remove it from the records.
		///
		/// NOTE: This extrinsic needs to be called multiple times to claim multiple matured payouts.
		///
		/// Payment: The member will receive a payment equal to their first matured
		/// payout to their free balance.
		///
		/// The dispatch origin for this call must be _Signed_ and a member with
		/// payouts remaining.
		///
		/// # <weight>
		/// Key: M (len of members), P (number of payouts for a particular member)
		/// - One storage read O(M) and O(log M) search to check signer is a member.
		/// - One storage read O(P) to get all payouts for a member.
		/// - One storage read O(1) to get the current block number.
		/// - One currency transfer call. O(X)
		/// - One storage write or removal to update the member's payouts. O(P)
		///
		/// Total Complexity: O(M + logM + P + X)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(30_000)]
		pub fn payout(origin) {
			let who = ensure_signed(origin)?;

			let members = <Members<T, I>>::get();
			ensure!(Self::is_member(&members, &who), Error::<T, I>::NotMember);

			let mut payouts = <Payouts<T, I>>::get(&who);
			if let Some((when, amount)) = payouts.first() {
				if when <= &<system::Module<T>>::block_number() {
					T::Currency::transfer(&Self::payouts(), &who, *amount, KeepAlive)?;
					payouts.remove(0);
					if payouts.is_empty() {
						<Payouts<T, I>>::remove(&who);
					} else {
						<Payouts<T, I>>::insert(&who, payouts);
					}
					return Ok(())
				}
			}
			Err(Error::<T, I>::NoPayout)?
		}

		/// Found the society.
		///
		/// This is done as a discrete action in order to allow for the
		/// module to be included into a running chain and can only be done once.
		///
		/// The dispatch origin for this call must be from the _FounderOrigin_.
		///
		/// Parameters:
		/// - `founder` - The first member and head of the newly founded society.
		///
		/// # <weight>
		/// - One storage read to check `Head`. O(1)
		/// - One storage write to add the first member to society. O(1)
		/// - One storage write to add new Head. O(1)
		/// - One event.
		///
		/// Total Complexity: O(1)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(10_000)]
		fn found(origin, founder: T::AccountId) {
			T::FounderOrigin::ensure_origin(origin)?;
			ensure!(!<Head<T, I>>::exists(), Error::<T, I>::AlreadyFounded);
			// This should never fail in the context of this function...
			Self::add_member(&founder)?;
			<Head<T, I>>::put(&founder);
			Self::deposit_event(RawEvent::Founded(founder));
		}

		/// Allow suspension judgement origin to make judgement on a suspended member.
		///
		/// If a suspended member is forgiven, we simply add them back as a member, not affecting
		/// any of the existing storage items for that member.
		///
		/// If a suspended member is rejected, remove all associated storage items, including
		/// their payouts, and remove any vouched bids they currently have.
		///
		/// The dispatch origin for this call must be from the _SuspensionJudgementOrigin_.
		///
		/// Parameters:
		/// - `who` - The suspended member to be judged.
		/// - `forgive` - A boolean representing whether the suspension judgement origin
		///               forgives (`true`) or rejects (`false`) a suspended member.
		///
		/// # <weight>
		/// Key: B (len of bids), M (len of members)
		/// - One storage read to check `who` is a suspended member. O(1)
		/// - Up to one storage write O(M) with O(log M) binary search to add a member back to society.
		/// - Up to 3 storage removals O(1) to clean up a removed member.
		/// - Up to one storage write O(B) with O(B) search to remove vouched bid from bids.
		/// - Up to one additional event if unvouch takes place.
		/// - One storage removal. O(1)
		/// - One event for the judgement.
		///
		/// Total Complexity: O(M + logM + B)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(30_000)]
		fn judge_suspended_member(origin, who: T::AccountId, forgive: bool) {
			T::SuspensionJudgementOrigin::ensure_origin(origin)?;
			ensure!(<SuspendedMembers<T, I>>::exists(&who), Error::<T, I>::NotSuspended);
			
			if forgive {
				// Try to add member back to society. Can fail with `MaxMembers` limit.
				Self::add_member(&who)?;
			} else {
				// Cancel a suspended member's membership, remove their payouts.
				<Payouts<T, I>>::remove(&who);
				<Strikes<T, I>>::remove(&who);
				// Remove their vouching status, potentially unbanning them in the future.
				if <Vouching<T, I>>::take(&who) == Some(VouchingStatus::Vouching) {
					// Try to remove their bid if they are vouching.
					// If their vouch is already a candidate, do nothing.
					<Bids<T, I>>::mutate(|bids|
						// Try to find the matching bid
						if let Some(pos) = bids.iter().position(|b| b.kind.check_voucher(&who).is_ok()) {
							// Remove the bid, and emit an event
							let vouched = bids.remove(pos).who;
							Self::deposit_event(RawEvent::Unvouch(vouched));
						}
					);
				}
			}

			<SuspendedMembers<T, I>>::remove(&who);
			Self::deposit_event(RawEvent::SuspendedMemberJudgement(who, forgive));
		}


		/// Allow suspended judgement origin to make judgement on a suspended candidate.
		///
		/// If the judgement is `Approve`, we add them to society as a member with the appropriate
		/// payment for joining society.
		///
		/// If the judgement is `Reject`, we either slash the deposit of the bid, giving it back
		/// to the society treasury, or we ban the voucher from vouching again.
		///
		/// If the judgement is `Rebid`, we put the candidate back in the bid pool and let them go
		/// through the induction process again.
		///
		/// The dispatch origin for this call must be from the _SuspensionJudgementOrigin_.
		///
		/// Parameters:
		/// - `who` - The suspended candidate to be judged.
		/// - `judgement` - `Approve`, `Reject`, or `Rebid`.
		///
		/// # <weight>
		/// Key: B (len of bids), M (len of members), X (balance action)
		/// - One storage read to check `who` is a suspended candidate.
		/// - One storage removal of the suspended candidate.
		/// - Approve Logic
		/// 	- One storage read to get the available pot to pay users with. O(1)
		/// 	- One storage write to update the available pot. O(1)
		/// 	- One storage read to get the current block number. O(1)
		/// 	- One storage read to get all members. O(M)
		/// 	- Up to one unreserve currency action.
		/// 	- Up to two new storage writes to payouts.
		/// 	- Up to one storage write with O(log M) binary search to add a member to society.
		/// - Reject Logic
		/// 	- Up to one repatriate reserved currency action. O(X)
		/// 	- Up to one storage write to ban the vouching member from vouching again.
		/// - Rebid Logic
		/// 	- Storage mutate with O(log B) binary search to place the user back into bids.
		/// - Up to one additional event if unvouch takes place.
		/// - One storage removal.
		/// - One event for the judgement.
		///
		/// Total Complexity: O(M + logM + B + X)
		/// # </weight>

		#[weight = SimpleDispatchInfo::FixedNormal(50_000)]
		fn judge_suspended_candidate(origin, who: T::AccountId, judgement: Judgement) {
			T::SuspensionJudgementOrigin::ensure_origin(origin)?;
			if let Some((value, kind)) = <SuspendedCandidates<T, I>>::get(&who) {
				match judgement {
					Judgement::Approve => {
						// Suspension Judgement origin has approved this candidate
						// Make sure we can pay them
						let pot = Self::pot();
						ensure!(pot >= value, Error::<T, I>::InsufficientPot);
						// Try to add user as a member! Can fail with `MaxMember` limit.
						Self::add_member(&who)?;
						// Reduce next pot by payout
						<Pot<T, I>>::put(pot - value);
						// Add payout for new candidate
						let maturity = <system::Module<T>>::block_number()
							+ Self::lock_duration(Self::members().len() as u32);
						Self::pay_accepted_candidate(&who, value, kind, maturity);
					}
					Judgement::Reject => {
						// Founder has rejected this candidate
						match kind {
							BidKind::Deposit(deposit) => {
								// Slash deposit and move it to the society account
								let _ = T::Currency::repatriate_reserved(&who, &Self::account_id(), deposit);
							}
							BidKind::Vouch(voucher, _) => {
								// Ban the voucher from vouching again
								<Vouching<T, I>>::insert(&voucher, VouchingStatus::Banned);
							}
						}
					}
					Judgement::Rebid => {
						// Founder has taken no judgement, and candidate is placed back into the pool.
						let bids = <Bids<T, I>>::get();
						Self::put_bid(bids, &who, value, kind);
					}
				}

				// Remove suspended candidate
				<SuspendedCandidates<T, I>>::remove(who);
			} else {
				Err(Error::<T, I>::NotSuspended)?
			}
		}

		fn on_initialize(n: T::BlockNumber) {
			let mut members = vec![];

			// Run a candidate/membership rotation
			if (n % T::RotationPeriod::get()).is_zero() {
				members = <Members<T, I>>::get();
				Self::rotate_period(&mut members);
			}

			// Run a challenge rotation
			if (n % T::ChallengePeriod::get()).is_zero() {
				// Only read members if not already read.
				if members.is_empty() {
					members = <Members<T, I>>::get();
				}
				Self::rotate_challenge(&mut members);
			}
		}
	}
}

decl_error! {
	/// Errors for this module.
	pub enum Error for Module<T: Trait<I>, I: Instance> {
		/// An incorrect position was provided.
		BadPosition,
		/// User is not a member
	}
}

decl_event! {
	/// Events for this module.
	pub enum Event<T, I=DefaultInstance> where
		AccountId = <T as system::Trait>::AccountId,
		Balance = BalanceOf<T, I>
	{
		Vouch(AccountId, Balance, AccountId),
	}
}

/// Pick an item at pseudo-random from the slice, given the `rng`. `None` iff the slice is empty.
fn pick_item<'a, R: RngCore, T>(rng: &mut R, items: &'a [T]) -> Option<&'a T> {
	if items.is_empty() {
		None
	} else {
		Some(&items[pick_usize(rng, items.len() - 1)])
	}
}

/// Pick a new PRN, in the range [0, `max`] (inclusive).
fn pick_usize<'a, R: RngCore>(rng: &mut R, max: usize) -> usize {

	(rng.next_u32() % (max as u32 + 1)) as usize
}

// impl<T: Trait<I>, I: Instance> Module<T, I> {
	
// }
