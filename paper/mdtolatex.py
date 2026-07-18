#!/usr/bin/env python3
"""DRAFT.md -> paper.tex, a reproducible build for the arXiv preprint.

No pandoc in this environment, so this is a focused converter for the markdown
subset DRAFT.md actually uses: ATX headers, pipe tables, fenced + inline code,
**bold**/*italic*, bullet/numbered lists, horizontal rules, and the handful of
unicode glyphs the paper leans on (arrows, comparisons, section marks). The
paper carries its OWN section numbering (§1, §2.1, ...), so headers map to the
starred, unnumbered LaTeX sectioning commands — LaTeX must not renumber them.

    python mdtolatex.py            # writes paper.tex next to DRAFT.md

Compile with pdflatex (unicode is mapped to LaTeX commands, so no xelatex/font
dependency): pdflatex paper.tex (twice, for the table of contents / refs).
"""

from __future__ import annotations

import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "DRAFT.md")
OUT = os.path.join(HERE, "paper.tex")

PREAMBLE = r"""\documentclass[11pt]{article}
\usepackage[margin=1in]{geometry}
\usepackage{amssymb}
\usepackage{booktabs}
\usepackage{array}
\usepackage{longtable}
\usepackage{enumitem}
\usepackage{fancyvrb}
\usepackage[T1]{fontenc}
\usepackage{lmodern}
\usepackage{microtype}
\usepackage[hidelinks]{hyperref}
\setlength{\parskip}{0.5em}
\setlength{\parindent}{0pt}
\newcommand{\code}[1]{\texttt{#1}}
% Tighten tables and allow them to break/scale.
\renewcommand{\arraystretch}{1.15}

\title{@@TITLE@@}
\author{[author]\\\normalsize (preprint --- authorship to be set before submission)}
\date{\today}

\begin{document}
\maketitle
"""

# Unicode -> LaTeX (text-mode safe), applied to regular text (not code).
UNICODE = {
    "→": r"$\rightarrow$",   # ->
    "≤": r"$\le$",           # <=
    "≥": r"$\ge$",           # >=
    "∉": r"$\notin$",        # not-in
    "×": r"$\times$",        # times
    "—": "---",              # em dash
    "–": "--",               # en dash
    "§": r"\S{}",            # section
    "…": r"\ldots{}",        # ellipsis
    "✓": r"$\checkmark$",    # check
    "±": r"$\pm$",           # plus-minus
    "≈": r"$\approx$",       # approx
    "≥": r"$\ge$",
    "’": "'",                # right single quote
    "‘": "`",                # left single quote
    "“": "``",               # left double quote
    "”": "''",               # right double quote
}

SPECIALS = {
    "\\": r"\textbackslash{}",
    "&": r"\&",
    "%": r"\%",
    "#": r"\#",
    "_": r"\_",
    "{": r"\{",
    "}": r"\}",
    "$": r"\$",
    "~": r"\textasciitilde{}",
    "^": r"\textasciicircum{}",
    "|": r"\textbar{}",  # a literal pipe (from an escaped \| in a table cell)
}


def esc(s: str) -> str:
    """Escape LaTeX specials + map unicode, for REGULAR text (not code)."""
    out = []
    for ch in s:
        if ch in SPECIALS:
            out.append(SPECIALS[ch])
        elif ch in UNICODE:
            out.append(UNICODE[ch])
        else:
            out.append(ch)
    return "".join(out)


def inline(s: str) -> str:
    """Process one line of inline markdown: code, bold, italic, then escape.

    Code spans are extracted to placeholders first so their contents are never
    treated as bold/italic and are escaped with the code rules.
    """
    codes: list[str] = []

    def stash(m: re.Match) -> str:
        codes.append(m.group(1))
        return f"\x00{len(codes) - 1}\x00"

    s = re.sub(r"`([^`]+)`", stash, s)

    # Bold then italic, on placeholder-free text. Use sentinels so the escape
    # pass does not touch the command braces we introduce.
    s = re.sub(r"\*\*([^*]+)\*\*", lambda m: f"\x01B\x01{m.group(1)}\x01b\x01", s)
    s = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", lambda m: f"\x01I\x01{m.group(1)}\x01i\x01", s)

    s = esc(s)

    s = s.replace("\x01B\x01", r"\textbf{").replace("\x01b\x01", "}")
    s = s.replace("\x01I\x01", r"\emph{").replace("\x01i\x01", "}")

    def restore(m: re.Match) -> str:
        # esc() maps the few unicode glyphs that appear in code spans (arrows in
        # e.g. `Compile -> 0`) to math; acceptable and rare.
        return r"\code{" + esc(codes[int(m.group(1))]) + "}"

    s = re.sub(r"\x00(\d+)\x00", restore, s)
    return s


def convert(md: str) -> str:
    lines = md.split("\n")
    out: list[str] = []
    i = 0
    n = len(lines)
    title = "Purpose-sized languages"
    list_stack: list[str] = []  # 'itemize' | 'enumerate'

    def close_lists(to_depth: int = 0) -> None:
        while len(list_stack) > to_depth:
            out.append(f"\\end{{{list_stack.pop()}}}")

    while i < n:
        line = lines[i]

        # Fenced code block
        if line.startswith("```"):
            close_lists()
            out.append(r"\begin{Verbatim}[fontsize=\small,frame=single,samepage=false]")
            i += 1
            while i < n and not lines[i].startswith("```"):
                out.append(lines[i])
                i += 1
            out.append(r"\end{Verbatim}")
            i += 1
            continue

        # Table: a run of lines starting with '|'
        if line.lstrip().startswith("|"):
            close_lists()
            tbl: list[str] = []
            while i < n and lines[i].lstrip().startswith("|"):
                tbl.append(lines[i].strip())
                i += 1
            out.append(render_table(tbl))
            continue

        # Headers
        m = re.match(r"^(#{1,3})\s+(.*)$", line)
        if m:
            close_lists()
            level, text = len(m.group(1)), m.group(2).strip()
            if level == 1:
                title = text  # H1 is the paper title (used in preamble)
            elif level == 2:
                out.append(r"\section*{" + inline(text) + "}")
            else:
                out.append(r"\subsection*{" + inline(text) + "}")
            i += 1
            continue

        # Horizontal rule
        if line.strip() == "---":
            close_lists()
            out.append(r"\bigskip\hrule\bigskip")
            i += 1
            continue

        # List items (support one level of nesting via leading spaces)
        lm = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)$", line)
        if lm:
            indent, marker, text = lm.group(1), lm.group(2), lm.group(3)
            depth = len(indent) // 2 + 1  # 2 spaces per nesting level
            kind = "enumerate" if marker[0].isdigit() else "itemize"
            while len(list_stack) > depth:
                out.append(f"\\end{{{list_stack.pop()}}}")
            # Same depth but the list KIND changed (bullets -> numbers with no
            # blank line): close and reopen so items land in the right env.
            if len(list_stack) == depth and list_stack and list_stack[-1] != kind:
                out.append(f"\\end{{{list_stack.pop()}}}")
            while len(list_stack) < depth:
                out.append(f"\\begin{{{kind}}}[leftmargin=*]")
                list_stack.append(kind)
            out.append(r"\item " + inline(text))
            i += 1
            continue

        # Blank line
        if line.strip() == "":
            close_lists()
            out.append("")
            i += 1
            continue

        # Regular paragraph line
        close_lists()
        out.append(inline(line))
        i += 1

    close_lists()
    body = "\n".join(out)
    return PREAMBLE.replace("@@TITLE@@", esc(title)) + body + "\n\\end{document}\n"


def render_table(rows: list[str]) -> str:
    def cells(r: str) -> list[str]:
        # Split on UNESCAPED pipes only, then unescape `\|` back to a literal
        # pipe (which esc() renders as \textbar). A naive split on every "|"
        # would truncate cells that contain an escaped pipe in inline code.
        r = r.strip().strip("|")
        return [c.strip().replace(r"\|", "|") for c in re.split(r"(?<!\\)\|", r)]

    header = cells(rows[0])
    ncol = len(header)
    # rows[1] is the |---|---| separator; detect alignment (:) if present.
    aligns = []
    for spec in cells(rows[1]) if len(rows) > 1 else []:
        if spec.startswith(":") and spec.endswith(":"):
            aligns.append("c")
        elif spec.endswith(":"):
            aligns.append("r")
        else:
            aligns.append("l")
    while len(aligns) < ncol:
        aligns.append("l")
    colspec = " ".join(f"p{{{0.9 / ncol:.3f}\\linewidth}}" for _ in range(ncol))

    body_rows = rows[2:] if len(rows) > 2 else []
    out = [r"\begingroup\small", r"\begin{longtable}{" + colspec + "}", r"\toprule"]
    out.append(" & ".join(r"\textbf{" + inline(c) + "}" for c in header) + r" \\")
    out.append(r"\midrule\endhead")
    for r in body_rows:
        c = cells(r)
        c += [""] * (ncol - len(c))
        out.append(" & ".join(inline(x) for x in c[:ncol]) + r" \\")
    out.append(r"\bottomrule")
    out.append(r"\end{longtable}\endgroup")
    return "\n".join(out)


if __name__ == "__main__":
    with open(SRC, encoding="utf-8") as f:
        md = f.read()
    tex = convert(md)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(tex)
    print(f"wrote {OUT} ({tex.count(chr(10))} lines)")
