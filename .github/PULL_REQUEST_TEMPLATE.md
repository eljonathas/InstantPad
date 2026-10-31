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
- [ ] `cargo test --workspace` and `pnpm check && pnpm lint && pnpm test` pass
- [ ] New behavior has tests
- [ ] `domain` and `application` stay free of Tauri, cpal, reqwest and Windows types
- [ ] New text is in both `en.json` and `pt.json`, in sentence case
- [ ] The README and other docs are up to date
