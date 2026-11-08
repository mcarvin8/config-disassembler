"""Summarize a dhat-heap.json written by `replay_workload --features dhat-heap`.

Prints which crate call sites hold the most memory at the global heap peak
(dhat's "gb" = bytes live at t-gmax), so a memory fix can target what is
actually live rather than what allocates the most in total.

Each allocation is attributed to the first `config_disassembler::` frame in its
stack (the crate code that asked for the memory); allocations with no crate
frame (tokio, std, the replay harness) are grouped under their first non-
allocator frame.

Usage: python3 examples/dhat_summary.py dhat-heap.json [top_n]
"""

import json
import re
import sys
from collections import defaultdict

ALLOCATOR_FRAME = re.compile(
    r"dhat::|alloc::alloc|alloc::raw_vec|__rust_|core::alloc|<alloc::|std::alloc|hashbrown::raw|"
    r"alloc::vec::|alloc::string::|alloc::boxed::|alloc::collections::|indexmap::|serde_json::|"
    r"core::ptr::|core::iter::|core::ops::"
)


def frame_name(raw: str) -> str:
    # "0x7ff6...: some::function (file.rs:12:5)" -> "some::function (file.rs:12)"
    name = raw.split(": ", 1)[-1]
    return re.sub(r":\d+\)$", ")", name)


def main() -> None:
    path = sys.argv[1]
    top_n = int(sys.argv[2]) if len(sys.argv) > 2 else 15
    data = json.load(open(path))
    frames = data["ftbl"]

    total_at_peak = sum(pp["gb"] for pp in data["pps"])
    by_site = defaultdict(lambda: {"bytes": 0, "blocks": 0, "inner": defaultdict(int)})
    for pp in data["pps"]:
        if pp["gb"] == 0:
            continue
        names = [frame_name(frames[i]) for i in pp["fs"]]
        site = next((n for n in names if n.startswith("config_disassembler::")), None)
        if site is None:
            site = next((n for n in names if not ALLOCATOR_FRAME.search(n)), names[-1] if names else "?")
        # The frame just inside the crate frame says WHAT was allocated
        # (e.g. serde_json map insert, String clone, Vec growth).
        inner = "?"
        if site in names:
            idx = names.index(site)
            inner = names[idx - 1] if idx > 0 else "(direct)"
        entry = by_site[site]
        entry["bytes"] += pp["gb"]
        entry["blocks"] += pp["gbk"]
        entry["inner"][inner] += pp["gb"]

    mb = lambda b: b / (1024 * 1024)
    print(f"heap at global peak: {mb(total_at_peak):.1f} MB across {len(by_site)} call sites\n")
    print("| MB at peak | share | blocks | crate call site | largest inner allocation |")
    print("|---:|---:|---:|---|---|")
    ranked = sorted(by_site.items(), key=lambda kv: kv[1]["bytes"], reverse=True)[:top_n]
    for site, entry in ranked:
        inner, _ = max(entry["inner"].items(), key=lambda kv: kv[1])
        share = entry["bytes"] / total_at_peak * 100 if total_at_peak else 0
        print(
            f"| {mb(entry['bytes']):.2f} | {share:.1f}% | {entry['blocks']} | "
            f"`{site[:110]}` | `{inner[:90]}` |"
        )


if __name__ == "__main__":
    main()
