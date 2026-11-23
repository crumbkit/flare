# Working on Flare's TypeScript packages

This directory is a pnpm workspace. `headless` holds the comment parts'
state, rules and words with no DOM, network or framework. `react` binds them
to React as props and renders nothing. The rules are typescript-standards'
base profile, as [typescript-standards.toml](typescript-standards.toml)
adopts it. Node's version is in the repository's `.node-version`.

Run these from this directory before every push. They are the steps of the
`ui` job in [../.github/workflows/ci.yml](../.github/workflows/ci.yml).

```sh
pnpm install --frozen-lockfile
pnpm run fix
pnpm run lint
pnpm run typecheck
pnpm run knip
pnpm run test
```

- Write the test first, named as a sentence (BASE-18, BASE-24).
- Type tests live in `headless/src/**/*.test-d.ts` and run in `pnpm run test`.
- Pin every dependency to an exact version (BASE-13).
