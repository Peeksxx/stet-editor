# Publishing

Repository: [peeksxx/stet-editor](https://github.com/peeksxx/stet-editor).

The Git repository is the contents of `dist`. Keep local tooling, full tests, logs, and build artifacts in the sibling `working` folder. The source tree includes the small CI validation suite and the notices needed for redistribution.

## First push

Create `peeksxx/stet-editor` as an empty GitHub repository if it does not exist. Leave GitHub's README, license, and `.gitignore` options unchecked so the repository has no separate initial commit.

From `dist`, on `review/stet`, commit the final changes:

```powershell
git add src/app.rs src/ui.rs docs/configuration.md
git diff --cached --stat
git commit -m "Fix search state, reload highlighting, and session persistence"
git add README.md docs/publishing.md
git diff --cached --stat
git commit -m "Polish README and publishing instructions"
git switch main
git merge --ff-only review/stet
git remote add origin https://github.com/peeksxx/stet-editor.git
git push -u origin main
```

These commands match the current local review branches. If `origin` already exists, inspect `git remote -v` and use the existing correct remote. If GitHub already has commits and rejects the push, fetch and reconcile the histories before retrying. Do not force-push to resolve that rejection.

## Release

1. Confirm Windows CI passes. Run the [acceptance checks](acceptance.md), including installation in a disposable VM. Resolve failures before release.
2. Choose a project license if granting reuse rights. Dependency notices do not license Stet itself.
3. Create and push the version tag from the reviewed commit:

```powershell
git tag v1.0.0
git push origin v1.0.0
```

4. In Actions, select **Build release installer**, then **Run workflow**, with `v1.0.0`.
5. Download and unzip `Stet-1.0.0-windows-x64` after the workflow passes. Artifacts follow the repository's access permissions.
6. Create a GitHub release for `v1.0.0`. Attach `Stet-Setup-x64.exe` and `SHA256SUMS.txt`. State that the binaries are unsigned and include any remaining acceptance limits.
7. Verify the published download's checksum and test that installer in a disposable VM.

The workflow has read-only repository permissions and uploads build artifacts. Publishing a release is a separate manual step. Ordinary pushes run CI.
