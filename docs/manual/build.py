# -*- coding: utf-8 -*-
"""Build the SemOps System manual: the web edition, the EPUB and the A4 PDF.

    python docs/manual/build.py                  # all three, into docs/manual/build/
    python docs/manual/build.py --site           # just the web edition
    python docs/manual/build.py --epub --pdf     # just these
    python docs/manual/build.py --out DIR        # somewhere else

The chapters are the numbered Markdown files beside this script, in pandoc's Markdown,
read in file-name order. Each starts with a level-one heading carrying an identifier,
`# Installing {#installing}`, and links to another chapter by its file name,
`[Installing](02-installing.md)`, or to a section in it, `02-installing.md#from-source`.
The build rewrites those links for each edition: to the chapter's page on the web, to an
anchor in the single document the EPUB and the PDF are made from.

Needs pandoc (PANDOC to override) for all three, and Microsoft Edge or Chromium (BROWSER
to override) to print the PDF. Nothing else, and nothing is fetched while it runs.
"""
from __future__ import annotations

import argparse
import html
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TITLE = "The SemOps System Manual"
SUBTITLE = "Installing, running and checking the SemOps toolchain"
REPO = "https://github.com/pwin/SemopsSystem"


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^\[workspace\.package\][^\[]*?^version = "([^"]+)"', text, re.M | re.S)
    return m.group(1) if m else "unknown"


@dataclass
class Chapter:
    path: Path
    number: str
    ident: str
    title: str
    text: str

    @property
    def page(self) -> str:
        return "index.html" if self.number == "00" else f"{self.path.stem}.html"


def chapters() -> list[Chapter]:
    out = []
    for path in sorted(HERE.glob("[0-9][0-9]-*.md")):
        text = path.read_text(encoding="utf-8")
        m = re.match(r"#\s+(.+?)\s+\{#([a-z0-9-]+)\}\s*$", text.splitlines()[0])
        if not m:
            sys.exit(f"{path.name}: the first line must be '# Title {{#id}}'")
        out.append(Chapter(path, path.name[:2], m.group(2), m.group(1), text))
    return out


def pandoc() -> str:
    exe = os.environ.get("PANDOC") or shutil.which("pandoc")
    if not exe:
        sys.exit("pandoc is needed: https://pandoc.org/installing.html, or set PANDOC")
    return exe


def browser() -> str:
    candidates = [
        os.environ.get("BROWSER"),
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        shutil.which("msedge"), shutil.which("microsoft-edge"), shutil.which("chromium"),
        shutil.which("chromium-browser"), shutil.which("google-chrome"),
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    for c in candidates:
        if c and Path(c).exists():
            return c
    sys.exit("printing the PDF needs Microsoft Edge or Chromium; set BROWSER to one")


def no_highlighting() -> str:
    """pandoc 3.8 renamed `--no-highlight`; older ones know only the old name."""
    out = subprocess.run([pandoc(), "--version"], capture_output=True, text=True).stdout
    m = re.search(r"pandoc\S*\s+(\d+)\.(\d+)", out)
    new = m and (int(m.group(1)), int(m.group(2))) >= (3, 8)
    return "--syntax-highlighting=none" if new else "--no-highlight"


def to_html(markdown: str) -> str:
    """One chapter's Markdown as an HTML fragment."""
    run = subprocess.run(
        [pandoc(), "-f", "markdown-smart", "-t", "html5", "--wrap=none", no_highlighting()],
        input=markdown.encode("utf-8"), capture_output=True, check=True,
    )
    # pandoc on Windows writes CRLF; a text-mode write would then double the CR.
    return run.stdout.decode("utf-8").replace("\r\n", "\n")


CHAPTER_LINK = re.compile(r'href="(\d\d-[a-z0-9-]+)\.md(#[^"]*)?"')
REPO_LINK = re.compile(r'href="(\.\./\.\./[^"]+|\.\./[^"]+)"')


def site_links(fragment: str, by_stem: dict[str, Chapter]) -> str:
    def chapter(m: re.Match) -> str:
        target = by_stem.get(m.group(1))
        page = target.page if target else m.group(1) + ".html"
        return f'href="{page}{m.group(2) or ""}"'

    def repo(m: re.Match) -> str:
        rel = m.group(1)
        while rel.startswith("../"):
            rel = rel[3:]
        kind = "tree" if rel.endswith("/") else "blob"
        return f'href="{REPO}/{kind}/main/{rel}"'

    return REPO_LINK.sub(repo, CHAPTER_LINK.sub(chapter, fragment))


def single_links(markdown: str, by_stem: dict[str, Chapter]) -> str:
    """Chapter links as anchors within one document, for the EPUB and the PDF."""
    def chapter(m: re.Match) -> str:
        target = by_stem.get(m.group(1))
        anchor = m.group(2) or (f"#{target.ident}" if target else "")
        return f"]({anchor})"

    markdown = re.sub(r"\]\((\d\d-[a-z0-9-]+)\.md(#[a-z0-9-]+)?\)", chapter, markdown)
    return re.sub(r"\]\((?:\.\./)+([^)]+)\)", lambda m: f"]({REPO}/blob/main/{m.group(1)})", markdown)


# No web fonts: the pages make no third-party requests, as the other Semantechs manuals
# do not. The stylesheet names the house fonts first and falls back to the system's.
FONTS = ""


def rail(chs: list[Chapter], current: Chapter, ver: str) -> str:
    items = []
    for c in chs:
        here = ' aria-current="page"' if c is current else ""
        n = "" if c.number == "00" else c.number
        items.append(f'<li><a href="{c.page}"{here}><span class="n">{n}</span>{html.escape(c.title)}</a></li>')
    return (
        '<nav class="rail" aria-label="Chapters">'
        '<p class="brand">The <em>SemOps System</em> Manual</p>'
        f'<p class="tagline">For SemOps System {ver}</p>'
        f'<ol>{"".join(items)}</ol>'
        '<p class="railnote">Also as an <a href="The-SemOps-System-Manual.epub">EPUB</a> '
        'and an <a href="The-SemOps-System-Manual-A4.pdf">A4 PDF</a>.</p>'
        "</nav>"
    )


def page(chs: list[Chapter], c: Chapter, body: str, ver: str) -> str:
    i = chs.index(c)
    prev = chs[i - 1] if i > 0 else None
    nxt = chs[i + 1] if i + 1 < len(chs) else None
    pager = '<nav class="pager">'
    if prev:
        pager += f'<a class="prev" href="{prev.page}"><span>Previous</span>{html.escape(prev.title)}</a>'
    if nxt:
        pager += f'<a class="next" href="{nxt.page}"><span>Next</span>{html.escape(nxt.title)}</a>'
    pager += "</nav>"
    eyebrow = "A Semantechs manual" if c.number == "00" else f"Chapter {int(c.number)}"
    title = TITLE if c.number == "00" else f"{c.title} · {TITLE}"
    logo = '<img class="logo" src="semantechs-logo.png" alt="Semantechs">' if c.number == "00" else ""
    return f"""<!doctype html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
{FONTS}
<link rel="stylesheet" href="manual.css">
<link rel="icon" href="favicon.png">
</head>
<body>
<div class="shell">
{rail(chs, c, ver)}
<main>
<article class="chapter">
{logo}<p class="eyebrow">{eyebrow}</p>
{body}
{pager}
</article>
</main>
</div>
</body>
</html>
"""


def build_site(chs: list[Chapter], out: Path, ver: str) -> None:
    out.mkdir(parents=True, exist_ok=True)
    by_stem = {c.path.stem: c for c in chs}
    for c in chs:
        body = site_links(to_html(c.text), by_stem)
        (out / c.page).write_text(page(chs, c, body, ver), encoding="utf-8", newline="\n")
    shutil.copy(HERE / "assets" / "manual.css", out / "manual.css")
    shutil.copy(HERE / "assets" / "semantechs-logo.png", out / "favicon.png")
    shutil.copy(HERE / "assets" / "semantechs-logo.png", out / "semantechs-logo.png")
    print(f"web edition: {out}/index.html ({len(chs)} pages)")


def combined(chs: list[Chapter]) -> str:
    """The chapters as one document. The front page's heading is the book's title, which
    the title page and the cover already carry, so in a book it becomes an introduction."""
    by_stem = {c.path.stem: c for c in chs}
    texts = []
    for c in chs:
        text = single_links(c.text, by_stem)
        if c.number == "00":
            text = text.replace(text.splitlines()[0], f"# About this manual {{#{c.ident}}}", 1)
        texts.append(text)
    return "\n\n".join(texts)


COVER = """<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;width:1600px;height:2400px;background:#fbfaf7;font-family:"Atkinson Hyperlegible","Segoe UI",sans-serif;color:#191d24}}
.c{{box-sizing:border-box;height:2400px;padding:220px 170px;display:flex;flex-direction:column;border-left:60px solid #c8006e}}
img{{width:220px;height:220px}}
.e{{margin-top:160px;font-family:Consolas,monospace;font-size:44px;letter-spacing:.16em;text-transform:uppercase;color:#c8006e}}
h1{{font-family:Georgia,serif;font-weight:600;font-size:168px;line-height:1.02;margin:40px 0 70px;letter-spacing:-2px}}
p{{font-size:62px;line-height:1.3;color:#454c58;margin:0;max-width:1150px}}
.v{{margin-top:auto;font-family:Consolas,monospace;font-size:48px;color:#6f7784}}
</style></head><body><div class="c"><img src="{logo}" alt=""><div class="e">A Semantechs manual</div>
<h1>The SemOps System Manual</h1><p>{subtitle}</p><div class="v">For SemOps System {ver}</div></div></body></html>"""


def build_cover(out: Path, ver: str) -> Path | None:
    """A cover image for the EPUB, drawn by the browser so it carries the version."""
    try:
        exe = browser()
    except SystemExit:
        return None
    target = out / "cover.png"
    with tempfile.TemporaryDirectory() as tmp:
        page_ = Path(tmp) / "cover.html"
        page_.write_text(COVER.format(
            logo=(HERE / "assets" / "semantechs-logo.png").as_uri(),
            subtitle=html.escape(SUBTITLE), ver=ver), encoding="utf-8")
        subprocess.run(
            [exe, "--headless", "--disable-gpu", "--no-first-run", f"--user-data-dir={Path(tmp) / 'p'}",
             "--hide-scrollbars", "--window-size=1600,2400", f"--screenshot={target}", page_.as_uri()],
            check=True, capture_output=True, timeout=120,
        )
    return target if target.exists() else None


def build_epub(chs: list[Chapter], out: Path, ver: str) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    target = out / "The-SemOps-System-Manual.epub"
    cover = build_cover(out, ver)
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / "manual.md"
        source.write_text(combined(chs), encoding="utf-8", newline="\n")
        subprocess.run(
            [pandoc(), str(source), "-f", "markdown-smart", "-o", str(target),
             "--toc", "--toc-depth=2", "--split-level=1", no_highlighting(),
             "--css", str(HERE / "assets" / "epub.css"),
             "--metadata", f"title={TITLE}",
             "--metadata", f"subtitle={SUBTITLE}",
             "--metadata", "author=Peter Winstanley",
             "--metadata", "publisher=Semantechs",
             "--metadata", "lang=en-GB",
             "--metadata", f"date=SemOps System {ver}",
             "--metadata", "rights=MIT OR Apache-2.0",
             *(["--epub-cover-image", str(cover)] if cover else [])],
            check=True,
        )
    print(f"EPUB: {target}")
    return target


PRINT_TEMPLATE = """<!doctype html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<title>{title}</title>
<link rel="stylesheet" href="{css}">
</head>
<body class="print">
<section class="titlepage">
<img src="{logo}" alt="" class="logo">
<p class="eyebrow">A Semantechs manual</p>
<h1 class="booktitle">{title}</h1>
<p class="booksub">{subtitle}</p>
<p class="bookver">For SemOps System {ver}</p>
</section>
<nav class="contents">
<h2 class="contents-title">Contents</h2>
<ol start="0">{contents}</ol>
</nav>
{body}
</body>
</html>
"""


def build_pdf(chs: list[Chapter], out: Path, ver: str) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    target = out / "The-SemOps-System-Manual-A4.pdf"
    body = to_html(combined(chs))
    contents = "".join(
        f'<li><a href="#{c.ident}">'
        f'{"About this manual" if c.number == "00" else html.escape(c.title)}</a></li>'
        for c in chs
    )
    # Kept beside the PDF: opening it in a browser shows what the printer was given.
    doc = out / "manual-print.html"
    with tempfile.TemporaryDirectory() as tmp:
        doc.write_text(PRINT_TEMPLATE.format(
            title=html.escape(TITLE), subtitle=html.escape(SUBTITLE), ver=ver, contents=contents,
            body=body, css=(HERE / "assets" / "print.css").as_uri(),
            logo=(HERE / "assets" / "semantechs-logo.png").as_uri(),
        ), encoding="utf-8", newline="\n")
        profile = Path(tmp) / "profile"
        subprocess.run(
            [browser(), "--headless", "--disable-gpu", "--no-first-run",
             f"--user-data-dir={profile}", "--no-pdf-header-footer",
             f"--print-to-pdf={target}", doc.as_uri()],
            check=True, capture_output=True, timeout=300,
        )
    if not target.exists() or target.stat().st_size < 10_000:
        sys.exit("the browser did not write the PDF")
    print(f"PDF: {target}")
    return target


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--site", action="store_true")
    ap.add_argument("--epub", action="store_true")
    ap.add_argument("--pdf", action="store_true")
    ap.add_argument("--out", type=Path, default=HERE / "build")
    args = ap.parse_args()
    everything = not (args.site or args.epub or args.pdf)
    chs = chapters()
    ver = version()
    site = args.out / "site"
    if everything or args.site:
        build_site(chs, site, ver)
    if everything or args.epub:
        epub = build_epub(chs, args.out, ver)
        if site.exists():
            shutil.copy(epub, site / epub.name)
    if everything or args.pdf:
        pdf = build_pdf(chs, args.out, ver)
        if site.exists():
            shutil.copy(pdf, site / pdf.name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
