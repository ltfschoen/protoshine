# modules

## protoshine
* minimal implementation with a lot of unnecessary boilerplate but as much functionality as can fit cleanly in a single module

The state transitions to be implemented for this module are noted here:

```rust
decl_event! {
    /// Rejected by whatever voting algorithm is used; the request was for `Balance` staked in exchange for `Shares` issued
    MembershipApplicationRejected(ProposalIndex, Balance, Shares),
    /// Proposal passed and there is a scheduled change such that the Bank's Account Balance 
    MembershipApplicationPassed(ProposalIndex, Balance, Shares),
    /// Proposal absorbed and the Bank's Account Balance is `Balance` with a total of `Shares` issued (dilution amount changed)
    MembershipApplicationAbsorbed(ProposalIndex, Balance, Shares, BlockNumber),
    /// A member left the DAO with `Balance` and burned `Shares`
    MembershipExit(AccountId, Balance, Shares),
    GrantApplicationProposal(),
    GrantApplicationSponsored(),
    GrantApplicationRejected(),
    GrantApplicationPassed(),
    GrantApplicationSpend(),
    MetaProposal(),
    MetaProposalRejected(),
    MetaProposalPassed(),
    /// Should be removed after sufficient testing
    EmergencyReset(BlockNumber),
}
```

## signal
* like util, just storing some functional abstractions for gradual abstraction...