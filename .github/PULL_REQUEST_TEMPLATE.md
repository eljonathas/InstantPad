<!--
The title becomes the commit on main and decides the next version, so it must follow
Conventional Commits in English, for example: feat: add pad volume fade
CONTRIBUTING.md lists the types.
-->

## What changed

<!-- What does this do, and why? -->

Closes #

## How you tested it

<!-- Which tests cover it? What did you try in the running app? -->

## Screenshots

<!-- Needed for visual changes. Delete this section otherwise. -->

## Checklist

- [ ] The title follows Conventional Commits, in English
- [ ] `dotnet test InstantPad.sln` passes
- [ ] New behavior has tests
- [ ] Domain and Application stay free of WPF, NAudio and network types
- [ ] New text is in both `Strings.resx` and `Strings.pt.resx`, in sentence case
- [ ] The README and other docs are up to date
