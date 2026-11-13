# Build the release tarball, attach it to a GitHub Release, and publish the
# download page under docs/ for GitHub Pages.
#
#   packaging/make-dist.sh            # build + release + regenerate docs
#   packaging/make-dist.sh --pages    # also commit and push the docs/ page
set -euo pipefail

cd "$(dirname "$0")/.."

BIN=revolinux
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TAG="v$VERSION"
ARCH=x86_64-unknown-linux-gnu
ASSET="$BIN-$VERSION-$ARCH.tar.gz"
URL="https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-/releases/download/$TAG/$ASSET"

echo "== build"
if command -v make >/dev/null 2>&1; then
    make dist
else
    # Fedora without `make`: build and package the same way by hand.
    cargo build --release
    rm -rf "dist/$BIN"
    mkdir -p "dist/$BIN"
    cp "target/release/$BIN" assets/revolinux.svg assets/revolinux.desktop "dist/$BIN/"
    cp packaging/install-release.sh "dist/$BIN/install.sh"
    chmod +x "dist/$BIN/install.sh"
    tar -C dist -czf "dist/$ASSET" "$BIN"
    (cd dist && sha256sum "$ASSET" > "$ASSET.sha256")
fi

if ! grep -qxF "dist/" .gitignore; then
    printf 'dist/\n' >> .gitignore
fi

echo "== GitHub release $TAG"
if ! command -v gh >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
    echo "note: gh not available or not logged in; asset not uploaded here."
    echo "      packaging/publish-release.sh publishes it through GitHub Actions."
elif gh release view "$TAG" >/dev/null 2>&1; then
    gh release upload "$TAG" "dist/$ASSET" "dist/$ASSET.sha256" --clobber
else
    gh release create "$TAG" "dist/$ASSET" "dist/$ASSET.sha256" \
        --title "RevoLinux $VERSION" \
        --generate-notes \
        --notes "Standalone Linux binary (x86_64). Download the tarball, extract it, run ./install.sh"
fi

echo "== download page"
mkdir -p docs
cat > docs/index.html <<HTML
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>RevoLinux - clean uninstaller for Fedora</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: #15171c; color: #e6e9ef;
    font: 16px/1.6 system-ui, "Cantarell", "Noto Sans", sans-serif;
  }
  a { color: #4fa3ff; }
  .wrap { max-width: 880px; margin: 0 auto; padding: 40px 20px 64px; }
  header { display: flex; align-items: center; gap: 16px; }
  header img { width: 64px; height: 64px; }
  h1 { margin: 0; font-size: 2rem; }
  .sub { color: #939aa6; }
  .badge {
    display: inline-block; padding: 2px 8px; border-radius: 999px;
    background: #21252d; color: #939aa6; font-size: .8rem;
  }
  .hero-btns { margin: 28px 0; display: flex; flex-wrap: wrap; gap: 12px; }
  .btn {
    display: inline-flex; align-items: center; gap: 8px;
    padding: 12px 20px; border-radius: 8px; text-decoration: none;
    font-weight: 600; border: 1px solid #2a2f39;
  }
  .primary { background: #4fa3ff; color: #08111c; border-color: #4fa3ff; }
  .secondary { background: #21252d; color: #e6e9ef; }
  section { margin-top: 40px; }
  h2 { font-size: 1.25rem; border-bottom: 1px solid #2a2f39; padding-bottom: 8px; }
  pre {
    background: #1b1e24; border: 1px solid #2a2f39; border-radius: 8px;
    padding: 14px 16px; overflow-x: auto; font-size: .9rem;
  }
  code { font-family: "JetBrains Mono", "Source Code Pro", monospace; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 16px; }
  .card { background: #21252d; border-radius: 10px; padding: 16px; }
  .card h3 { margin: 0 0 6px; font-size: 1rem; }
  .card p { margin: 0; color: #939aa6; font-size: .92rem; }
  img.shot { width: 100%; border-radius: 10px; border: 1px solid #2a2f39; }
  footer { margin-top: 56px; color: #6b7280; font-size: .85rem; }
  .sha { word-break: break-all; font-size: .8rem; color: #939aa6; }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <img src="https://raw.githubusercontent.com/lsupwp/Revo-Uninstall-Clone-Linux-/main/assets/revolinux.svg" alt="RevoLinux icon">
    <div>
      <h1>RevoLinux</h1>
      <div class="sub">Clean uninstaller for Fedora - DNF and Flatpak, with leftover purge.
        <span class="badge">$TAG</span></div>
    </div>
  </header>

  <div class="hero-btns">
    <a class="btn primary" href="$URL">Download for Linux x86_64 (tarball)</a>
    <a class="btn secondary" href="https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-">Source on GitHub</a>
  </div>

  <img class="shot" src="https://raw.githubusercontent.com/lsupwp/Revo-Uninstall-Clone-Linux-/main/assets/screenshot.png" alt="RevoLinux leftover review">

  <section>
    <h2>Quick install</h2>
    <pre><code>curl -LO $URL
tar -xzf $ASSET
cd $BIN
./install.sh</code></pre>
    <p class="sub">Installs to <code>/usr/local</code> and adds RevoLinux to your application menu.
      Per-user install without root: <code>PREFIX="$HOME/.local" ./install.sh</code>.</p>
  </section>

  <section>
    <h2>Run without installing</h2>
    <pre><code>tar -xzf $ASSET
./$BIN/revolinux</code></pre>
    <p class="sub">Requires Fedora with <code>dnf5</code>. Flatpak is optional.
      <code>pkexec</code> is used for admin authentication when system packages change.</p>
  </section>

  <section>
    <h2>Verify the download</h2>
    <pre><code>curl -LO $URL.sha256
sha256sum -c $ASSET.sha256</code></pre>
  </section>

  <section>
    <h2>What it does</h2>
    <div class="grid">
      <div class="card"><h3>Uninstall</h3><p>Removes the app with dnf5 or flatpak.</p></div>
      <div class="card"><h3>Purge scan</h3><p>Finds config, cache, data, temp and desktop files left behind.</p></div>
      <div class="card"><h3>Your choice</h3><p>Each leftover path has a checkbox and a confirmation dialog.</p></div>
      <div class="card"><h3>Safe</h3><p>RPM-owned files are never offered. System paths use polkit.</p></div>
    </div>
  </section>

  <section>
    <h2>CLI</h2>
    <pre><code>revolinux list
revolinux scan &lt;id&gt;
revolinux purge &lt;id&gt;          # read-only preview
revolinux purge &lt;id&gt; --yes    # uninstall + delete HIGH/MED leftovers</code></pre>
  </section>

  <footer>
    Release $TAG - built for x86_64 - MIT license.
    <a href="https://github.com/lsupwp/Revo-Uninstall-Clone-Linux-">github.com/lsupwp/Revo-Uninstall-Clone-Linux-</a>
  </footer>
</div>
</body>
</html>
HTML

touch docs/.nojekyll

if [[ "${1:-}" == "--pages" ]]; then
    echo "== commit and push docs"
    git add docs .gitignore
    git commit -m "docs: add download page for $TAG" || true
    git push origin HEAD
fi

echo "done: docs/index.html"
