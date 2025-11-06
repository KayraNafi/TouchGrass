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

- Builds platform packages (Debian `.deb`, Fedora `.rpm`, Windows `.msi` + update `.msi.zip`, Linux `.AppImage`).
- Collects the signed updater payloads (`*.AppImage.tar.gz`, `*.msi.zip`) and their `.sig` files.
- Synthesizes `latest.json` from those artifacts and publishes a GitHub Release named after the tag.

All signing keys must be present as repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY` - The complete content of your private key file (both lines, exactly as-is)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Your private key password (required if you set one)

The updater public key is committed in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.

### 4. Post-release checks

Inside the GitHub Release verify:

- The generated notes look right (edit manually if needed).
- `.deb`, `.rpm`, `.AppImage`, `.AppImage.tar.gz`, `.msi`, `.msi.zip`, signatures, and `latest.json` are attached.
- The release is marked as draft/prerelease as expected (depends on tag naming; tags containing `-` mark prereleases).

Optional: download/install each artifact on its platform as a smoke test.

### 5. Ship the announcement (optional)

- Update docs, changelog, or marketing copy.
- Share the release link on your channels.

---

## Signing Key Setup (One-Time)

If you need to generate or regenerate signing keys:

1. **Generate keypair:**
   ```bash
   pnpm tauri signer generate -w ~/.tauri/touchgrass.key
   ```
   Enter a password when prompted and save it securely.

2. **Get the public key:**
   ```bash
   cat ~/.tauri/touchgrass.key.pub
   ```
   Copy the entire output and update `src-tauri/tauri.conf.json`:
   ```json
   {
     "plugins": {
       "updater": {
         "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDExMzc3RDQzRDNFMEM0OTMKUldTVHhPRFRRMzAzRVRUWEwrRFgzUHIxczB0TmEwWHRWQmp0ZGFLOFo0a21odmZ2NEtTdkZYZTkK"
       }
     }
   }
   ```

3. **Add GitHub secrets:**
   - Go to Settings → Secrets and variables → Actions
   - Add `TAURI_SIGNING_PRIVATE_KEY`: Paste the complete content of `~/.tauri/touchgrass.key` (both lines)
   - Add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: Your password

**Important:** If you rotate keys, all existing users must update to the new version manually (their auto-updater won't trust the old signature).
