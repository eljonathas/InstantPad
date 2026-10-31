# Contributing to InstantPad

Thanks for helping out. You can write issues and pull requests in English or Portuguese.

## Before you start

Report bugs with a [bug report](https://github.com/eljonathas/InstantPad/issues/new?template=bug_report.yml). For a new feature, open a [feature request](https://github.com/eljonathas/InstantPad/issues/new?template=feature_request.yml) first so we can agree on it before you write code. Small fixes can go straight to a pull request.

Security problems should never go in a public issue. [SECURITY.md](SECURITY.md) explains how to report them.

## Setting up

You need [Rust](https://rustup.rs), Node 22 and [pnpm](https://pnpm.io). On Linux you also need the [Tauri system libraries](https://tauri.app/start/prerequisites/).

```bash
git clone https://github.com/eljonathas/InstantPad.git
cd InstantPad
pnpm install
pnpm dev
```

`pnpm --filter @instantpad/desktop dev` runs just the interface in a browser, against a mock of the backend. The `INSTANTPAD_DATA` environment variable points the app at another folder, which keeps your own sounds out of your tests.

Testing the audio path end to end needs a virtual cable: VB-CABLE on Windows, BlackHole on macOS.

## Project layout

Dependencies only point inwards:

| Folder | Holds | May depend on |
| --- | --- | --- |
| `crates/domain` | Pad board and hotkey rules | Nothing of ours |
| `crates/application` | Use cases, ports and all app state | `domain` |
| `crates/audio`, `catalog`, `storage`, `platform` | Adapters for the ports | `application` |
| `apps/desktop/src-tauri` | Commands, events, composition root | Everything above |
| `apps/desktop/src` | React interface | `src/shared/ipc` only |

New behavior starts in `domain` or `application`, with tests. Code that talks to the outside world goes behind a port in `crates/application/src/ports.rs`, with the implementation in an adapter, wired up in `composition.rs`. A test fails if `domain` or `application` reach Tauri, cpal, reqwest or Windows. The interface draws what the backend sends and holds no rules of its own.

The audio callbacks must not allocate, free, block or do I/O. A test with a counting allocator guards this.

After changing a command or an event, run `pnpm gen:ipc` in `apps/desktop` to rewrite the TypeScript bindings. A test fails if they are out of date.

## Code style

Follow the code around your change. Code, comments and identifiers are in English. Comments explain why something is done, and unused code gets removed. Every color and measure of the interface is a token in `tokens.css`.

## Translations

All text shown to users lives in `apps/desktop/src/shared/i18n`, in `en.json` and `pt.json`. Add every new text to both, in sentence case. The backend sends a `TextKey` and its arguments, and the interface renders them.

To add a language, create its JSON file and add it to `shared/i18n/index.ts` and to the options in the settings screen. A test checks that all languages have the same keys and placeholders.

## Tests

Every behavior change needs a test. Name tests as plain sentences, such as `a_grid_that_cannot_fit_the_sounds_is_refused`. Tests must not need audio devices, a network connection or user data.

An adapter must also pass the contract tests of its port, in `crates/test-support/src/contract.rs`.

## Commits and pull request titles

We use [Conventional Commits](https://www.conventionalcommits.org/) in English:

```
<type>[optional scope]: <description>
```

Pull requests are squash merged, so the title becomes the commit on `main`. CI rejects titles that do not follow this format. Use `feat!:` or a `BREAKING CHANGE:` footer for a breaking change.

Write the description in the imperative mood, in lowercase, with no period at the end.

Releases are started by hand with the `Release desktop` workflow, which leaves a draft to review. `main` must always be ready to ship.

## Pull requests

1. Fork the repository and branch from `main`.
2. Keep the pull request to one change.
3. Run `cargo test --workspace` and `pnpm check && pnpm lint && pnpm test`.
4. Open the pull request and fill in the template. Visual changes need a screenshot.

A maintainer reviews every pull request, and CI must pass before merging.

## License

By contributing, you agree that your work is licensed under the [MIT License](LICENSE). Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
