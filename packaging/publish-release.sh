#!/usr/bin/env python3
"""Tag a release from a local checkout. CI does the rest.

The tag push triggers .github/workflows/release.yml, which builds the
tarball on GitHub and attaches it to the Release. No GitHub CLI or token
is needed locally; SSH push rights are enough.

    packaging/publish-release.sh [tag]     # default: v<Cargo.toml version>
"""

import subprocess
import sys


def git(*args, check=True):
    result = subprocess.run(args, text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        message = (result.stderr or result.stdout or "").strip()
        sys.exit(f"error: {' '.join(args)} failed: {message}")
    return result


def version() -> str:
    with open("Cargo.toml", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("version"):
                return line.split('"')[1]
    sys.exit("error: version not found in Cargo.toml")


def main() -> None:
    if git("rev-parse", "--is-inside-work-tree", check=False).returncode:
        sys.exit("error: run this inside the git repository")

    tag = sys.argv[1] if len(sys.argv) > 1 else f"v{version()}"
    if git("status", "--porcelain").stdout.strip():
        sys.exit("error: working tree not clean; commit first")

    print("== download page")
    subprocess.run(["bash", "packaging/make-dist.sh"], check=True)

    print("== tag")
    bot = [
        "-c", "user.name=release-bot",
        "-c", "user.email=release-bot@users.noreply.github.com",
    ]
    git(*bot, "tag", "-f", "-a", tag, "-m", f"RevoLinux {tag.lstrip('v')}")
    git("push", "origin", "HEAD")
    git("push", "-f", "origin", f"refs/tags/{tag}")

    print(
        "\nDone. CI builds the asset and publishes the Release.\n"
        f"  actions: https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/actions\n"
        f"  release: https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/releases/tag/{tag}"
    )


if __name__ == "__main__":
    main()
