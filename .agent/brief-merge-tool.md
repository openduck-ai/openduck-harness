# Brief: build the i18n fragment merge tool + wire it into package.json

Repo: /mnt/e/openduck-harness. Area: `ui/hub`.

You may ONLY create/modify these 3 files:
1. `ui/hub/scripts/i18n-merge.mjs`   (new)
2. `ui/hub/scripts/i18n-check.mjs`   (new)
3. `ui/hub/package.json`             (add exactly two scripts, change nothing else)

Do NOT modify `ui/hub/src/**`, `ui/hub/src/locales/*.json`, lockfiles, root `package.json`,
or any test. Do not run `pnpm install`, git commands, or cargo.

## Context
Several agents are localizing the Hub in parallel. Each writes a nested-JSON *fragment*:
`.agent/i18n-fragments/<OWNER>.en.json` and `.agent/i18n-fragments/<OWNER>.zh.json`
(paths relative to repo root `/mnt/e/openduck-harness`).

`ui/hub/src/locales/en.json` and `zh.json` are the real catalogs consumed at runtime
(read `ui/hub/src/i18n/catalogs.ts`). Fragments must be merged INTO those catalogs, and the
merge must be safe when several agents run it concurrently.

There are 8 "foundation" keys that must NEVER be deleted or overwritten by a merge:
`app.documentTitle`, `app.errorBoundary.heading`, `app.errorBoundary.message`,
`app.errorBoundary.reload`, `header.projects`, `header.running`, `header.settings`,
`header.themeToggleTitle`, `header.themeToggleLabel`, `header.connectedTitle`,
`header.disconnectedTitle`, `common.cancel`, `common.save`, `common.retry`,
`common.close`, `common.loading`, `language.label`, `language.english`, `language.chinese`.
(Read the current `en.json` to get the real list rather than trusting this summary.)
Implement the protection generically: a `PROTECTED` list of dot-paths that merge never
writes to. Skip a fragment value if its dot-path is protected.

## `ui/hub/scripts/i18n-merge.mjs`
Node ESM, zero dependencies, run from `ui/hub` (`pnpm run i18n:merge`).

Behavior:
1. Resolve repo root as `path.resolve(__dirname, '../../..')` (ui/hub/scripts -> repo root).
2. Read all `*.en.json` / `*.zh.json` in `<root>/.agent/i18n-fragments/` (missing dir or no
   files -> print a notice and exit 0).
3. Validate each fragment: must be a JSON object; all leaves must be strings; reject
   empty-string leaves (throw with the file name and dot-path).
4. Deep-merge all en fragments into one object, all zh fragments into one object.
   If two DIFFERENT fragments define the same dot-path with different values, THROW a clear
   error naming both fragments and the path (do not silently pick a winner).
5. Deep-merge those into the existing catalogs, preserving existing keys, but
   (a) skip PROTECTED dot-paths, (b) overwrite a non-protected existing leaf with the
   fragment value (fragments are newer).
6. Write `en.json` / `zh.json` back as 2-space-indented JSON with a trailing newline.
   Keep keys sorted (localeCompare) so the diff is stable and concurrent merges converge.
   Never reorder without reason: sorting IS the convergence mechanism.
7. Concurrency: wrap the read-modify-write in an exclusive lock file at
   `<root>/.agent/i18n-merge.lock` using `fs.open(lock, 'wx')` retry loop (e.g. 200
   attempts * 50ms, then throw). Use `fs.rm` in a `finally` to release. Also handle a stale
   lock: if the lock file is older than 60s, delete it and retry.
8. Parity check before writing: the merged en key set must EQUAL the merged zh key set.
   If it differs, print the symmetric difference clearly and exit 1 WITHOUT writing
   (so a worker can fix its fragment). Also verify placeholder parity: for every shared
   leaf, the set of `{{name}}` placeholders must be identical in en and zh; report violations.
9. Print a one-line summary: number of en keys, zh keys, and how many changed.

## `ui/hub/scripts/i18n-check.mjs`
Standalone validator (`pnpm run i18n:check`), same helpers:
- en/zh key-set equality, placeholder parity, no empty leaves, and (WARN only, don't fail)
  any `zh.json` leaf that still byte-equals its `en.json` counterpart AND contains ASCII
  letters only (a hint that a worker forgot to translate something; English loanwords like
  `English`/`English (US)` are fine so warnings are acceptable).
- Fail (exit 1) if `src/locales/zh.json` is missing or if catalogs are not valid JSON.

## `ui/hub/package.json`
Add exactly:
```json
"i18n:merge": "node scripts/i18n-merge.mjs",
"i18n:check": "node scripts/i18n-check.mjs",
```
right after the existing `"test"` script. Do not reformat or reorder anything else.

## Verification (run yourself, report exact output)
```
cd ui/hub && node scripts/i18n-merge.mjs          # no fragments yet -> notice, exit 0
cd ui/hub && node scripts/i18n-check.mjs          # exit 0, en/zh parity OK
```
Then prove the merge works end to end with a THROWAWAY fragment; afterwards restore the
catalogs exactly:
```
cd /mnt/e/openduck-harness
cp ui/hub/src/locales/en.json /tmp/en.bak && cp ui/hub/src/locales/zh.json /tmp/zh.bak
mkdir -p .agent/i18n-fragments
printf '{"zzzProbe":{"a":"Probe {{n}}","b_one":"One","b_other":"Many"}}' > .agent/i18n-fragments/ZZProbe.en.json
printf '{"zzzProbe":{"a":"探针 {{n}}","b_one":"一","b_other":"多"}}' > .agent/i18n-fragments/ZZProbe.zh.json
cd ui/hub && node scripts/i18n-merge.mjs          # should report new keys
cd /mnt/e/openduck-harness && rm .agent/i18n-fragments/ZZProbe.*.json
cp /tmp/en.bak ui/hub/src/locales/en.json && cp /tmp/zh.bak ui/hub/src/locales/zh.json
git diff --stat ui/hub/src/locales/    # MUST be empty: catalogs restored byte-identically
```
Also confirm a mismatched-parity fragment fails loudly: create only `ZZBad.en.json` and check
the merge exits 1, then delete it and restore the catalogs again.
Finally confirm the real app still builds: `cd ui/hub && pnpm run typecheck`.

Report: files created, the verification transcripts, and confirmation `git diff ui/hub/src/locales/`
is empty.
