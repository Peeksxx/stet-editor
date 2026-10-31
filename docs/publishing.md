# Publish manually

Nothing in the build scripts commits or publishes. The release workflow only uploads private workflow artifacts. A maintainer publishes the release through the website.

1. Review this source folder, `Cargo.lock`, CI, and the installer. Run the README checks and manual acceptance. Choose a license before granting reuse rights; none has been selected.
2. On GitHub, create an empty repository named `stet`. Do not add a README or license through the website.
3. In this source folder, run:

```powershell
git init
git branch -M main
git add .
git diff --cached --stat
git diff --cached
git commit -m "Initial release of Stet"
git remote add origin https://github.com/YOUR-USERNAME/stet.git
git push -u origin main
```

`YOUR-USERNAME` is a placeholder. Stage only the contents of `dist`, never its parent workspace or `working` folder.

4. Confirm Windows CI passes in Actions. Resolve failures before releasing.
5. Create the version tag locally and push it after review:

```powershell
git tag v1.0.0
git push origin v1.0.0
```

6. In Actions, select **Build release installer**, then **Run workflow**, with `v1.0.0`. Download and unzip `Stet-1.0.0-windows-x64` after it passes.
7. In Releases, create a release for `v1.0.0`. Upload `Stet-Setup-x64.exe` and `SHA256SUMS.txt`. State that the build is unsigned and include any remaining acceptance limitations.
8. Publish when ready. Open the release page in a signed-out browser, download the installer, verify its checksum, and test installation in a disposable VM.

The workflow has read-only repository permissions and cannot publish a release. Ordinary pushes run CI only.
