# SPEC

## lib.rs

```rust
// on-chain state
decl_storage! {
    proposals;
    votes;
    members;
    recipients;
}

// logic to govern on-chain state
decl_module! {
    // for applicants to apply for grants
    fn propose();

    // for members to escalate a proposal to voting
    fn sponsor_proposal();

    // only for members
    fn vote_on_proposal();

    // only for recipients
    fn vote_on_fund_allocation();
}

// events to emit state transitions
decl_events! {
    ProposalProposed,
    ProposalSponsored,
    ProposalPassed,
    MemberJoined,
    MemberLeft,
}
```