# Release runbook

Goal: users add `setreplace` to their dependencies and `pip install` gets a
prebuilt wheel on macOS (arm64 + x86_64 via universal2), Linux (manylinux
x86_64 + aarch64), and Windows (x64), for every Python ≥ 3.9 (abi3 — one
wheel per platform). Other platforms fall back to the sdist, which compiles
with any stable Rust toolchain (verified: `pip install <sdist>` builds the
whole workspace and passes the test suite).

## One-time setup (requires the PyPI account owner)

PyPI **Trusted Publishing** lets the GitHub workflow publish with no API
tokens. On [pypi.org](https://pypi.org) → your account → *Publishing* →
*Add a new pending publisher*, enter exactly:

| field | value |
|---|---|
| PyPI project name | `setreplace` |
| Owner | `kovasb` |
| Repository name | `setreplace-py` |
| Workflow name | `release.yml` |
| Environment name | `pypi` |

A "pending" publisher covers the very first release — the PyPI project is
created by the first trusted publish. Optionally also create a GitHub
environment named `pypi` (repo → Settings → Environments) and restrict it to
tags, for an approval gate.

## Cutting a release

1. Bump the version in **both** [python/pyproject.toml](../python/pyproject.toml)
   (`[project] version`) and [python/Cargo.toml](../python/Cargo.toml) —
   pyproject is the one that matters for the wheel; keep them in sync.
2. Commit, then tag and push:

   ```bash
   git tag v0.1.0
   git push && git push --tags
   ```

3. The tag triggers [release.yml](../.github/workflows/release.yml):
   wheels for all platforms (with an install-and-evolve smoke test on each
   native runner) + the sdist, then a single publish job uploads everything
   to PyPI via OIDC.

That's it — once the workflow is green, `pip install setreplace` works
everywhere, and projects can list `setreplace` in their dependencies.

## Notes

- [ci.yml](../.github/workflows/ci.yml) runs on every push/PR: rustfmt,
  clippy (warnings denied), the full Rust test suite, and a from-source
  `pip install ./python` + Python API tests on Python 3.9 (the minimum).
- The sdist contains the entire Cargo workspace (maturin packages local path
  dependencies automatically).
- Publishing the Rust crates to crates.io is independent of PyPI and not
  required for Python users; if/when wanted: add `version` to the path
  dependencies, `repository`/`readme` metadata, then
  `cargo publish -p setreplace` followed by `-p setreplace-viz`.
