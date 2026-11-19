# Cutting a release

Everything a release needs that a machine cannot do on its own is written down
here: the signing identities, the repository secrets, the manual passes, and the
order things have to happen in.

The rule of the whole document: **the workflow builds and signs; a person
publishes.** A release that nobody installed is not a release.

---

## 1. What a release is

| | |
|---|---|
| Trigger | a tag `v<version>`, pushed to `main` |
| Build | `.github/workflows/release.yml` |
| Artifacts | `Worktree-Cleaner_<version>_macos_universal.dmg`, `Worktree-Cleaner_<version>_macos_universal.app.tar.gz` (+ `.sig`), `Worktree-Cleaner_<version>_windows_x86_64-setup.exe` (+ `.sig`), `latest.json`, `SHA256SUMS` |
| Release | a **draft** on the Releases page; publishing it is a manual step |
| Update | the app checks `latest.json` at launch, names the version it found, and installs it on request (D25) |

The macOS build is universal: one archive answers for Apple silicon and for
Intel. The updater asks for the key of the architecture it runs on, and the
manifest publishes the same file under both keys.

---

## 2. Before the first release: the signing identities

### 2.1 The updater key

Already generated, and it lives outside the repository:

```
~/.tauri/worktree-cleaner.key        the private key
~/.tauri/worktree-cleaner.key.pub    the public key
```

The public key is in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
**A published public key cannot be changed**: an app shipped with one key refuses
every update signed with another, so the users of that build would have to
reinstall by hand. If the key is lost, the update path is over for the builds
that carry it.

The private key goes into the repository as a secret, and nowhere else:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/worktree-cleaner.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ''   # or the real password
```

The key as generated has no password. That is acceptable for a key that lives
only in the repository's secret store, and it is worth giving it one if the
account allows it — a regenerated key means a new public key, so the change has
to happen before the first release, not after.

### 2.2 The Apple Developer ID identity and the notary key

The app is signed with a **Developer ID Application** identity and notarized,
or macOS will not open it on somebody else's machine.

The team already owns that identity, with its private key. **Do not create a
second certificate for this app**: one Developer ID Application identity signs
every app of the team, and a redundant certificate is a second key to protect and
eventually revoke. On a Mac that signs interactively it is enough that the
identity is in the login keychain:

```bash
security find-identity -v -p codesigning | grep "Developer ID Application"
```

Notarization uses an App Store Connect **team API key**, not an Apple ID with an
app-specific password: a key does not break when a password changes, has no 2FA,
and can be revoked on its own. The key id, the issuer id and the `.p8` are kept
outside this repository, in the team's own credential store. Never copy the
`.p8` into this repository, a workflow file, or a shell profile.

`scripts/release-macos-dmg.sh` takes them from the environment:

| Variable | Value |
|---|---|
| `APPLE_API_KEY_PATH` | a decrypted `.p8`, mode `600`, on a temporary path |
| `APPLE_API_KEY` | the key id |
| `APPLE_API_ISSUER` | the issuer id |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: … (TEAMID)` |

Without them the script stops before it builds. A build without them is
unsigned, and Gatekeeper refuses it on every machine but the one that made it.


### 2.3 The Windows code-signing certificate

Windows Defender SmartScreen warns about an unsigned installer, and the warning
is the difference between "install it" and "do not install this". A signing
certificate is bought from a certificate authority; this repository does not
have one yet, and the workflow does not require it to produce a working
installer.

To use one, either

- put it in the Windows certificate store on the runner and set
  `bundle.windows.certificateThumbprint` in `src-tauri/tauri.windows.conf.json`,
  or
- use a signing service and set `bundle.windows.signCommand`.

Both are documented under `bundle > windows` in the Tauri configuration
reference. Until then, the Windows installer works and says nothing about who
published it.

---

## 3. Cutting the release

Two ways to produce the same DMG. Both run `scripts/release-macos-dmg.sh`; the
only difference is where the credentials come from.

### 3.1 On a Mac, with the credentials in the environment

The one command that is a release:

```bash
./scripts/release-macos-dmg.sh
```

It refuses to start unless the four `APPLE_*` variables of §2.2 are set and the
`.p8` is mode `600`. Then, in order: checks the tree is clean and the version
agrees in `package.json`, `src-tauri/tauri.conf.json` and `Cargo.toml`; runs the
gate (`pnpm test`, `pnpm lint:rust`); builds a universal app with
`pnpm tauri build --target universal-apple-darwin`; lets Tauri sign and notarize
the app and staple it; notarizes and staples the DMG; verifies both with
`codesign`, `stapler` and `spctl`; and collects the result into `release/` with
a `SHA256SUMS` beside it.

Flags, for the cases that need them:

| Flag | Effect |
|---|---|
| `--skip-gate` | skip `pnpm test` and `pnpm lint:rust` (CI runs them in its own job) |
| `--updater` | also build `Worktree-Cleaner_<version>_macos_universal.app.tar.gz` and its `.sig`; needs `TAURI_SIGNING_PRIVATE_KEY` |
| `--allow-dirty` | build from a tree with local changes, for a trial only |
| `--out <dir>` | collect somewhere other than `release/` |

The script never publishes. A tag, a push and a draft release stay separate,
deliberate steps.

On a Mac that holds the credentials, stage them for the duration of that one
command rather than exporting them into a shell. The team keeps a small wrapper
that decrypts the `.p8` into a mode-`600` temporary file, exports the three
`APPLE_API_KEY*` variables plus the signing identity, and deletes the file when
the command exits — on success, failure and interrupt alike. It lives with the
credentials, not here; its own documentation has the invocation.

### 3.2 From a tag, in CI

```bash
# 1. The version, in the one file that writes it down.
$EDITOR package.json            # "version": "0.1.0" -> "0.2.0"

# 2. The same number in the other manifests, checked by the test.
#    src-tauri/tauri.conf.json  and  Cargo.toml  ([workspace.package] version)

# 3. The gate.
pnpm test && pnpm lint:rust && pnpm build

# 4. The commit and the tag.
git add package.json src-tauri/tauri.conf.json Cargo.toml
git commit -m "chore: release 0.2.0"
git tag v0.2.0
git push origin main && git push origin v0.2.0
```

Pushing the tag starts the workflow. It builds both platforms, signs them,
writes `latest.json` and creates a **draft** release.

A release can also be built by hand from the workflow page ("Run workflow"),
which is the way to rebuild a tag that already exists.

The workflow signs only once the secrets of §3.3 exist. Until then a tagged
build stops at a step that says which value is missing, rather than producing an
unsigned DMG that Gatekeeper rejects on every other machine.

### 3.3 What CI needs before it can sign

The runner starts with no keychain and no `.p8`. Instead of copying the
certificate into a secret, the job reads the team's existing Match storage and
notary key from private repositories pinned to a commit, and builds a temporary
keychain for the length of the build. One private key, one place.

What this buys and what it costs: no second copy of the Developer ID key exists,
and the repository names and pins below are public — they already are, in the
public release workflow of the team's other macOS app. What is *not* public is
any value: the secrets API answers `403` without admin, GitHub masks secret
values in logs, and a `pull_request` run of this repository cannot read the
`mac-release` environment at all.

| Secret | What it is |
|---|---|
| `RELEASE_CREDENTIALS_DEPLOY_KEY` | read-only deploy key for the credential repository |
| `APPLE_SIGNING_MATCH_DEPLOY_KEY` | read-only deploy key for the Match storage |
| `RELEASE_AGE_IDENTITY` | the age identity of this repository's CI recipient |
| `TAURI_SIGNING_PRIVATE_KEY` | the updater key of §2.1 — required, see below |

The credential repository holds one ciphertext pair per consumer, encrypted to
that consumer's recipient: the release Mac's pair, the original CI pair, and now
`AuthKey_JG5HB3CLJ3.p8.worktree-cleaner.age` plus
`match-password.worktree-cleaner.age` for this repository. A compromised or
retired consumer is removed by deleting its pair and its recipient, without
re-encrypting anything for the others.

The updater key is not optional here. `main.ts` calls `checkForUpdates()` at
launch, so a build without a signed updater archive and a published `latest.json`
ships an app whose update path fails every time it starts. The macOS job therefore
always runs the script with `--updater`, and the publish job writes `latest.json`
from the `.sig` the build produced.

```bash
gh secret set RELEASE_AGE_IDENTITY --env mac-release < /path/to/ci-age-identity.txt
gh secret set RELEASE_CREDENTIALS_DEPLOY_KEY --env mac-release < /path/to/credentials_deploy_key
gh secret set APPLE_SIGNING_MATCH_DEPLOY_KEY --env mac-release < /path/to/match_deploy_key
gh secret set TAURI_SIGNING_PRIVATE_KEY --env windows-release < ~/.tauri/worktree-cleaner.key
```

The `mac-release` environment already exists and accepts only `v*` tags. Keep it
that way: an unprotected environment on a public repository is a secret-leak
path, not a formality.

The Windows job has its own environment, `windows-release`, holding only
`TAURI_SIGNING_PRIVATE_KEY` and also restricted to `v*` tags. It needs the
updater key to sign its installer and nothing else the release holds, so the
Apple credentials stay readable only by the macOS job. A job without an
environment sees no secrets at all: the bundle step then stops with *"A public
key has been found, but no private key"* and the publish job never runs.

Two properties of this route are worth knowing before the first run:

- **It depends on Apple serving its intermediate certificates.** `fastlane match`
  downloads `https://www.apple.com/certificateauthority/AppleWWDRCAG*.cer` and
  imports them. Those four URLs answer `403` from some networks, even though
  `https://www.apple.com/` itself answers `200`. The workflow checks them first
  and fails with a message that says so, instead of the misleading
  `Could not install WWDR certificate`.
- **It does not run on this Mac.** The identity is already in the login keychain
  here, so local packaging never calls Match. A local Match failure says nothing
  about the workflow.

The workflow pins the credential repository and the Match storage to exact
commits (`env:` under the `macos` job). Bump both when either repository changes,
or the run keeps using the old script.

The macOS job then runs the same script a person runs:

```bash
./scripts/release-macos-dmg.sh --skip-gate
```

— with `CODE_SIGN_IDENTITY` and the notary key supplied by the isolated-keychain
wrapper, which the script accepts in place of `APPLE_SIGNING_IDENTITY` and
`APPLE_API_KEY_PATH`.

---

## 4. Before the draft is published

The checks a machine cannot do. Each one has been done at least once before a
release, and each one has caught something.

### 4.1 The macOS app

- [ ] Download the `.dmg` from the draft. It shows the license agreement of
      `bundle.licenseFile` before it mounts — accepting it is the first click a
      user meets, and `hdiutil attach` cancelled by that screen is why an
      automated mount of this image fails.
- [ ] Drag the app to **Applications**. It opens without a warning about an
      unknown developer — that is what notarization is for.
- [ ] `spctl -a -vv "/Applications/Worktree Cleaner.app"` says `accepted` and
      names the team.
- [ ] The app scans a real repository and the verdicts are the ones the
      repository's own tests expect.
- [ ] The **Trash** path works: delete one safe worktree, find it in the Trash,
      and put it back. This is D5 and it is checked by hand on every release.

### 4.2 The Windows app

- [ ] Download the `.exe`, install it as the current user, and run it.
- [ ] The **Recycle Bin** path works: delete one safe worktree and find it in
      the Recycle Bin. This is D16, and it is still open in
      `docs/DEV_CONTRACT.md` §7 — no Windows machine has run it yet. Until one
      does, the Windows release is the one carrying the untested path.
- [ ] The app scans a real repository.

### 4.3 The update path

- [ ] Publish the release, then install the published build on a machine that
      has an older one.
- [ ] The old build says an update is available, names the version, downloads
      it, installs it and restarts into the new one.
- [ ] The update is refused when the signature does not match: change one byte
      of the `.sig` in a copy of the feed and check that the app declines to
      install it.

---

## 5. After the release

- [ ] The draft is published (a draft is invisible to the updater).
- [ ] `latest.json` is reachable at
      `https://github.com/GetSayAll/worktree-cleaner/releases/latest/download/latest.json`
      and carries the three platform keys: `darwin-aarch64`, `darwin-x86_64`,
      `windows-x86_64`.
- [ ] The entry in `CHANGELOG.md` names what changed and what had to be done by
      the reader, if anything.

---

## 6. The gate that has to be green

| Command | What it covers |
|---|---|
| `pnpm test` | the front-end tests, the locale tables, the version, the update state machine, the release script |
| `pnpm lint:rust` | `cargo fmt --check` and `cargo clippy --workspace --all-targets` |
| `pnpm build` | `vue-tsc --noEmit` and the production build |
| `pnpm test:layout` | the page grid in a real browser |
| `pnpm test:theme` | the light and dark palettes in a real browser |

`.github/workflows/ci.yml` runs all five, plus a Windows compile, on every push
and every pull request.
