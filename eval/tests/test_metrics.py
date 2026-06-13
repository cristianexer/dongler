"""Metric import + sanity tests (PRD §3.2 metric family)."""
from __future__ import annotations

from eval.harness.coverage import spurious_token_rate, token_coverage
from eval.metrics.table import teds
from eval.metrics.text import (
    bleu,
    char_error_rate,
    edit_similarity,
    word_error_rate,
)


def test_identical_strings_score_perfect():
    text = "The quick brown fox jumps over the lazy dog."
    assert edit_similarity(text, text) == 1.0
    assert char_error_rate(text, text) == 0.0
    assert word_error_rate(text, text) == 0.0
    assert bleu(text, text) == 1.0


def test_token_coverage_and_spurious_on_identical():
    text = "alpha beta gamma delta"
    assert token_coverage(text, text) == 1.0
    assert spurious_token_rate(text, text) == 0.0


def test_token_coverage_partial():
    gt = "alpha beta gamma delta"
    pred = "alpha beta"  # half the tokens present
    assert token_coverage(gt, pred) == 0.5
    assert spurious_token_rate(gt, pred) == 0.0  # nothing invented


def test_spurious_token_rate_detects_invented_tokens():
    gt = "alpha beta"
    pred = "alpha beta gamma delta"  # two extra tokens out of four
    assert token_coverage(gt, pred) == 1.0
    assert spurious_token_rate(gt, pred) == 0.5


def test_coverage_multiset_semantics():
    # duplicate token must appear twice to be fully covered
    assert token_coverage("a a b", "a b") == 2 / 3


def test_empty_edge_cases():
    assert token_coverage("", "anything") == 1.0
    assert spurious_token_rate("anything", "") == 0.0


def test_teds_identical_table():
    html = "<table><tr><td>a</td><td>b</td></tr></table>"
    assert teds(html, html) == 1.0
