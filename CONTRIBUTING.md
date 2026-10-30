# Contributing to InstantPad

Thanks for helping out. You can write issues and pull requests in English or Portuguese.

## Before you start

Report bugs with a [bug report](https://github.com/eljonathas/InstantPad/issues/new?template=bug_report.yml). For a new feature, open a [feature request](https://github.com/eljonathas/InstantPad/issues/new?template=feature_request.yml) first so we can agree on it before you write code. Small fixes can go straight to a pull request.

Security problems should never go in a public issue. [SECURITY.md](SECURITY.md) explains how to report them.

## Setting up

You need Windows and the [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0).

```bash
git clone https://github.com/eljonathas/InstantPad.git
cd InstantPad
dotnet test InstantPad.sln
dotnet run --project src/InstantPad.Desktop
```

Local builds report version `0.0.0`, so they never update themselves. The `INSTANTPAD_DATA` environment variable points the app at another folder, which keeps your own sounds out of your tests.

Testing the audio path end to end needs a virtual cable such as VB-CABLE.

## Project layout

Dependencies only point inwards:

| Project | Holds | May depend on |
| --- | --- | --- |
| `InstantPad.Domain` | Pad board and hotkey rules | Nothing |
| `InstantPad.Application` | Use cases, ports and translated text | Domain |
| `InstantPad.Infrastructure` | NAudio, MyInstants, GitHub, JSON files, driver | Application |
| `InstantPad.Desktop` | WPF views, view models, composition root | Everything above |

New behavior starts in `Domain` or `Application`, with tests, and without WPF, NAudio or network types. Code that talks to the outside world goes behind a port in `Application`, with the implementation in `Infrastructure`, wired up in `CompositionRoot.cs`. Code-behind only handles window chrome, drag and drop, and menus.

## Code style

Follow the code around your change. `.editorconfig` covers formatting, and warnings fail the build. Code, comments and identifiers are in English. Comments explain why something is done, and unused code gets removed.

## Translations

All text shown to users lives in `src/InstantPad.Application/Localization`. English is in `Strings.resx` and Portuguese in `Strings.pt.resx`. Add every new text to both, in sentence case. Use `{l:T Key}` in XAML and `Strings.Key` in C#.

To add a language, create `Strings.<language>.resx` and add it to the options in `SetupViewModel`. A test checks that all languages have the same keys and placeholders.

## Tests

Every behavior change needs a test in `tests/InstantPad.Tests`. Name tests as plain sentences, such as `AGridThatCannotFitTheSoundsIsRefused`. Tests must not need audio devices, a network connection or user data.

## Commits and pull request titles

We use [Conventional Commits](https://www.conventionalcommits.org/) in English:

```
<type>[optional scope]: <description>
```

Pull requests are squash merged, so the title becomes the commit on `main` and decides the next version:

| Title | Example | Release |
| --- | --- | --- |
| `feat:` | `feat: add pad volume fade` | Minor |
| `fix:` | `fix(audio): reopen the mic after a device change` | Patch |
| `feat!:` or a `BREAKING CHANGE:` footer | `feat!: change the library file format` | Major |
| `docs:`, `style:`, `refactor:`, `perf:`, `test:`, `build:`, `ci:`, `chore:`, `revert:` | `docs: explain the data folder` | Patch |

Write the description in the imperative mood, in lowercase, with no period at the end. CI rejects titles in any other format.

Every merge to `main` publishes a release, so `main` must always be ready to ship.

## Pull requests

1. Fork the repository and branch from `main`.
2. Keep the pull request to one change.
3. Run `dotnet test InstantPad.sln`.
4. Open the pull request and fill in the template. Visual changes need a screenshot.

A maintainer reviews every pull request, and CI must pass before merging.

## License

By contributing, you agree that your work is licensed under the [MIT License](LICENSE). Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
