# Dependencies

Versions are pinned in `Cargo.lock`. Runtime code makes no network requests.

The local advisory check queried OSV for all 134 registry packages in the lockfile on 2026-10-31. It returned two maintenance notices from Syntect's dependency tree:

| Package | Advisory | Status |
|---|---|---|
| bincode 1.3.3 | [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141.html) | Unmaintained; no patched version |
| yaml-rust 0.4.5 | [RUSTSEC-2024-0320](https://rustsec.org/advisories/RUSTSEC-2024-0320.html) | Unmaintained; no patched version |

Stet uses these through Syntect to load bundled syntax and theme data. It does not accept external syntax dumps, themes, or YAML files. These notices concern maintenance and are not a claim of an exploitable vulnerability in Stet. Recheck advisories before publishing and when updating Syntect.

`THIRD_PARTY_NOTICES.txt` retains dependency license texts. [Rust standard library notices](RUST_NOTICES.html) retain the toolchain copyright inventory. Those licenses do not select a license for Stet itself.
