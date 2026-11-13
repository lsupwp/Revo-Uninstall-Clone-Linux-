#!/usr/bin/env python3
"""Publish a release from a local Git checkout with SSH push access only.

No GitHub CLI, no API token. It ships the tarball through a Git ref
(``build-assets``), creates the tag on the commit that stack lives in, and
tells the GitHub Actions workflow ``release.yml`` to turn that stack into a
Release. Pushing a tag is enough authentication: the SSH key already works.

    packaging/publish-release.sh [tag]     # default: v<Cargo.toml version>
"""

import hashlib
import os
import shutil
import subprocess
import sys

BIN = "revolinux"
ASSETS_REF = "build-assets"
TMP_BRANCH = "release-tmp"
ANCHOR = ".release-anchor"
# Files that make up the asset stack. Nothing else is pushed.
STACK_FILES = [ANCHOR, ".gitignore"]


def run(*args, capture=False, check=True):
    result = subprocess.run(
        args,
        text=True,
        capture_output=capture,
        check=False,
    )
    if check and result.returncode != 0:
        message = (result.stderr or result.stdout or "").strip()
        sys.exit(f"error: {' '.join(args)} failed: {message}")
    return result


def git(*args, capture=False, check=True):
    return run("git", *args, capture=capture, check=check)


def version() -> str:
    with open("Cargo.toml", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("version"):
                return line.split('"')[1]
    sys.exit("error: version not found in Cargo.toml")


def main() -> None:
    if git("rev-parse", "--is-inside-work-tree", capture=True, check=False).returncode:
        sys.exit("error: run this inside the git repository")

    tag = sys.argv[1] if len(sys.argv) > 1 else f"v{version()}"
    status = git("status", "--porcelain", capture=True).stdout.strip()
    if status:
        print("warning: uncommitted changes are ignored by the release (buildable files only)", file=sys.stderr)

    print(f"== build {tag}")
    run("bash", "packaging/make-dist.sh")

    tarballs = sorted(
        name
        for name in os.listdir("dist")
        if name.endswith(".tar.gz") and BIN in name
    )
    if not tarballs:
        sys.exit("error: no tarball in dist/")
    tarball = tarballs[-1]
    sha_file = f"{tarball}.sha256"
    sha = hashlib.sha256(open(os.path.join("dist", tarball), "rb").read()).hexdigest()

    print("== push asset stack")
    workdir = ".release-tmp"
    shutil.rmtree(workdir, ignore_errors=True)
    os.makedirs(workdir)
    git("worktree", "add", "--force", "--detach", workdir, "HEAD")
    try:
        shutil.copy(os.path.join("dist", tarball), workdir)
        shutil.copy(os.path.join("dist", sha_file), workdir)
        with open(os.path.join(workdir, ANCHOR), "w", encoding="utf-8") as anchor:
            anchor.write(f"{tag}\n{tarball}\n{sha}\n")

        # The worktree still holds the source tree. Drop everything from the
        # index and the disk except the stack files, then build one orphan
        # commit that contains only the asset.
        git("-C", workdir, "checkout", "--orphan", TMP_BRANCH)
        git("-C", workdir, "rm", "-r", "-q", "--cached", ".")
        for name in os.listdir(workdir):
            if name in STACK_FILES or name in (tarball, sha_file):
                continue
            path = os.path.join(workdir, name)
            shutil.rmtree(path) if os.path.isdir(path) else os.remove(path)

        git("-C", workdir, "add", "-f", tarball, sha_file, ANCHOR)
        git(
            "-C",
            workdir,
            "-c",
            "user.name=release-bot",
            "-c",
            "user.email=release-bot@users.noreply.github.com",
            "commit",
            "-m",
            f"release: assets for {tag}",
        )
        head = git("-C", workdir, "rev-parse", "HEAD", capture=True).stdout.strip()
        changed = git("-C", workdir, "show", "--name-only", "--format=", "HEAD",
                      capture=True).stdout.split()
        print("   stack files:", ", ".join(sorted(changed)))


        previous = git(
            "rev-parse", "--verify", f"refs/remotes/origin/{ASSETS_REF}",
            capture=True, check=False,
        ).stdout.strip()
        args = ["push", "--force", "origin", f"{head}:refs/heads/{ASSETS_REF}"]
        git(*args)
        if previous:
            print(f"   previous assets commit: {previous[:12]} -> kept as before")
    finally:
        git("worktree", "remove", "--force", workdir, check=False)

    print("== tag")
    git("tag", "-f", "-a", tag, "-m", f"RevoLinux {tag.lstrip('v')}", head)
    git("push", "-f", "origin", f"refs/tags/{tag}")

    print(
        "\nDone. GitHub Actions (.github/workflows/release.yml) turns the\n"
        f"{ASSETS_REF} stack into a Release with {tarball}.\n"
        f"Watch: https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/actions\n"
        f"Asset URL after the run:\n"
        f"  https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/releases/download/{tag}/{tarball}"
    )


if __name__ == "__main__":
    main()
