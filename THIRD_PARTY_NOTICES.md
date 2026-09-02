# Third-Party Software Notices

This project includes or may distribute third-party software components. Those
components remain subject to their respective licenses. The MIT License in the
repository root covers only the original source code of CareerOS; it does not
relicense any component identified below.

## OpenAI Codex CLI

Component: OpenAI Codex CLI

Upstream project: [openai/codex](https://github.com/openai/codex)

Distribution: Each production Tauri bundle includes the Codex CLI binary for
its target platform under `src-tauri/resources/runtime/`. Release automation
downloads the pinned official upstream asset and verifies its SHA256 before
packaging. CareerOS invokes it as a separate process through the Codex App
Server protocol. An explicit
`POSTDOCOS_CODEX_BIN` override can select another user-provided binary; if the
bundled runtime is unavailable, the development/runtime lookup also checks the
documented data directory and common system installation paths.

### Apple Silicon build record

```text
Component: OpenAI Codex CLI
Version: 0.144.3
Upstream tag: rust-v0.144.3
Platform: aarch64-apple-darwin (Apple Silicon)
SHA256: 718724d7221cf1298071ca92411cb74caa8422809154150cedca7b569a4518e3
License: Apache-2.0
Modified: No
```

The `Modified: No` statement was verified by downloading the official
`codex-aarch64-apple-darwin.tar.gz` asset for `rust-v0.144.3`, verifying the
archive SHA256
`249aaf12644add3876e740998cba0eac8d7d175e903add8cbc8d8eaa1f02e2b5`,
and comparing the extracted executable byte-for-byte with the release input.
The macOS packaging step then replaces code-signature metadata with an ad-hoc
signature so the complete application bundle can pass strict local validation;
it does not modify the executable code.

### Windows x64 build record

```text
Component: OpenAI Codex CLI
Version: 0.144.3
Upstream tag: rust-v0.144.3
Platform: x86_64-pc-windows-msvc
SHA256: e5dcc9f9b08102c58596af85345f689a69fd53a87d8d408bdc0fcdaf99fcf6e3
License: Apache-2.0
Modified: No
```

The Windows binary is the official
`codex-x86_64-pc-windows-msvc.exe` asset for `rust-v0.144.3`. Its SHA256 is
checked before every release build.

Copyright: Copyright 2025 OpenAI. The corresponding upstream NOTICE also
retains attribution for code derived from Ratatui.

License files:

- `licenses/openai-codex-LICENSE.txt`
- `licenses/openai-codex-NOTICE.txt`

OpenAI Codex CLI is not licensed under the CareerOS MIT License.

## Typst CLI

Component: Typst CLI

Upstream project: [typst/typst](https://github.com/typst/typst)

Distribution: Each production Tauri bundle includes the Typst CLI binary for
its target platform under `src-tauri/resources/runtime/`. Release automation
downloads the pinned official upstream archive and verifies both its archive
and extracted executable SHA256 before packaging. CareerOS invokes Typst as a
separate process to render application materials.

### Apple Silicon build record

```text
Component: Typst CLI
Version: 0.15.1
Upstream tag: v0.15.1
Platform: aarch64-apple-darwin (Apple Silicon)
SHA256: 7c4a136b377f3689400afe37b4f0fe3528d50faaa55cbb1106ac8a128f86ba1a
License: Apache-2.0
Modified: No
```

The `Modified: No` statement was verified by downloading the official
`typst-aarch64-apple-darwin.tar.xz` asset for `v0.15.1`, verifying the archive
SHA256 `48f62ed034aa3a7978309579ac6ca00045e2ef0da73114e8af27cfd8e74dc05a`,
and comparing the extracted executable byte-for-byte with the release input.
The macOS packaging step then replaces code-signature metadata with an ad-hoc
signature so the complete application bundle can pass strict local validation;
it does not modify the executable code.

### Windows x64 build record

```text
Component: Typst CLI
Version: 0.15.1
Upstream tag: v0.15.1
Platform: x86_64-pc-windows-msvc
SHA256: 081217a463adb006f8894b44227fe4b9c9e91fc85f5463d5948e8370db9bb31e
License: Apache-2.0
Modified: No
```

The executable was extracted without modification from the official
`typst-x86_64-pc-windows-msvc.zip` asset for `v0.15.1`. Release automation
verifies the archive SHA256
`19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736`
and the executable SHA256 shown above.

License files:

- `licenses/typst-LICENSE.txt`
- `licenses/typst-NOTICE.txt`

Typst CLI is not licensed under the CareerOS MIT License. Its upstream NOTICE
contains additional third-party attributions that remain applicable.

## Package-managed dependencies

CareerOS also depends on Rust crates and frontend packages identified by
`src-tauri/Cargo.lock` and `pnpm-lock.yaml`. These dependencies retain their own
licenses and are not relicensed by CareerOS. The direct dependencies resolved
for this repository use permissive MIT, Apache-2.0, ISC, or Unlicense terms.
The resolved Apple Silicon Rust normal/build dependency graph and installed
frontend package metadata showed no GPL, AGPL, LGPL, SSPL, or BUSL license in
the audit performed on 2026-09-02.

This focused notice is not a substitute for a generated software bill of
materials. A future multi-platform public release should generate and archive a
complete dependency notice/SBOM for the exact release lockfiles and target.

## Trademarks and service terms

OpenAI, Codex, ChatGPT, Typst, and other third-party names are used only to
describe compatibility, integration, and upstream origin. No affiliation or
endorsement is implied. Software licenses are separate from cloud-service
entitlements, account eligibility, provider terms, and API usage rights.
