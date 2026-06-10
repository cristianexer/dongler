import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from eval_metrics.order import (
    kendall_tau,
    normalized_kendall_tau,
    reading_order_ard,
    reading_order_score,
    sequence_bleu,
)


# --------------------------------------------------------------------------- #
# reading_order_ard / reading_order_score
# --------------------------------------------------------------------------- #
def test_ard_identical_is_zero():
    assert reading_order_ard([1, 2, 3, 4], [1, 2, 3, 4]) == 0.0


def test_score_identical_is_one():
    assert reading_order_score([1, 2, 3, 4], [1, 2, 3, 4]) == 1.0


def test_ard_reference_swap_value():
    # each element off by 1, n=2 => mean(1, 1) / 2 = 0.5
    assert reading_order_ard([2, 1], [1, 2]) == 0.5


def test_score_is_one_minus_ard():
    pred, gt = [3, 1, 2, 4], [1, 2, 3, 4]
    assert abs(reading_order_score(pred, gt) - (1.0 - reading_order_ard(pred, gt))) < 1e-12


def test_ard_reversed_is_half_for_shared_set():
    # A fully reversed order is the worst case for a shared id set: ARD == 0.5.
    assert reading_order_ard([4, 3, 2, 1], [1, 2, 3, 4]) == 0.5
    assert reading_order_score([4, 3, 2, 1], [1, 2, 3, 4]) == 0.5


def test_ard_within_bounds():
    pred, gt = [5, 3, 1, 4, 2], [1, 2, 3, 4, 5]
    ard = reading_order_ard(pred, gt)
    assert 0.0 <= ard <= 1.0
    assert 0.0 <= reading_order_score(pred, gt) <= 1.0


def test_ard_restricts_to_intersection():
    # Common ids are {2, 3}; indices come from the full lists.
    # pred indices: 2->1, 3->2 ; gt indices: 2->0, 3->1 ; mean(|1|,|1|)/2 = 0.5
    assert reading_order_ard([1, 2, 3], [2, 3, 4]) == 0.5


def test_ard_disjoint_is_worst():
    assert reading_order_ard([1, 2], [3, 4]) == 1.0
    assert reading_order_score([1, 2], [3, 4]) == 0.0


def test_ard_single_common_element():
    assert reading_order_ard([9], [9]) == 0.0
    assert reading_order_score([9], [9]) == 1.0


# --------------------------------------------------------------------------- #
# sequence_bleu
# --------------------------------------------------------------------------- #
def test_bleu_identical_is_one():
    assert sequence_bleu([1, 2, 3, 4, 5], [1, 2, 3, 4, 5]) == 1.0


def test_bleu_reversed_in_unit_interval():
    value = sequence_bleu([5, 4, 3, 2, 1], [1, 2, 3, 4, 5])
    assert 0.0 <= value < 1.0


def test_bleu_partial_match_between_zero_and_one():
    value = sequence_bleu([1, 2, 3, 5, 4], [1, 2, 3, 4, 5])
    assert 0.0 < value < 1.0


def test_bleu_both_empty_is_one():
    assert sequence_bleu([], []) == 1.0


def test_bleu_one_empty_is_zero():
    assert sequence_bleu([1, 2, 3], []) == 0.0
    assert sequence_bleu([], [1, 2, 3]) == 0.0


def test_bleu_short_sequence_clamps_order():
    # Hypothesis shorter than max_n must still yield 1.0 when identical.
    assert sequence_bleu([1, 2], [1, 2], max_n=4) == 1.0
    assert sequence_bleu([7], [7], max_n=4) == 1.0


def test_bleu_brevity_penalty_shrinks_short_hypothesis():
    full = sequence_bleu([1, 2, 3, 4], [1, 2, 3, 4])
    short = sequence_bleu([1, 2], [1, 2, 3, 4])
    assert short < full == 1.0
    assert 0.0 < short < 1.0


def test_bleu_disjoint_tokens_low():
    value = sequence_bleu([10, 11, 12], [1, 2, 3])
    assert 0.0 <= value < 1.0


# --------------------------------------------------------------------------- #
# kendall_tau / normalized_kendall_tau
# --------------------------------------------------------------------------- #
def test_kendall_identical_is_one():
    assert kendall_tau([1, 2, 3, 4], [1, 2, 3, 4]) == 1.0


def test_kendall_reversed_is_minus_one():
    assert kendall_tau([4, 3, 2, 1], [1, 2, 3, 4]) == -1.0


def test_kendall_within_bounds():
    for tau in (
        kendall_tau([1, 2, 3, 4], [1, 2, 3, 4]),
        kendall_tau([4, 3, 2, 1], [1, 2, 3, 4]),
        kendall_tau([2, 1, 4, 3], [1, 2, 3, 4]),
        kendall_tau([3, 1, 2, 5, 4], [1, 2, 3, 4, 5]),
    ):
        assert -1.0 <= tau <= 1.0


def test_kendall_symmetry():
    pred, gt = [3, 1, 4, 2], [1, 2, 3, 4]
    assert abs(kendall_tau(pred, gt) - kendall_tau(gt, pred)) < 1e-12


def test_kendall_single_swap_value():
    # One discordant pair out of six => (5 - 1) / 6.
    assert abs(kendall_tau([2, 1, 3, 4], [1, 2, 3, 4]) - (4.0 / 6.0)) < 1e-12


def test_kendall_restricts_to_intersection():
    # Common ids {1, 2, 3} appear in the same relative order in both => 1.0.
    assert kendall_tau([1, 2, 3, 9], [1, 2, 3, 8]) == 1.0


def test_kendall_degenerate_few_common():
    assert kendall_tau([], []) == 0.0
    assert kendall_tau([1], [1]) == 0.0
    assert kendall_tau([1, 2], [3, 4]) == 0.0


def test_normalized_kendall_bounds_and_identity():
    assert abs(normalized_kendall_tau([1, 2, 3, 4], [1, 2, 3, 4]) - 1.0) < 1e-9
    assert abs(normalized_kendall_tau([4, 3, 2, 1], [1, 2, 3, 4]) - 0.0) < 1e-9
    for pred in ([1, 2, 3, 4], [4, 3, 2, 1], [2, 4, 1, 3]):
        value = normalized_kendall_tau(pred, [1, 2, 3, 4])
        assert 0.0 <= value <= 1.0


def test_normalized_kendall_degenerate_is_half():
    assert abs(normalized_kendall_tau([1, 2], [3, 4]) - 0.5) < 1e-12
