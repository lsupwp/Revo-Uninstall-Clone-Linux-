#!/usr/bin/env python3
"""Publish a release from a local Git checkout with push access only.

No GitHub CLI, no API token. The tarball travels through a Git branch
(``build-assets``); the tag triggers .github/workflows/release.yml, which
attaches the asset to a GitHub Release.

The asset branch is an orphan commit created in a throwaway clone inside
``dist/``: it contains the tarball, its checksum and ``.release-anchor``.

    packaging/publish-release.sh [tag]     # default: v<Cargo.toml version>
"""

import hashlib
import os
import shutil
import subprocess
import sys

BIN = "revolinux"
ASSETS_REF = "build-assets"
ANCHOR = ".release-anchor"


def run(*args, check=True):
    result = subprocess.run(args, text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        message = (result.stderr or result.stdout or "").strip()
        sys.exit(f"error: {' '.join(args)} failed: {message}")
    return result


def git(*args, check=True):
    return run("git", *args, check=check)


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
    branch = git("rev-parse", "--abbrev-ref", "HEAD").stdout.strip()
    if branch != "main":
        print(f"warning: tagging {branch}; releases normally tag main", file=sys.stderr)

    print(f"== build {tag}")
    run("bash", "packaging/make-dist.sh")

    tarballs = sorted(
        name for name in os.listdir("dist") if name.endswith(".tar.gz") and BIN in name
    )
    if not tarballs:
        sys.exit("error: no tarball in dist/")
    tarball = tarballs[-1]
    sha = hashlib.sha256(open(os.path.join("dist", tarball), "rb").read()).hexdigest()

    print("== stack asset branch")
    stack = os.path.join("dist", ".stack")
    shutil.rmtree(stack, ignore_errors=True)
    git("clone", "--quiet", "--no-hardlinks", ".", stack)
    shutil.copy(os.path.join("dist", tarball), stack)
    shutil.copy(os.path.join("dist", f"{tarball}.sha256"), stack)
    with open(os.path.join(stack, ANCHOR), "w", encoding="utf-8") as anchor:
        anchor.write(f"{tag}\n{tarball}\n{sha}\n")

    # Orphan branch: only the asset, nothing from the source tree.
    git("checkout", "--orphan", "-b", ASSETS_REF, cwd=stack)
    git("rm", "-r", "-q", "--cached", ".", cwd=stack)
    for name in os.listdir(stack):
        if name in (".git", tarball, f"{tarball}.sha256", ANCHOR):
            continue
        path = os.path.join(stack, name)
        shutil.rmtree(path) if os.path.isdir(path) else os.remove(path)

    bot = ["-c", "user.name=release-bot", "-c", "user.email=release-bot@users.noreply.github.com"]
    git("add", "-f", tarball, f"{tarball}.sha256", ANCHOR, cwd=stack)
    git(*bot, "commit", "-q", "-m", f"release: assets for {tag}", cwd=stack)
    files = git("show", "--name-only", "--format=", "HEAD", cwd=stack).stdout.split()
    print("   files:", ", ".join(files))
    git("push", "--quiet", "--force", "origin", f"{ASSETS_REF}:{ASSETS_REF}", cwd=stack)
    shutil.rmtree(stack, ignore_errors=True)

    print("== tag")
    git(*bot, "tag", "-f", "-a", tag, "-m", f"RevoLinux {tag.lstrip('v')}")
    git("push", "--quiet", "-f", "origin", f"refs/tags/{tag}")

    print(
        "\nDone. The release workflow attaches the asset.\n"
        f"  actions: https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/actions\n"
        f"  asset:   https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/releases/download/{tag}/{tarball}"
    )


if __name__ == "__main__":
    main()
