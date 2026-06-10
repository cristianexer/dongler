import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from eval_metrics.text import (
    bleu,
    bleu_tokens,
    char_error_rate,
    edit_similarity,
    normalized_edit_distance,
    word_error_rate,
)


# ---------------------------------------------------------------------------
# char_error_rate
# ---------------------------------------------------------------------------
def test_cer_reference_values():
    assert char_error_rate("kitten", "sitting") == 0.5
    assert char_error_rate("abc", "abc") == 0.0


def test_cer_empty_reference():
    assert char_error_rate("", "") == 0.0
    assert char_error_rate("", "x") == 1.0


def test_cer_identical_is_zero():
    assert char_error_rate("hello world", "hello world") == 0.0


def test_cer_disjoint_is_one():
    # Every character substituted -> distance == reference length.
    assert char_error_rate("abc", "xyz") == 1.0


def test_cer_can_exceed_one_when_hypothesis_longer():
    # Reference of length 1, hypothesis much longer -> raw ratio > 1, no clamp.
    assert char_error_rate("a", "abcdef") > 1.0


def test_cer_non_negative():
    assert char_error_rate("dongler", "donglr") >= 0.0


# ---------------------------------------------------------------------------
# word_error_rate
# ---------------------------------------------------------------------------
def test_wer_reference_values():
    assert abs(word_error_rate("the cat sat", "the dog sat") - 1 / 3) < 1e-9
    assert word_error_rate("a b c", "a b c") == 0.0


def test_wer_empty_reference():
    assert word_error_rate("", "") == 0.0
    assert word_error_rate("   ", "") == 0.0  # whitespace-only -> no words
    assert word_error_rate("", "hello") == 1.0


def test_wer_disjoint_is_one():
    assert word_error_rate("a b c", "x y z") == 1.0


def test_wer_handles_extra_whitespace():
    # Multiple spaces collapse during split(); identical word lists -> 0.
    assert word_error_rate("the   cat", "the cat") == 0.0


def test_wer_non_negative():
    assert word_error_rate("one two three four", "one two") >= 0.0


# ---------------------------------------------------------------------------
# edit_similarity / normalized_edit_distance
# ---------------------------------------------------------------------------
def test_edit_similarity_reference_values():
    assert edit_similarity("abc", "abc") == 1.0
    assert 0.0 <= edit_similarity("abc", "xyz") < 1.0


def test_edit_similarity_bounds():
    for ref, hyp in [("", ""), ("abc", ""), ("", "abc"), ("hello", "h3llo")]:
        score = edit_similarity(ref, hyp)
        assert 0.0 <= score <= 1.0


def test_edit_similarity_empty_both_is_one():
    assert edit_similarity("", "") == 1.0


def test_edit_similarity_symmetry():
    assert edit_similarity("abcd", "abxd") == edit_similarity("abxd", "abcd")


def test_normalized_edit_distance_reference_value():
    assert abs(normalized_edit_distance("abc", "abc")) < 1e-9


def test_normalized_edit_distance_is_complement():
    for ref, hyp in [("kitten", "sitting"), ("dongler", "dongle"), ("", "x")]:
        assert abs(
            normalized_edit_distance(ref, hyp) + edit_similarity(ref, hyp) - 1.0
        ) < 1e-9


def test_normalized_edit_distance_bounds():
    assert 0.0 <= normalized_edit_distance("abc", "xyz") <= 1.0


# ---------------------------------------------------------------------------
# bleu
# ---------------------------------------------------------------------------
def test_bleu_identical_is_one():
    assert bleu("the quick brown fox jumps", "the quick brown fox jumps") == 1.0


def test_bleu_disjoint_is_small():
    score = bleu("the quick brown fox jumps", "a completely different unrelated sentence")
    assert score < 0.1
    assert 0.0 <= score <= 1.0


def test_bleu_bounds():
    cases = [
        ("hello world foo bar baz", "hello world foo bar baz"),
        ("hello world", "world hello"),
        ("a b c d e", "a b c d e f g"),
        ("a b c", "x y z"),
        ("the cat sat on the mat", "the cat sat"),
    ]
    for ref, hyp in cases:
        score = bleu(ref, hyp)
        assert 0.0 <= score <= 1.0


def test_bleu_partial_match_between_extremes():
    score = bleu("the quick brown fox jumps", "the quick brown dog runs")
    assert 0.1 < score < 1.0


def test_bleu_short_identical_is_one():
    # Sequence shorter than max_n must still reach exactly 1.0 when identical.
    assert bleu("a b", "a b") == 1.0
    assert bleu("solo", "solo") == 1.0


def test_bleu_brevity_penalty_reduces_score():
    # A too-short but fully-contained hypothesis is penalized below 1.0.
    score = bleu("the quick brown fox jumps high", "the quick brown")
    assert 0.0 <= score < 1.0


def test_bleu_empty_cases():
    assert bleu("", "") == 1.0
    assert bleu("hello", "") == 0.0
    assert bleu("", "hello") == 0.0


def test_bleu_tokens_matches_bleu():
    ref = "the quick brown fox"
    hyp = "the quick brown dog"
    assert bleu_tokens(ref.split(), hyp.split()) == bleu(ref, hyp)


def test_bleu_tokens_identical_is_one():
    assert bleu_tokens(["x", "y", "z", "w", "v"], ["x", "y", "z", "w", "v"]) == 1.0
