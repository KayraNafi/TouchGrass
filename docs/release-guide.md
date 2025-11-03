## TouchGrass Release Checklist

Cutting a new TouchGrass version touches both the desktop bundle and the auto-update channel. Follow the steps below each time you ship.

### 1. Prepare locally

1. Pick the new semantic version.
2. Update every version reference:
   - `src-tauri/tauri.conf.json` → `"version"`.
   - Any documentation or UI copy that hard-codes the version (search for the previous number).
3. Commit the changes (e.g. `chore: bump version to vX.Y.Z`).

### 2. Tag and push

1. Create a signed tag: `git tag vX.Y.Z` (prefix with `v` to match the workflow trigger).
2. Push the tag: `git push origin vX.Y.Z`.

This kickstarts the GitHub Actions release workflow.

### 3. What the workflow does

- Builds platform packages (Debian `.deb`, Fedora `.rpm`, Windows `.msi` + `.zip`).
- Generates and signs `bundle/updater/latest.json` for each job.
- Merges the manifests, attaches all artifacts, and publishes a GitHub Release named after the tag.

All signing keys must be present as repository secrets:

- `TAURI_PRIVATE_KEY`
- `TAURI_KEY_PASSWORD` (optional; required if you set one)
- `TOUCHGRASS_UPDATER_PUBKEY`

### 4. Post-release checks

Inside the GitHub Release verify:

- The generated notes look right (edit manually if needed).
- `.deb`, `.rpm`, `.msi`, `.zip`, and `latest.json` are attached.
- The release is marked as draft/prerelease as expected (depends on tag naming; tags containing `-` mark prereleases).

Optional: download/install each artifact on its platform as a smoke test.

### 5. Ship the announcement (optional)

- Update docs, changelog, or marketing copy.
- Share the release link on your channels.

---

Need to regenerate signing keys? See `README.md > Auto updates`. Rotate the secrets, update `TOUCHGRASS_UPDATER_PUBKEY`, and ship a new release so clients pick up the new signature.
