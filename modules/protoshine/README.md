## spec

```rust
// on-chain state
decl_storage! {
    members;
    membership_shares;
    bank_account;
    votes;
    recipients;
}

// logic to govern on-chain state
decl_module! {
    // for applicants to apply for grants
    fn membership_application();

    // for members to escalate a proposal to voting
    fn sponsor_membership_application();

    // only for members
    fn vote_on_membership();

    // leave membership by selling shares for proportional capital
    fn leave_membership();
}
```

**TODO**
- scheduled execution of membership proposals (change membership set with issuance)
- burn method (with lock-in voting restrictions)
- grant proposal flow (create generic trait from existing code \forall *these* modules) 
- meta proposal flow ("")
- emergency reset mechanism