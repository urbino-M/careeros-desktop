# Contributing to CareerOS

Thanks for your interest in CareerOS. Contributions are welcome for bug fixes,
documentation, accessibility, reliability, and provider integrations that fit
the project's scope.

## Development setup

CareerOS is a Tauri 2 desktop application with a React frontend, Rust backend,
SQLite persistence, Codex integration, and Typst-based document generation.

1. Install Node.js 22+, pnpm, Rust, and the platform prerequisites for Tauri.
2. Install dependencies with `pnpm install --frozen-lockfile`.
3. Follow the validation commands in the README before opening a pull request.

The application uses user-provided accounts and credentials at runtime. Never
commit API keys, OAuth secrets, tokens, cookies, personal profiles, databases,
or generated application data.

## Pull requests

- Keep changes focused and explain the user-visible behavior being changed.
- Include or update tests for behavior changes.
- Update documentation when setup, compatibility, or security behavior changes.
- Describe validation results and any known limitations in the pull request.
- Do not include credentials or private user data in screenshots, fixtures, or
  logs.

## License

By contributing, you agree that your contribution is provided under the
project's [MIT License](LICENSE).
