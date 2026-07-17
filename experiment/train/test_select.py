"""Tests for the admission-set guards. Runs with plain python3 -- no torch, no
GPU -- because these guards are the load-bearing anti-collapse logic and must be
verifiable in the build environment.

    python3 -m pytest experiment/train/test_select.py     # if pytest present
    python3 experiment/train/test_select.py               # standalone fallback
"""

from __future__ import annotations

from admission import (
    Reward,
    Rollout,
    build_admission_set,
    extract_source,
    parse_rewards,
)


def test_extract_unwraps_a_fence_but_never_repairs() -> None:
    assert extract_source("```stratlite\nlookback 4;\nsignal long;\n```") == "lookback 4;\nsignal long;\n"
    assert extract_source("```\nsignal flat;\n```") == "signal flat;\n"
    # No fence: passed through verbatim, for the oracle to judge as-is.
    assert extract_source("lookback 8; signal long;") == "lookback 8; signal long;"
    # Prose with no closing fence is not a program; it goes through untouched
    # and earns its compile-zero honestly.
    assert extract_source("Sure, here you go:") == "Sure, here you go:"


def _rewards(*triples: tuple[str, str, str]) -> dict[str, Reward]:
    # (id, class, nkey) -> keyed rewards; fuel is irrelevant to admission.
    return {i: Reward(id=i, cls=c, nkey=k, fuel=10) for (i, c, k) in triples}


def test_only_ok_rung_is_admitted() -> None:
    # The 2/3 gate rung is a farmable local optimum -- it must NOT be admitted.
    rollouts = [Rollout(i, "trend", f"src{i}") for i in ("a", "b", "c", "d")]
    rewards = _rewards(
        ("a", "ok", "k1"),
        ("b", "gate", "k2"),
        ("c", "run", "k3"),
        ("d", "compile", "k4"),
    )
    admitted, stats = build_admission_set(rollouts, rewards)
    assert [r.id for r in admitted] == ["a"]
    # The full histogram is still logged -- it is the learning curve.
    assert dict(stats.histogram) == {"ok": 1, "gate": 1, "run": 1, "compile": 1}


def test_dedup_caps_a_template_family() -> None:
    # Four ok-rung copies of ONE program (same nkey) -- mode collapse. The cap
    # admits at most `per_key_cap` of them.
    rollouts = [Rollout(i, "trend", "same") for i in ("a", "b", "c", "d")]
    rewards = _rewards(*[(i, "ok", "K") for i in ("a", "b", "c", "d")])
    admitted, stats = build_admission_set(rollouts, rewards, per_key_cap=2)
    assert len(admitted) == 2
    # distinct_nkeys sees through the collapse even though 4 samples survived.
    assert stats.distinct_nkeys == 1


def test_per_style_cap_keeps_the_set_spread() -> None:
    # Six ok survivors, all from the easiest style, each a distinct program.
    rollouts = [Rollout(str(i), "easy", f"src{i}") for i in range(6)]
    rewards = _rewards(*[(str(i), "ok", f"k{i}") for i in range(6)])
    admitted, _ = build_admission_set(rollouts, rewards, per_style_frac=0.5)
    # ok_total=6, style_cap=3 -> at most half the set is the easy style.
    assert len(admitted) == 3
    assert all(r.style == "easy" for r in admitted)


def test_admission_ignores_reward_value_entirely() -> None:
    # Admission is by class + diversity, never by the scalar -- so pnl can never
    # leak into what we train on, keeping the held-out result non-circular.
    # (parse_rewards drops `value`/`train_pnl`; this just documents the intent.)
    parsed = parse_rewards(
        '{"id":"a","value":1.0,"class":"ok","fuel":25,"trades":9,'
        '"train_pnl":-99999,"hash":"0x1","nkey":"0xK"}\n'
    )
    assert parsed["a"].cls == "ok"
    assert not hasattr(parsed["a"], "value")
    assert not hasattr(parsed["a"], "train_pnl")


def test_order_is_preserved_for_reproducibility() -> None:
    # Distinct styles so the per-style cap does not bite -- this checks order.
    rollouts = [Rollout(i, i, f"src{i}") for i in ("z", "y", "x")]
    rewards = _rewards(("z", "ok", "k1"), ("y", "ok", "k2"), ("x", "ok", "k3"))
    admitted, _ = build_admission_set(rollouts, rewards)
    assert [r.id for r in admitted] == ["z", "y", "x"]


if __name__ == "__main__":
    fns = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for fn in fns:
        fn()
        print(f"ok  {fn.__name__}")
    print(f"\n{len(fns)} passed")
