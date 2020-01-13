// Moloch Module
#![cfg_attr(not(feature = "std"), no_std)]

mod origins;
use util::{Threshold, Signal};
use codec::{Decode, Encode};
use frame_support::traits::{ChangeMembers, Currency, Get, ReservableCurrency};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult, ensure,
};
use frame_system::{self as system, ensure_root, ensure_signed};
use sp_runtime::{
    traits::{
        AccountIdConversion, CheckedSub, EnsureOrigin, IntegerSquareRoot, Saturating, StaticLookup,
        TrailingZeroInput, Zero,
    },
    ModuleId, Percent, Permill, RuntimeDebug,
};
use sp_std::prelude::*;

/// Shares Type
/// - working on Signal trait in util but this works for prototyping...
/// ...just keep things as readable as possible (trade precision for readability)
pub type Shares = u32;
type BalanceOf<T, I> = <<T as Trait<I>>::Collateral as Currency<<T as system::Trait>::AccountId>>::Balance;

const MODULE_ID: ModuleId = ModuleId(*b"mololoch");

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct GrantApplication<AccountId, BalanceOf, BlockNumber, I> {
    /// Identifier for this grant application
    /// - just a nonce for now
    id: u32,
    /// The recipient group
    /// - replace this with a `GroupIdentifier`
    /// - map the `GroupIdentifier` to a new origin generated when this proposal is passed and manages this group's decisions (see `origins::recipients`)
    who: Vec<AccountId>,
    /// Schedule for payouts
    /// - instead of encoding it like this, it should be encoded as a polynomial...this data structure costs more the longer the proposed duration
    /// - see `VestingSchedule` and staking/inflation curve
    schedule: Vec<(BlockNumber, BalanceOf<I>)>,
}

/// The module's configuration trait
pub trait Trait<I = DefaultInstance>: system::Trait {
    /// This module's raw origin (membership's origin for now)
    type Origin: From<RawOrigin<Self::AccountId, <Self as Trait<I>>::Signal, I>>;

    /// The overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as system::Trait>::Event>;
	
	// Signal or Shares type will go here

    /// The type that corresponds to some outside currency
    type Collateral: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

    /// The receiver of the signal for when the members have changed
    /// TODO: this is the hook for which share issuance should be triggered
    type MembershipChanged: ChangeMembers<Self::AccountId>;

	/// import the meta origin instead of using a `FounderOrigin` here
}

// have a local origin at first for this and then plan how to scale it out
#[derive(PartialEq, Eq, Clone, RuntimeDebug)]
pub enum RawOrigin<AccountId, I> {
    /// single founder to start
    Founder(AccountId, Shares),
    /// multiple founders upon initialization
    Founders(Vec<(AccountId, Shares)>),
    /// (a, b, c) s.t. a = yes_votes, b = all_votes, c = all_possible_votes
    ShareWeighted(Shares, Shares, Shares),
    /// Dummy to manage the fact we have instancing.
    _Phantom(sp_std::marker::PhantomData<I>),
}

/// Origin for this module.
pub type Origin<T, I = DefaultInstance> =
    RawOrigin<<T as frame_system::Trait>::AccountId, <T as Trait<I>>::Signal, I>;

/// These are the event types which form a log of state transitions indexed by clients
decl_event! {
    /// Events for this module.
    pub enum Event<T, I=DefaultInstance> where
        AccountId = <T as system::Trait>::AccountId,
        Balance = BalanceOf<T, I>,
        Shares = Shares<T, I>,
    {
        Example(AccountId, Balance, Shares),
    }
}

decl_error! {
    /// Errors for this module.
    pub enum Error for Module<T: Trait<I>, I: Instance> {
        /// User is not a member and can't perform attempted action
        NotAMember,
        /// User is a member but tried to apply without membership
        /// - could add punishment for this option but whatever, like a cost of paying for that invocation
        /// - client-side should take care of it beforehand and not require this wasted runtime computation
        IsAMember,
    }
}

// This module's storage items.
decl_storage! {
    trait Store for Module<T: Trait<I>, I: Instance=DefaultInstance> as Moloch {
		/// Pull these in from meta or otherwise assign them
        /// The current set of members
        pub Members get(members): Vec<T::AccountId>;
        /// The shares of members
		pub MemberShares get(fn member_shares): map T::AccountId => Shares<T, I>;
		
        /// The current grant applications
		GrantApplications: Vec<GrantApplication<T::AccountId, BalanceOf<T, I>, T::BlockNumber>>;

		///
		
		/// Sponsored Member

        /// Pending payouts; ordered by block number, with the amount that should be paid out.
        /// - eventually using `sauce::scheduler` for logic isolation (increased future auditability)
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
    pub struct Module<T: Trait<I>, I: Instance=DefaultInstance> for enum Call where origin: <T as frame_system::Trait>::Origin {
        type Error = Error<T, I>;

        // Used for handling module events.
        fn deposit_event() = default;

        // delete once more logic is added
        fn example_runtime_method(origin) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            let members = <Members<T, I>>::get();
            ensure!(Self::is_member(&members, &voter), Error::<T, I>::NotAMember);

            Self::deposit_event(RawEvent::Example(voter, 26.into(), 32.into()));
            Ok(())
		}
		
		fn sponsor_application(origin) -> DispatchResult {
			Ok(())
		}

		fn grant_proposal(origin) -> DispatchResult {
			Ok(())
		}

		fn vote(origin) -> DispatchResult {
			Ok(())
		}

		fn vote_member(origin) -> DispatchResult {
			Ok(())
		}
    }
}

impl<T: Trait<I>, I: Instance> Module<T, I> {
    /// Binary search of is_members to verify membership
    /// - noted: by passing in the set of members, we can reuse it in the other runtime method instead of two calls to storage!
    fn is_member(members: &Vec<T::AccountId>, who: &T::AccountId) -> bool {
        members.binary_search(who).is_ok()
    } // TODO: should change back if most usages don't call map 2 or more times in runtime methods
}

pub struct EnsureFounder<AccountId, Shares, I = DefaultInstance>(
    sp_std::marker::PhantomData<(AccountId, Shares, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, AccountId, Shares, I> EnsureOrigin<O> for EnsureFounder<AccountId, Shares, I>
{
    type Success = (AccountId, Shares);
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
            // TODO: does another check need to be made here?
            RawOrigin::Founder(id, shares) => Ok((id, shares)),
            // only have to return `AccountId` if call fails
            r => Err(O::from(r)),
        })
    }
}

pub struct EnsureFounders<AccountId, Shares, I = DefaultInstance>(
    sp_std::marker::PhantomData<(AccountId, Shares, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, AccountId, Shares, I> EnsureOrigin<O> for EnsureFounders<AccountId, Shares, I>
{
    type Success = Vec<(AccountId, Shares)>;
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
            // TODO: does another check need to be made here?
            RawOrigin::Founders(vector_of_id_shares) => Ok(vector_of_id_shares),
            // only need to return `AccountId` if call fails
            r => Err(O::from(r)),
        })
    }
}

// Measures proportion of share weight passed in through the origin
pub struct EnsureShareWeightMoreThan<H: Threshold, AccountId, Shares, T: Trait<I>, I = DefaultInstance>(
    sp_std::marker::PhantomData<(H, AccountId, Shares, T, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, H: Threshold, AccountId, Shares, T: Trait<I>, I> EnsureOrigin<O> for EnsureShareWeightMoreThan<H, AccountId, Shares, T, I>
{
    type Success = (Shares, Shares, Shares);
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
			// weak sauce - update this with some default curve parameterization between a, b, and c
            RawOrigin::ShareWeighted(a, b, c) => Ok((a, b, c)),
            r => Err(O::from(r)),
        })
    }
}

pub struct EnsureShareWeightAtLeast<H: Threshold, AccountId, Shares, T: Trait<I>, I = DefaultInstance>(
    sp_std::marker::PhantomData<(H, AccountId, Shares, T, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, H: Threshold, AccountId, Shares, T: Trait<I>, I> EnsureOrigin<O> for EnsureShareWeightAtLeast<H, AccountId, Shares, T, I>
{
    type Success = (Shares, Shares, Shares);
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
			// TODO: check actual thresholds - see democracy and issue #
			RawOrigin::ShareWeighted(a, b, c) => Ok((a, b, c)),
			// this is exhaustive? wtf is `O`
            r => Err(O::from(r)),
        })
    }
}

// todo: tests in another file
