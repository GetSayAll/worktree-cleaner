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

### 2.2 The Apple Developer ID certificate

The app is notarized, or macOS will not open it on somebody else's machine.

1. In the Apple Developer portal, create a **Developer ID Application**
   certificate (not a development certificate).
2. Export it as a `.p12` with a password.
3. Put it in the repository as a secret, base64-encoded:

   ```bash
   base64 -i "Developer ID Application: GetSayAll (TEAMID).p12" | tr -d '\n' \
     | gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD --body '<the p12 password>'
   gh secret set APPLE_SIGNING_IDENTITY --body 'Developer ID Application: GetSayAll (TEAMID)'
   ```

4. Create an **app-specific password** for the Apple ID that owns the team, and
   put it in with the Apple ID and the team id:

   ```bash
   gh secret set APPLE_ID --body 'you@example.com'
   gh secret set APPLE_PASSWORD --body '<the app-specific password>'
   gh secret set APPLE_TEAM_ID --body 'TEAMID'
   ```

Without these, the workflow still builds, but the `.dmg` is unsigned and
Gatekeeper refuses it on any machine but this one.

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

---

## 4. Before the draft is published

The checks a machine cannot do. Each one has been done at least once before a
release, and each one has caught something.

### 4.1 The macOS app

- [ ] Download the `.dmg` from the draft, open it, and drag the app to
      **Applications**. It opens without a warning about an unknown developer —
      that is what notarization is for.
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
