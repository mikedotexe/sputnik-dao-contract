#![allow(clippy::ref_in_deref)]
#![allow(clippy::identity_op)]

use near_sdk::json_types::U128;
use near_sdk::AccountId;
use near_sdk_sim::{call, to_yocto, view};
use near_sdk_sim::{ContractAccount, UserAccount};

use crate::utils::*;
use sputnikdao2::ContractContract;
use sputnikdao2::{Bounty, Proposal, ProposalInput, ProposalKind, ProposalStatus};

mod utils;

const NANO_TO_MICRO: u64 = 1000;
const MICRO_TO_MILLI: u64 = 1000;
const MILLI_TO_SEC: u64 = 1000;
const NANO_TO_SEC: u64 = NANO_TO_MICRO * MICRO_TO_MILLI * MILLI_TO_SEC;
const SEC_TO_MINUTE: u64 = 60;
const MINUTE_TO_HOUR: u64 = 60;

/// = thousand = 10^3 = 1000
const KILO: u128 = 1000;
/// = million = 10^6 = [`KILO`]^2
const MEGA: u128 = KILO * KILO;
/// = 10^24 = [`MEGA`]^4
const YOTTA: u128 = MEGA * MEGA * MEGA * MEGA;

fn user(id: u32) -> AccountId {
    format!("user{}", id).parse().unwrap()
}

/// Creates an add-bounty proposal and approves it.
///
/// Returns the bounty id.
fn new_bounty(
    root: &UserAccount,
    dao: &ContractAccount<ContractContract>,
    amount: u128,
    times: u32,
    max_deadline: u64,
) -> u64 {
    let bounty = Bounty {
        description: "my bounty".to_string(),
        token: None,
        amount: U128::from(amount),
        times,
        max_deadline: U64::from(max_deadline),
    };
    let add_bounty = ProposalKind::AddBounty {
        bounty: bounty.clone(),
    };
    let input = ProposalInput {
        description: "bounty proposal".to_string(),
        kind: add_bounty,
    };

    let res = call!(root, dao.add_proposal(input), deposit = to_yocto("1"));
    res.assert_success();
    let add_bounty_id = res.unwrap_json::<u64>();

    // approves
    vote(vec![root], dao, add_bounty_id);

    // check approval
    let proposal = view!(dao.get_proposal(add_bounty_id)).unwrap_json::<Proposal>();
    assert_eq!(proposal.status, ProposalStatus::Approved);

    // gets the bounty-id
    let res = view!(dao.get_last_bounty_id());
    let bounty_id = res.unwrap_json::<u64>() - 1;

    // check that the bounty stored is the same
    // as intended
    use sputnikdao2::views::BountyOutput;
    let res = view!(dao.get_bounty(bounty_id));
    let bounty_output = res.unwrap_json::<BountyOutput>();
    assert_eq!(
        bounty_output,
        BountyOutput {
            id: bounty_id,
            bounty
        }
    );

    bounty_id
}

#[test]
fn test_bounty_general() {
    let (root, dao) = setup_dao();
    let user2 = root.create_user(user(2), to_yocto("1000"));
    let user3 = root.create_user(user(3), to_yocto("1000"));

    let bounty = new_bounty(
        &root,
        &dao,
        1u128,
        1,
        1 * NANO_TO_SEC * SEC_TO_MINUTE * MINUTE_TO_HOUR,
    );

    // fail: user2 claims the wrong bounty
    let res = call!(
        &user2,
        dao.bounty_claim(
            // non-existing bounty
            bounty + 1,
            U64::from(1 * NANO_TO_SEC * SEC_TO_MINUTE)
        )
    );
    should_fail_with(res, 0, "ERR_NO_BOUNTY");

    // fail: user2 claims with not enought bond
    let res = call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(1 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA - 1
    );
    should_fail_with(res, 0, "ERR_BOUNTY_WRONG_BOND");

    // fail: user2 claims with the wrong deadline
    let res = call!(
        &user2,
        dao.bounty_claim(
            bounty,
            // wrong deadline
            U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE * MINUTE_TO_HOUR)
        ),
        deposit = 1 * YOTTA
    );
    should_fail_with(res, 0, "ERR_BOUNTY_WRONG_DEADLINE");

    // ok: user2 claims the bounty
    call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    )
    .assert_success();

    // fail: user2 tries to claim again
    let res = call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    );
    should_fail_with(res, 0, "ERR_BOUNTY_ALL_CLAIMED");

    // fail: user2 gives up the wrong bounty
    let res = call!(
        &user2,
        dao.bounty_giveup(
            // non-existing bounty
            bounty + 1
        )
    );
    should_fail_with(res, 0, "ERR_NO_BOUNTY_CLAIM");

    // ok: user2 gives up
    call!(&user2, dao.bounty_giveup(bounty)).assert_success();

    // fail: user2 gives up again
    let res = call!(&user2, dao.bounty_giveup(bounty));
    should_fail_with(res, 0, "ERR_NO_BOUNTY_CLAIMS");

    // ok: user2 re-claims the bounty
    call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    )
    .assert_success();

    // fail: user2 tries to finish the wrong bounty
    let res = call!(
        &user2,
        dao.bounty_done(
            // non-existing bounty
            bounty + 1,
            None,
            "".to_string()
        )
    );
    should_fail_with(res, 0, "ERR_NO_BOUNTY_CLAIM");

    // fail: user3 tries to finish the bounty
    let res = call!(
        // wrong user
        &user3,
        dao.bounty_done(bounty, None, "".to_string())
    );
    should_fail_with(res, 0, "ERR_NO_BOUNTY_CLAIMS");

    // fail: user3 tries to finish the bounty as user2
    let res = call!(
        &user3,
        dao.bounty_done(bounty, Some(user2.account_id()), "".to_string())
    );
    should_fail_with(res, 0, "ERR_BOUNTY_DONE_MUST_BE_SELF");

    // fail: user2 tries to finish the bounty without attaching bonds
    // see issue #36:
    // https://github.com/near-daos/sputnik-dao-contract/issues/36
    let res = call!(&user2, dao.bounty_done(bounty, None, "".to_string()));
    should_fail_with(res, 0, "ERR_MIN_BOND");

    // ok: user2 finishes the bounty
    let res = call!(
        &user2,
        dao.bounty_done(bounty, None, "".to_string()),
        deposit = 1 * YOTTA
    );
    res.assert_success();
    let bounty_done1 = res.unwrap_json::<Option<u64>>().unwrap();

    // fail: user2 tries to finish the bounty again
    let res = call!(&user2, dao.bounty_done(bounty, None, "".to_string()));
    should_fail_with(
        res,
        0,
        // there are no non-completed bounty claims available
        "ERR_NO_BOUNTY_CLAIM",
    );

    // approve and withdraw
    assert!(view!(dao.get_bounty(bounty)).is_ok());
    let user2amount = user2.account().unwrap().amount;
    vote(vec![&root], &dao, bounty_done1);
    let proposal = view!(dao.get_proposal(bounty_done1)).unwrap_json::<Proposal>();
    assert_eq!(proposal.status, ProposalStatus::Approved);
    assert_eq!(
        user2amount
        // gets back the bounty-done proposal bond
       + 1 * YOTTA
       // the actual bounty
       + 1,
        user2.account().unwrap().amount
    );

    let res = view!(dao.get_bounty(bounty));
    view_should_fail_with(res, "ERR_NO_BOUNTY");
}

#[test]
fn test_bounty_general2() {
    let (root, dao) = setup_dao();
    let user2 = root.create_user(user(2), to_yocto("1000"));

    let bounty = new_bounty(
        &root,
        &dao,
        1u128,
        // this bounty can be claimed twice
        2,
        1 * NANO_TO_SEC * SEC_TO_MINUTE * MINUTE_TO_HOUR,
    );

    // ok: user2 claims the bounty
    call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    )
    .assert_success();

    // ok: user2 claims the bounty for the second time
    call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    )
    .assert_success();

    // fail: user2 tries to claim for the third time
    let res = call!(
        &user2,
        dao.bounty_claim(bounty, U64::from(2 * NANO_TO_SEC * SEC_TO_MINUTE)),
        deposit = 1 * YOTTA
    );
    should_fail_with(res, 0, "ERR_BOUNTY_ALL_CLAIMED");

    // ok: user2 finishes the bounty
    let res = call!(
        &user2,
        dao.bounty_done(bounty, None, "".to_string()),
        deposit = 1 * YOTTA
    );
    res.assert_success();
    let bounty_done1 = res.unwrap_json::<Option<u64>>().unwrap();

    // ok: user2 finishes the bounty again (second claim)
    let res = call!(
        &user2,
        dao.bounty_done(bounty, None, "".to_string()),
        deposit = 1 * YOTTA
    );
    res.assert_success();
    let bounty_done2 = res.unwrap_json::<Option<u64>>().unwrap();

    // fail: user2 tries to finish the bounty for the third time
    let res = call!(
        &user2,
        dao.bounty_done(bounty, None, "".to_string()),
        deposit = 1 * YOTTA
    );
    should_fail_with(res, 0, "ERR_NO_BOUNTY_CLAIM");

    // approve and withdraw
    assert!(view!(dao.get_bounty(bounty)).is_ok());
    let user2amount = user2.account().unwrap().amount;
    vote(vec![&root], &dao, bounty_done1);
    let proposal = view!(dao.get_proposal(bounty_done1)).unwrap_json::<Proposal>();
    assert_eq!(proposal.status, ProposalStatus::Approved);
    assert_eq!(
        user2amount
            // gets back the bounty-done proposal bond
           + 1 * YOTTA
           // the actual bounty
           + 1,
        user2.account().unwrap().amount
    );

    // approve and withdraw
    assert!(view!(dao.get_bounty(bounty)).is_ok());
    let user2amount = user2.account().unwrap().amount;
    vote(vec![&root], &dao, bounty_done2);
    let proposal = view!(dao.get_proposal(bounty_done2)).unwrap_json::<Proposal>();
    assert_eq!(proposal.status, ProposalStatus::Approved);
    assert_eq!(
        user2amount
             // gets back the bounty-done proposal bond
            + 1 * YOTTA
            // the actual bounty
            + 1,
        user2.account().unwrap().amount
    );

    let res = view!(dao.get_bounty(bounty));
    view_should_fail_with(res, "ERR_NO_BOUNTY");

    // TODO: check that the claims were removed properly
    // let res = view!(dao.get_bounty_claims(user2.account_id()));
    // let claims = res.unwrap_json::<Vec<sputnikdao2::BountyClaim>>();
    // panic!("{:#?}", &claims);
}

// ---

// TODO: test expired "done" bounty
// user cancelling itself,
// another user cancelling it

// ---
