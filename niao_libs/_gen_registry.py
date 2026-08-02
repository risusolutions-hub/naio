#!/usr/bin/env python3
"""Generate niao_libs/registry.md from package.json descriptions."""
from __future__ import annotations

import json
import os
import re
from datetime import date

ROOT = os.path.dirname(os.path.abspath(__file__))

CATEGORIES: dict[str, list[str]] = {
    "Core & builtins": [
        "core",
        "dsa",
        "collections",
        "bignum",
        "rand",
        "nrand",
        "re",
        "parallel",
        "nasync",
        "npersist",
        "nsorted",
        "npar",
        "nlazy",
        "niter",
        "nproc",
        "nsimd",
        "narena",
        "npipe",
    ],
    "I/O, formats & encoding": [
        "io",
        "json",
        "codec",
        "nbinary",
        "ncbor",
        "archive",
        "time",
        "ncal",
        "ncsv",
        "nencoding",
        "nipaddr",
        "ntoml",
        "nyaml",
        "nzip",
        "nproto",
        "nmsgpack",
        "nmime",
        "nmarkdown",
        "nhtml",
        "nview",
        "nurl",
        "ncolumnar",
        "nparquet",
        "nxlsx",
        "npdf",
        "nmmap",
        "nhdf5",
        "nfs",
        "ncanon",
        "nsnap",
        "ntar",
        "nglob",
        "ngeo",
        "nical",
        "njpath",
    ],
    "Networking & web": [
        "http",
        "nreq",
        "nscrape",
        "net",
        "net_clients",
        "nws",
        "ngraphql",
        "nsmtp",
        "nssh",
        "nmail",
        "nimap",
        "nmqtt",
        "nmdns",
        "nrpc",
        "nopenapi",
        "crypto",
        "ncrypt",
        "njwt",
        "nwebhook",
        "notp",
        "nauth",
        "log",
        "nlog",
        "ntrace",
    ],
    "Databases & storage": [
        "nsqlite",
        "npg",
        "nmysql",
        "nmongo",
        "nredis",
        "nsearch",
        "nsupa",
        "nmodel",
        "nmigrate",
        "nvec",
        "nfts",
        "ncache",
        "nkv",
        "ndocstore",
    ],
    "Data & analytics": [
        "nframe",
        "nparquet",
        "ncl",
        "nsoa",
        "nstats",
        "nts",
        "nplot",
        "nvis",
    ],
    "Math & numerics": ["nmath", "ndecimal", "nnum", "ndsp", "nfin", "noptim"],
    "Machine learning": [
        "nml",
        "nlearn",
        "nboost",
        "ndataset",
        "nvision",
        "nnlp",
        "nspeech",
        "ntts",
        "neval",
        "ntune",
        "nsketch",
    ],
    "AI & LLM": [
        "nllm",
        "nhub",
        "nrag",
        "nembed",
        "nctx",
        "ntok",
        "ntemplate",
        "nprompt",
        "nprovider",
        "nguard",
        "nschema",
        "nmem",
    ],
    "Cloud & integrations": [
        "naws",
        "nazure",
        "nblob",
        "ngcp",
        "ncost",
        "nbudget",
        "nquota",
    ],
    "System & runtime": [
        "nos",
        "nenv",
        "nconfig",
        "nargs",
        "args",
        "ncpu",
        "ngpu",
        "nnpu",
        "nram",
        "ndevice",
        "nevent",
        "npace",
        "nbatch",
        "ncap",
        "nfallback",
        "nretry",
        "nkeyring",
        "ncrash",
        "nhotreload",
        "nwatch",
        "nfs",
        "nsignal",
        "nworkspace",
        "ncron",
    ],
    "Strings, validation & utilities": [
        "nstr",
        "nunicode",
        "nfmt",
        "nfsm",
        "nfunc",
        "nid",
        "npass",
        "ncolor",
        "nvalid",
        "nsanitize",
        "nsemver",
        "nshape",
        "ndiff",
        "ntextdiff",
        "nwhy",
        "nexplain",
        "nerrgen",
    ],
    "Developer tools & testing": [
        "ntest",
        "nbench",
        "nlint",
        "ndoc",
        "ndebug",
        "nreflect",
        "nfuzz",
        "ncassette",
        "nreplay",
        "nrepl",
        "nscaffold",
        "ncontract",
        "nprofile",
        "nshell",
        "nproc",
    ],
    "Applications & frameworks": ["ahiru", "nagent"],
}


def load_libs() -> dict[str, dict[str, str]]:
    libs: dict[str, dict[str, str]] = {}
    for name in os.listdir(ROOT):
        pkg = os.path.join(ROOT, name, "package.json")
        if not os.path.isdir(os.path.join(ROOT, name)) or not os.path.isfile(pkg):
            continue
        with open(pkg, encoding="utf-8-sig") as f:
            data = json.load(f)
        desc = re.sub(r"\s+", " ", data.get("description", "").strip())
        libs[data.get("name", name)] = {
            "description": desc,
            "version": data.get("version", ""),
            "kind": data.get("kind", "native"),
        }
    return libs


def anchor_for(category: str) -> str:
    slug = category.lower()
    slug = slug.replace(" & ", "-")
    slug = slug.replace(", ", "-")
    slug = slug.replace(" ", "-")
    return slug


def main() -> None:
    libs = load_libs()
    assigned: set[str] = set()
    sections: list[str] = []

    for category, names in CATEGORIES.items():
        present = [n for n in names if n in libs]
        if not present:
            continue
        rows = [
            f"| **{name}** | {libs[name]['description']} |"
            for name in sorted(present)
        ]
        assigned.update(present)
        sections.append(f"## {category}\n")
        sections.append("| Library | Description |")
        sections.append("|---------|-------------|")
        sections.extend(rows)
        sections.append("")

    uncat = sorted(set(libs) - assigned)
    if uncat:
        rows = [f"| **{name}** | {libs[name]['description']} |" for name in uncat]
        sections.append("## Other\n")
        sections.append("| Library | Description |")
        sections.append("|---------|-------------|")
        sections.extend(rows)
        sections.append("")

    alpha_rows = [
        f"| **{name}** | {libs[name]['description']} |"
        for name in sorted(libs)
    ]

    toc = ["## Contents", ""]
    for category in CATEGORIES:
        if any(n in libs for n in CATEGORIES[category]):
            toc.append(f"- [{category}](#{anchor_for(category)})")
    if uncat:
        toc.append("- [Other](#other)")
    toc.append("- [Alphabetical index](#alphabetical-index)")
    toc.append("")
    toc.append("---")
    toc.append("")

    today = date.today().isoformat()
    header = f"""# Niao Library Registry

Canonical index of all **{len(libs)}** libraries in [`niao_libs/`](.). Each one-line summary comes from that library's `package.json` `description` field.

- **Default runtime version:** `0.2.3` (most native builtins; `ahiru` is `0.3.0`)
- **Install:** `nm install <lib>` · **Browse:** [nms.taurus-tech.in](https://nms.taurus-tech.in)
- **Source catalog:** [`catalog.json`](catalog.json) — core packages shipped with `nm install --global`

> Auto-generated from `niao_libs/*/package.json` on {today}. Regenerate: `python niao_libs/_gen_registry.py`

---

"""

    alpha = [
        "## Alphabetical index",
        "",
        "| Library | Description |",
        "|---------|-------------|",
        *alpha_rows,
        "",
    ]

    content = header + "\n".join(toc) + "\n" + "\n".join(sections) + "\n" + "\n".join(alpha)
    out = os.path.join(ROOT, "registry.md")
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)

    print(f"Wrote {out} ({len(libs)} libraries)")
    if uncat:
        print("Uncategorized:", ", ".join(uncat))


if __name__ == "__main__":
    main()
