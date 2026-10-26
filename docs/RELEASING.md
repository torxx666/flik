# Distributing Flik without a local build

Users download a versioned executable from GitHub Releases. They do not need Rust, Cargo, Docker, or a compiler. The website should link to published release assets; it must not promise a downloadable release before one exists.

## Prepared platform matrix

| Platform | Archive suffix | Runner |
| --- | --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-musl` | `ubuntu-24.04` |
| Linux ARM64 | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| macOS Intel, macOS 11+ | `x86_64-apple-darwin` | `macos-15-intel` |
| macOS Apple Silicon, macOS 11+ | `aarch64-apple-darwin` | `macos-14` |

Archives are named `flik-vMAJOR.MINOR.PATCH-TARGET.tar.gz` and contain the executable `flik` and the project `LICENSE`, with no enclosing directory. Linux releases are built against musl and checked for static linking. Each archive has an entry in the release-wide `SHA256SUMS` file.

Windows binaries are not advertised or published by this workflow. Some file-viewing code is portable, but terminal behavior and the Unix-only stdin-pipe workflow need Windows validation first. Linux binaries can be used inside a compatible Linux WSL environment.

## First publication

1. The selected public repository is https://github.com/torxx666/flik and the prepared license is MIT. Review the source tree before the initial push; do not publish local logs, private fixtures, credentials, unrelated projects, internal notes, or build artifacts. Keep `Cargo.lock` tracked for repeatable dependency resolution.
2. Push the reviewed source, `.github/workflows/release.yml`, `scripts/install.sh`, and these instructions to that repository. Enable GitHub Actions and allow the three official `actions/*` actions used by the workflow.
3. Confirm that the package version in `Cargo.toml` is the version you want to release. The prepared workflow pins Rust **1.94.0**, matching the locally verified toolchain; update it deliberately when testing a newer compiler.
4. Run `cargo test --locked --all-targets`, `sh -n scripts/install.sh` and `python3 scripts/test_install.py`.
5. Create and push the matching stable tag:

   ```sh
   git tag -a v0.1.0 -m 'Flik v0.1.0'
   git push origin v0.1.0
   ```

6. The tag triggers four native builds. Every target runs `cargo test --locked --all-targets` and `cargo build --locked --release`, then verifies `flik --version`. The release job starts only after all four builds succeed.
7. After every target has passed its tests and binary checks, GitHub publishes a stable release with the four archives and `SHA256SUMS`, marked as latest. The installer and website can then use it anonymously. Download and try the published assets on representative machines as a follow-up check.
8. The website repository setting is already `torxx666/flik`; its download links activate when the published release contains the matching archives. Avoid inserting a personal account name or repository URL until it has been explicitly selected for publication.

The workflow checks that the tag equals the package version and accepts only `vMAJOR.MINOR.PATCH`. It does not create a repository, push a tag, publish a release, or modify DNS merely because the workflow file exists. Pushing a version tag authorizes the workflow to publish the tested release automatically. If a release for the same tag already exists, a repeat creation fails instead of silently replacing its assets. Publish fixes under a new version tag.

The default workflow token is read-only. Only the final release job has `contents: write`; no personal token is needed. Actions are pinned to full commit SHAs verified in their official repositories. Source checkout credentials are not persisted. Local checkout paths are remapped out of release binary diagnostics.

## User installation

The selected repository is `torxx666/flik`. Installation becomes available after its first stable release is published.

Download the installer from a reviewed release tag, inspect it, then run it:

```sh
curl --proto '=https' --tlsv1.2 -fL \
  https://raw.githubusercontent.com/torxx666/flik/v0.1.0/scripts/install.sh \
  -o install-flik.sh
# Read install-flik.sh before running it.
FLIK_REPO=torxx666/flik FLIK_VERSION=v0.1.0 sh install-flik.sh
```

To resolve the latest published stable release instead:

```sh
FLIK_REPO=torxx666/flik sh install-flik.sh
```

The installer detects the operating system and CPU architecture; downloads an archive and its checksum file over HTTPS; checks SHA-256 and the executable version; and installs to `$HOME/.local/bin/flik`, retaining the license beside it as `$HOME/.local/bin/flik.LICENSE`. It uses no `sudo`, changes no shell profile and sends no telemetry. It needs standard Unix tools, `curl`, `tar`, and either `sha256sum` or `shasum`.

It refuses to overwrite an existing executable by default. An intentional update is:

```sh
FLIK_REPO=torxx666/flik FLIK_VERSION=v0.1.1 FLIK_FORCE=1 sh install-flik.sh
```

Choose another absolute installation directory with `FLIK_INSTALL_DIR`. If the directory is not in `PATH`, the installer prints where it installed Flik; add it to your shell configuration or invoke the absolute executable path. Files named `flik` or `flik.LICENSE` that are symbolic links are never replaced by this installer.

Remove it with `rm "$HOME/.local/bin/flik" "$HOME/.local/bin/flik.LICENSE"` if you used the default location. The installer does not register background services or install dependencies.

### Manual download

From GitHub Releases, download the archive matching the platform table plus `SHA256SUMS`. For example:

```sh
# Linux x86-64, after downloading these two files into the current directory:
grep '  flik-v0.1.0-x86_64-unknown-linux-musl.tar.gz$' SHA256SUMS | sha256sum --check -
tar -xzf flik-v0.1.0-x86_64-unknown-linux-musl.tar.gz
./flik --version
./flik /path/to/app.log
```

On macOS, use `shasum -a 256 flik-v0.1.0-aarch64-apple-darwin.tar.gz` and compare its full hash with the corresponding line in `SHA256SUMS` before extracting. Substitute the Intel suffix on Intel Macs.

A checksum fetched from the same release detects corruption or mismatched assets. It is **not** an independent publisher signature. Code signing and signed attestations can be added later; do not describe the current builds as notarized, signed by a verified publisher, or independently reproducible.

## macOS signing

The prepared workflow does not contain Apple Developer ID credentials or notarization credentials. The compiler may apply an ad hoc signature required by the platform; this is not a Developer ID signature or notarization. macOS security policy may block downloaded executables. The installer does not disable Gatekeeper or remove quarantine attributes. If you trust the release after verification, follow the operating system's per-app approval flow when offered, or build from reviewed source. The polished distribution path is to add Developer ID signing, hardened runtime where applicable, notarization, and stapling with separately managed release secrets.

## Local verification and limits

```sh
sh -n scripts/install.sh
python3 scripts/test_install.py
cargo test --locked --all-targets
```

`test_install.py` uses synthetic local archives and mocked HTTPS downloads. It verifies all four platform selections, latest and pinned versions, checksum and binary-version failures, invalid arguments, cleanup, existing-file protection, intentional replacement, and symlink protection. It never contacts GitHub or installs into a real home directory. The native CI build matrix must still run in the selected GitHub repository before claiming that all release platforms have passed.

## References

- [GitHub-hosted runner labels and architectures](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job)
- [Pinning actions to commit SHAs](https://docs.github.com/en/actions/reference/security/secure-use)
- [GitHub CLI release creation, drafts and tag verification](https://cli.github.com/manual/gh_release_create)
- [Rust ARM64 musl target](https://doc.rust-lang.org/rustc/platform-support/aarch64-unknown-linux-musl.html)
- [Apple's guidance for opening unverified apps](https://support.apple.com/en-au/102445)
