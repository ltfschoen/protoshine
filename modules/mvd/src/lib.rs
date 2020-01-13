// Moloch Module
#![cfg_attr(not(feature = "std"), no_std)]

mod origins;
mod util;
use util::Signal;

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

pub type Shares<T, I> = <<T as Trait<I>>::Signal as Signal<<T as system::Trait>::AccountId>>::Shares;
type BalanceOf<T, I> = <<T as Trait<I>>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

const MODULE_ID: ModuleId = ModuleId(*b"mololoch");

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

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct GrantApplication<AccountId, Currency, BlockNumber> {
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
    schedule: Vec<(BlockNumber, Currency)>,
}

/// The module's configuration trait
pub trait Trait<I = DefaultInstance>: system::Trait {
    /// This module's raw origin (membership's origin for now)
    type Origin: From<RawOrigin<Self::AccountId, <Self as Trait<I>>::Signal, I>>;

    /// The overarching event type.
    type Event: From<Event<Self, I>> + Into<<Self as system::Trait>::Event>;

    /// The type that corresponds to native signal
    type Signal: Signal<Self::AccountId>;

    /// The type that corresponds to some outside currency
    /// TODO: change this and collateral to different types in `util::signal` based on requirements (impl From<Currency> though)
    type Currency: Currency<Self::AccountId>;

    /// The native value standard, corresponding to collateral
    type Collateral: ReservableCurrency<Self::AccountId>;

    /// The receiver of the signal for when the members have changed
    /// TODO: this is the hook for which signal's issuance should be triggered
    type MembershipChanged: ChangeMembers<Self::AccountId>;

    /// The origin that is allowed to call `found`
    /// - I am unsure if this is incompatible with local `Origin` type now
    type FounderOrigin: EnsureOrigin<<Self as Trait<I>>::Origin>;
}

// have a local origin at first for this and then plan how to scale it out
#[derive(PartialEq, Eq, Clone, RuntimeDebug)]
pub enum RawOrigin<AccountId, Shares, I> {
    /// single founder to start
    Founder(AccountId, Shares),
    /// multiple founders upon initialization
    Founders(Vec<(AccountId, Shares)>),
    /// (x, y) s.t. x of the y shares that voted were in approval (<=> y disapproved)
    ShareWeighted(Shares, Shares),
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
        /// The current set of members
        pub Members get(members): Vec<T::AccountId>;
        /// The shares of members
        pub MemberShares get(fn member_shares): map T::AccountId => Shares<T, I>;

        /// The current membership applications
        MemberApplications: Vec<MemberApplication<T::AccountId, BalanceOf<T, I>, T::BlockNumber>>;

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

        fn new_member_application(origin) -> DispatchResult {
            let new_member = ensure_signed(origin)?;
            let members = <Members<T, I>>::get();
            // this error case should add more negative incentives like why would you make us do this storage call `=>` penalty for whatever client causes this path!
            ensure!(!Self::is_member(&members, &new_member), Error::<T, I>::IsAMember);
            Ok(())
        }

        // new_members_application

        // existing_member_changes

        // existing_member_exit

        // grant_application

        // member_vote
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
pub struct EnsureShareWeightMoreThan<Permill, AccountId, Shares, T: Trait<I>, I = DefaultInstance>(
    sp_std::marker::PhantomData<(Permill, AccountId, Shares, T, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, Permill, AccountId, Shares, T: Trait<I>, I> EnsureOrigin<O> for EnsureShareWeightMoreThan<Permill, AccountId, Shares, T, I>
{
    type Success = ();
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
            RawOrigin::ShareWeighted(n, m) => Ok(()),
            r => Err(O::from(r)),
        })
    }
}

pub struct EnsureShareWeightAtLeast<Permill, AccountId, Shares, T: Trait<I>, I = DefaultInstance>(
    sp_std::marker::PhantomData<(Permill, AccountId, Shares, T, I)>,
);

impl<O: Into<Result<RawOrigin<AccountId, Shares, I>, O>> + From<RawOrigin<AccountId, Shares, I>>, Permill, AccountId, Shares, T: Trait<I>, I> EnsureOrigin<O> for EnsureShareWeightAtLeast<Permill, AccountId, Shares, T, I>
{
    type Success = ();
    fn try_origin(o: O) -> Result<Self::Success, O> {
        o.into().and_then(|o| match o {
			// TODO: check actual thresholds - see democracy and issue #
			RawOrigin::ShareWeighted(n, m) => {
				if n >= 
			},
			// this is exhaustive?
            r => Err(O::from(r)),
        })
    }
}

// todo: tests in another file
