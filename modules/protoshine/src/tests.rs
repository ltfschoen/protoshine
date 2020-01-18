// testing
use super::*;
use mock::*;

use frame_support::{assert_noop, assert_ok};

fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        // Total issuance will be 200 with treasury account initialized at ED.
        balances: vec![(0, 100), (1, 98), (2, 1)],
        vesting: vec![],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    GenesisConfig::default()
        .assimilate_storage::<Test>(&mut t)
        .unwrap();
    t.into()
}

#[test]
fn genesis_config_works() {
    new_test_ext().execute_with(|| {
        // would need to instantiate the bank
        // then pass that in
        // assert_eq!(Protoshine::bank_balance(&Protoshine::account_id()), 0);
        assert_eq!(Protoshine::membership_application_count(), 0);
    });
}

// add to tests proper
// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn should_work() {
//         assert_eq!(
//             VoteThreshold::SuperMajorityApprove.approved(60, 50, 110, 210),
//             false
//         );
//         assert_eq!(
//             VoteThreshold::SuperMajorityApprove.approved(100, 50, 150, 210),
//             true
//         );
//     }
// }
