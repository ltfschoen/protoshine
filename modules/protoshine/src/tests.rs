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
        vesting: vec![],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    // I don't know why they would all get the same shares amount but that's how we're doing it
    // for simplicity (could calculate fair value for each person `=>` could also use range-based negotiations)
    GenesisConfig::<Test> {
        member_buy_in: vec![
            (1, 10, 10),
            (2, 10, 10),
            (3, 10, 10),
            (4, 10, 10),
            (5, 10, 10),
            (6, 10, 10),
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
        let mut expected_non_members: Vec<u64> = Vec::new();
        // the first element is padded because vecs are zero indexed but are accounts start at 1
        let expected_balances: Vec<u64> = vec![0, 90, 12, 39, 49, 59, 69, 11, 616, 17, 10];
        // for members
        for i in 1..11 {
            let is_a_member = i < 7;
            if is_a_member {
                expected_members.push(i);
                // for now, configure same share profile for all members
                let share_profile = ShareProfile {
                    reserved_shares: 0u32.into(),
                    total_shares: 10,
                };
                // check if the member share profile matches previously expressed expectations
                assert_eq!(share_profile, Protoshine::membership_shares(&i).unwrap());
            } else {
                expected_non_members.push(i);
            }
            let t = i as usize;
            assert_eq!(
                &Balances::free_balance(i),
                expected_balances.get(t).unwrap()
            );
        }
        assert_eq!(expected_members, Protoshine::members());
        for j in expected_non_members {
            assert!(!Protoshine::is_member(&j));
        }
        let default_bank = Protoshine::bank_account();
        assert_eq!(60, Protoshine::bank_balance(default_bank).unwrap());
    });
}

#[test]
fn membership_check_works() {
    new_test_ext().execute_with(|| {
        for i in 1..7 {
            assert!(Protoshine::is_member(&i));
        }
        assert!(!Protoshine::is_member(&0));
        for j in 7..20 {
            assert!(!Protoshine::is_member(&j));
        }
    });
}

// #[test]
// fn membership_application_works() {
//     new_test_ext(|| {

//     });
// }

// #[test]
// fn bond_calculations() {
//     new_test_ext().execute_with(|| {

//     });
// }

// #[test]
// fn poor_cant_afford_membership_application() {
//     // I name this test intentionally because *crowdfunding* applications that can't afford bonds
//     // is a necessary TODO (the current iteration is classist until this is changed)
//     new_test_ext().execute_with(|| {
//         // apply without enough capital to pay bond

//         // apply

//         // ask them to
//     })
// }

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
