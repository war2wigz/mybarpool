//! The winning box, PROGRAM §6.3, as properties over random axis pairs:
//! every one of the 100 digit pairs hits exactly one box and every box is
//! hit by exactly four pairs; higher scores reduce mod 10; a digit missing
//! from an axis is an error, never a panic. The vector cross-checks are in
//! `vectors.rs`.

use mybarpool::winner::{lane_of, winning_box};

/// A deterministic permutation of 0–9 from a seed byte (a Fisher–Yates over a tiny LCG).
fn permutation(seed: u8) -> [u8; 10] {
    let mut a = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let mut state = u32::from(seed)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(12_345);
    for i in (1..10).rev() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let j = (state >> 16) as usize % (i + 1);
        a.swap(i, j);
    }
    a
}

#[test]
fn every_digit_pair_hits_exactly_one_box_and_every_box_exactly_four_pairs() {
    // PROGRAM §6.3 "Exactly one box matches for any pair of scores": 100 (hd, ad) pairs over
    // 25 boxes, four per box (2 home digits × 2 away digits per lane pair).
    for seed in 0..10u8 {
        let home_axis = permutation(seed);
        let away_axis = permutation(seed.wrapping_add(100));
        let mut hits = [0u8; 25];
        for hd in 0..10u16 {
            for ad in 0..10u16 {
                let b = winning_box(hd, ad, &home_axis, &away_axis).unwrap();
                assert!(b < 25);
                hits[usize::from(b)] += 1;
                // Higher scores with the same last digit land on the same box, 65,535 included.
                for (k, m) in [(1u16, 2u16), (7, 0), (123, 45)] {
                    assert_eq!(
                        winning_box(hd + 10 * k, ad + 10 * m, &home_axis, &away_axis).unwrap(),
                        b
                    );
                }
                if hd == 5 && ad == 5 {
                    assert_eq!(
                        winning_box(65_535, 65_535, &home_axis, &away_axis).unwrap(),
                        b
                    );
                }
            }
        }
        assert_eq!(hits, [4u8; 25], "seed {seed}");
    }
}

#[test]
fn identity_axes_give_the_briefs_worked_boxes() {
    // Lane l holds digits l and l + 5, so digit d is in lane d mod 5: 7–3 → col 2, row 3, box
    // 17; 14–10 → col 4, row 0, box 4; 17–17 → 12; 24–20 → 4 (the Step 3 fixture scores).
    let id = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    assert_eq!(winning_box(7, 3, &id, &id).unwrap(), 17);
    assert_eq!(winning_box(14, 10, &id, &id).unwrap(), 4);
    assert_eq!(winning_box(17, 17, &id, &id).unwrap(), 12);
    assert_eq!(winning_box(24, 20, &id, &id).unwrap(), 4);
    // Columns are the home team: swapping the scores swaps row and column.
    assert_eq!(winning_box(3, 7, &id, &id).unwrap(), 2 * 5 + 3);
}

#[test]
fn lane_of_on_a_non_permutation_is_an_error_not_a_panic() {
    // The standing rule: no unwrap on account data. Digit 9 missing (two 0s).
    let broken = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 0];
    assert_eq!(lane_of(&broken, 0).unwrap(), 0);
    let err = lane_of(&broken, 9).unwrap_err();
    assert_eq!(format!("{err:?}").contains("MathOverflow"), true, "{err:?}");
    assert!(winning_box(9, 0, &broken, &broken).is_err());
    assert!(winning_box(0, 9, &broken, &broken).is_err());
}
