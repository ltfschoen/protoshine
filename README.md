# protoshine

a **[protostar](https://en.wikipedia.org/wiki/Protostar)** is a very young star that is still gathering mass from its parent molecular cloud

This is a prototype of the sunshine protocol for a single organization. Thoughts for scaling this design with a different storage configuration are haphazardly organized in the issues.

## user flow

The balances of 13 new accounts are initialized using `pallet_balances::GenesisConfig`. For each `(u64, u64)` tuple, the first element represents the `AccountId` and the second element represents the amount of `Currency` minted.

In `modules/protoshine/src/test.rs` in `new_test_ext()`,

```rust
balances: vec![
    // members
    (1, 100),
    (2, 22),
    (3, 49),
    (4, 59),
    (5, 69),
    (6, 79),
    // non-members
    (7, 11),
    (8, 616),
    (9, 17),
    (10, 10),
],
```

> TODO: in the future, we want to mock arbitrary initialization configurations instead of testing a specific user flow.

Note that comments delineate between members and non-members. Upon initialization, only the accounts 1-6 (inclusive) are members (6 initial members). This membership initialization is enforced as share issuance based on the genesis config in this module's `decl_storage` block.

```rust
config(member_buy_in): Vec<(T::AccountId, BalanceOf<T>, Shares)>;
```

The fields of this configuration item specify the associated `AccountId`, the amount of `Currency` committed by this `AccountId` and the number of shares issued to this `AccountId` in the initialization of the on-chain organization.

> TODO: we need to enforce the variant that the amount committed is less than the configured balance (would be easier if the balances type was native to this module; start on a separate `balances` type that is voted on by members for acceptance according to some exchange rate mechanism)

In the same `new_test_ext()` method in `protoshine/modules/src/test`

```rust
GenesisConfig::<Test> {
    member_buy_in: vec![
        // (AccountId, Stake_Committed, Shares_Requested)
        (1, 10, 10),
        (2, 10, 10),
        (3, 10, 10),
        (4, 10, 10),
        (5, 10, 10),
        (6, 10, 10),
    ],
}
```

The organization is initialized with 6 new members. Each member commits 10 `Currency`. I wanted to keep it simple so I just set it so that every member exchanges 10 `Currency` for 10 shares at initialization. This means that every member is also initialized as part of the organization with 10 `Share`s worth of voting power.

Next, we will discuss each runtime method and isolate each necessary step in the method body's logic. There are a few types of runtime methods. A taxonomy based on access permissions would distinguish between methods available for non-members and methods that can only be called by members of the organization.

*non-member actions*
* [apply](#apply)

*member actions only*
* [sponsor](#sponsor)
* [vote](#vote)
* [leave](#leave)

> if curious about patterns for adding permissions, check out the [recipe](https://substrate.dev/recipes/declarative/permissioned.html)

### apply

The method header reveals the inputs.

```rust
fn membership_application(
    origin,
    stake_promised: BalanceOf<T>,
    shares_requested: Shares,
) -> DispatchResult
```

The basic structure resembles a request to transfer `stake_promised` amount of capital in denomination of `BalanceOf<T>` in exchange for issuance of `shares_requested` amount of `Shares`, which serve as the internal *unit of account* for use within the organization.

1. a few checks are done to filter unrealistic membership applications

```rust
let shares_as_balance = BalanceOf::<T>::from(shares_requested);
ensure!(
    stake_promised > T::Currency::minimum_balance(),
    Error::<T>::InvalidMembershipApplication,
);
```

2. the application bond is calculated inside another runtime method; it's inputs include the parameters with which the applicant called the method

```rust
let collateral = Self::calculate_member_application_bond(
    stake_promised,
    shares_requested,
)?;
```

3. the application bond is reserved from the applicant

```rust
T::Currency::reserve(&applicant, collateral)
.map_err(|_| Error::<T>::InsufficientMembershipApplicantCollateral)?;
```

> TODO: when is this bond unbonded? needs to be some sort of obligation that is defacto dropped when the application is handled `=>` LockIdentifier might want to be used instead

4. The membership application is added to the `MembershipApplications` storage item

```rust
<MembershipApplications<T>>::insert(c, membership_app);
```

For context, all storage items are in the `decl_storage` block,

```rust
pub MembershipApplications get(fn membership_applications):
map ProposalIndex => Option<MembershipProposal<T::AccountId, BalanceOf<T>, T::BlockNumber>>;
```

### sponsor

### vote 

### leave
