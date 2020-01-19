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
        balances: vec![
            (1, 100),
            (2, 82),
            (3, 71),
            (4, 69),
            (5, 69),
            (6, 79),
            (7, 21),
            (8, 72),
        ],
        vesting: vec![],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    // I don't know why they would all get the same shares amount but that's how we're doing it
    // for simplicity (could calculate fair value for each person `=>` could also use range-based negotiations)
    GenesisConfig::<Test> {
        member_buy_in: vec![
            (1, 90, 10),
            (2, 72, 10),
            (3, 61, 10),
            (4, 59, 10),
            (5, 59, 10),
            (6, 69, 10),
            (7, 11, 10),
            (8, 62, 10),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    t.into()
}

#[test]
fn genesis_config_works() {
    new_test_ext().execute_with(|| {
        let mut expected_members: Vec<u64> = Vec::new();
        for i in 1..9 {
            expected_members.push(i);
            // check balance configured as expected
            assert_eq!(Balances::free_balance(i), 10);
            // for now, configure same share profile for all members
            let share_profile = ShareProfile {
                reserved_shares: 0u32.into(),
                total_shares: 10,
            };
            // check if the member share profile matches previously expressed expectations
            assert_eq!(share_profile, Protoshine::membership_shares(&i).unwrap());
        }
        assert_eq!(expected_members, Protoshine::members());
        let default_bank = Protoshine::bank_account().joint_account.inner().unwrap();

        assert_eq!(484, Balances::free_balance(&default_bank));
    });
}

#[test]
fn membership_check_works() {
    new_test_ext().execute_with(|| {
        for i in 1..9 {
            assert!(Protoshine::is_member(&i));
        }
        assert!(!Protoshine::is_member(&0));
        for j in 10..20 {
            assert!(!Protoshine::is_member(&j));
        }
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
