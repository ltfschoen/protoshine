// Moloch Module
#![cfg_attr(not(feature = "std"), no_std)]

mod origins;
mod util;
use util::Signal;

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
use frame_support::traits::{
	Currency, ReservableCurrency, Get, ChangeMembers,
};
use frame_system::{self as system, ensure_signed, ensure_root};

type Shares<T, I> = <<T as Trait<I>>::Signal as Signal<<T as system::Trait>::AccountId>>::Shares;
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
pub trait Trait<I=DefaultInstance>: system::Trait {
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
    
    // TODO: add membership origin(s)

	/// The origin that is allowed to call `found`.
	type FounderOrigin: EnsureOrigin<Self::Origin>;
}

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
	pub struct Module<T: Trait<I>, I: Instance=DefaultInstance> for enum Call where origin: T::Origin {
		type Error = Error<T, I>;

		// Used for handling module events.
		fn deposit_event() = default;

		fn example_runtime_method(origin) -> DispatchResult {
			let voter = ensure_signed(origin)?;
			ensure!(Self::is_member(&voter), Error::<T, I>::NotAMember);

			Self::deposit_event(RawEvent::Example(voter, 26.into(), 32.into()));
			Ok(())
		}
	}
}

impl<T: Trait<I>, I: Instance> Module<T, I> {
	pub fn is_member(who: &T::AccountId) -> bool {
		Self::members().contains(who)
	}
}

// tests here or another file