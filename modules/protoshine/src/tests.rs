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
    // GenesisConfig::default()
    //     .assimilate_storage::<Test>(&mut t)
    //     .unwrap();
    t.into()
}

#[test]
fn genesis_config_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(Balances::free_balance(&0), 100);
        assert_eq!(Balances::free_balance(&1), 98);
        assert_eq!(Balances::free_balance(&2), 1);
        assert_eq!(Protoshine::membership_application_count(), 0);
    });
}

// make a genesis config for the bank

// #[test]
// fn membership_app_panics_as_expected() {
//     new_test_ext().execute_with(|| {

//     });
// }

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
