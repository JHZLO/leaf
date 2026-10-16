"""Write the Phase 0 spike fixtures into target/spike/fixtures.

Code files are real Rust sources from the cargo registry (the gpui-kit
dependencies), repeated up to 1, 10 and 50 MB, so highlighting sees real code.
"""

import glob
import os
import sys
import unicodedata

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "target", "spike", "fixtures")
REGISTRY = os.path.expanduser("~/.cargo/registry/src")


def code_pool():
    files = []
    for name in ("gpui-pre-0.3.9", "gpui-component-0.7.3", "gpui-base-0.7.3"):
        files += glob.glob(os.path.join(REGISTRY, "*", name, "**", "*.rs"), recursive=True)
    if not files:
        sys.exit("run `cargo fetch` first: no gpui-kit sources in the cargo registry")
    return b"".join(open(f, "rb").read() for f in sorted(files))


def markdown(sections):
    block = (
        "## Section {n}\n\n"
        "Some prose about **paths** and `code`, with a [link](https://example.com/{n}).\n"
        "한글 문장도 섞는다. 경로는 `docs/guide/paths.md:{n}` 처럼 복사한다.\n\n"
        "- item one\n- item two with `inline`\n- [ ] a task\n\n"
        "| Keys | Result |\n|---|---|\n| a | b |\n\n"
        "```rust\nfn main() {{ println!(\"{n}\"); }}\n```\n\n"
    )
    return "# Markdown bench\n\n" + "".join(block.format(n=i) for i in range(1, sections + 1))


def main():
    os.makedirs(OUT, exist_ok=True)
    pool = code_pool()
    for name, size in (("code-1mb.rs", 1 << 20), ("code-10mb.rs", 10 << 20), ("code-50mb.rs", 50 << 20)):
        data = (pool * (size // len(pool) + 1))[:size]
        data = data[: data.rfind(b"\n") + 1]
        open(os.path.join(OUT, name), "wb").write(data)
        open(os.path.join(OUT, name.replace("code-", "plain-").replace(".rs", ".txt")), "wb").write(data)
    open(os.path.join(OUT, "hello.md"), "w").write("# Hello\n\nA small file for startup runs.\n")
    open(os.path.join(OUT, "doc-2k.md"), "w").write(markdown(114))
    open(os.path.join(OUT, "doc-10k.md"), "w").write(markdown(570))
    # A Hangul file name in NFD, the decomposed form macOS apps sometimes write.
    nfd = unicodedata.normalize("NFD", "한글 메모.md")
    open(os.path.join(OUT, nfd), "w").write("# 한글 메모\n\n자모가 풀린 파일 이름\n")
    print(f"fixtures in {OUT}")


if __name__ == "__main__":
    main()
