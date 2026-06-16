#!/usr/bin/env python3
"""Print the supported-models table built from profiles/*.toml.

    python3 gen/models_table.py            # plain text (for `make models`)
    python3 gen/models_table.py --markdown # a Markdown table (for the README)

The README's supported-models section is generated from this so it never goes
stale: every profile that exists is a supported board, listed here automatically.
"""
import sys
import glob
import os

try:
    import tomllib
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # type: ignore
    except ModuleNotFoundError:
        sys.exit("error: need Python 3.11+ (tomllib) or `pip install tomli`")


def rows(profile_dir):
    out = []
    for path in sorted(glob.glob(os.path.join(profile_dir, "*.toml"))):
        with open(path, "rb") as f:
            p = tomllib.load(f)
        meta = p.get("meta", {})
        mx = p.get("matrix", {})
        keys = mx.get("cols", 0) * mx.get("rows", 0)
        out.append({
            "dev_id": meta.get("dev_id", "?"),
            "name": meta.get("display_name", "?"),
            "internal": meta.get("name", ""),
            "switch": meta.get("switch_type", "?"),
            "wireless": "yes" if meta.get("wireless") else "no",
            "keys": keys,
        })
    return out


def main():
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    data = rows(os.path.join(here, "profiles"))
    md = "--markdown" in sys.argv[1:]
    if md:
        print("| dev_id | Model | Switch | Wireless | Sites |")
        print("|---|---|---|---|---|")
        for r in data:
            print(f"| `{r['dev_id']}` | {r['name']} | {r['switch']} | {r['wireless']} | {r['keys']} |")
    else:
        print(f"{'dev_id':>7}  {'model':<22} {'switch':<6} {'wireless':<8} sites")
        for r in data:
            print(f"{r['dev_id']:>7}  {r['name']:<22} {r['switch']:<6} {r['wireless']:<8} {r['keys']}")
    if not data:
        print("(no profiles found)")


if __name__ == "__main__":
    main()
