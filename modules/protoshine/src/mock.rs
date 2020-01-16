// One Mock Runtime
// - todo: _generate_ more
use super::*;

use frame_support::{assert_noop, assert_ok, impl_outer_origin, parameter_types, weights::Weight};
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};

impl_outer_origin! {
    pub enum Origin for Test where system = frame_system {}
}

#[derive(Clone, Eq, PartialEq)]
pub struct Test;
parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::one();
}
impl frame_system::Trait for Test {
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Call = ();
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = ();
    type BlockHashCount = BlockHashCount;
    type MaximumBlockWeight = MaximumBlockWeight;
    type AvailableBlockRatio = AvailableBlockRatio;
    type MaximumBlockLength = MaximumBlockLength;
    type Version = ();
    type ModuleToIndex = ();
}
parameter_types! {
    pub const ExistentialDeposit: u64 = 1;
    pub const TransferFee: u64 = 0;
    pub const CreationFee: u64 = 0;
}
impl pallet_balances::Trait for Test {
    type Balance = u64;
    type OnNewAccount = ();
    type OnFreeBalanceZero = ();
    type OnReapAccount = System;
    type Event = ();
    type TransferPayment = ();
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type TransferFee = TransferFee;
    type CreationFee = CreationFee;
}
parameter_types! {
    // proposal bond is 5% of amount stake_promised or the minimium (TODO: change this logic ASAP)
    pub const MembershipProposalBond: Permill = Permill::from_percent(5);
    pub const MembershipProposalBondMinimum: u64 = 1;
    pub const BatchPeriod: u64 = 2;
    pub const MaximumShareIssuance: Permill = Permill::from_percent(50);
}
impl Trait for Test {
    type Currency = pallet_balances::Module<Test>;
    // not testing event emission in this runtime or using it?
    type Event = ();
    type MembershipProposalBond = MembershipProposalBond;
    type MembershipProposalBondMinimum = MembershipProposalBondMinimum;
    type MaximumShareIssuance = MaximumShareIssuance;
    type BatchPeriod = BatchPeriod;
}
type System = frame_system::Module<Test>;
type Balances = pallet_balances::Module<Test>;
type Protoshine = Module<Test>;

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
        assert_eq!(Protoshine::bank_balance(&Protoshine::account_id()), 0);
        assert_eq!(Protoshine::membership_application_count(), 0);
    });
}
