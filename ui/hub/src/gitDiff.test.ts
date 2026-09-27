import assert from 'node:assert/strict'
import test from 'node:test'
import { gitDiffLineClass, parseGitDiff } from './gitDiff.ts'

test('classifies unified diff lines', () => {
  assert.equal(gitDiffLineClass('diff --git a/a.rs b/a.rs'), 'git-diff-meta')
  assert.equal(gitDiffLineClass('--- a/a.rs'), 'git-diff-file')
  assert.equal(gitDiffLineClass('+++ b/a.rs'), 'git-diff-file')
  assert.equal(gitDiffLineClass('@@ -1,2 +1,3 @@'), 'git-diff-hunk')
  assert.equal(gitDiffLineClass('+added'), 'git-diff-add')
  assert.equal(gitDiffLineClass('-removed'), 'git-diff-del')
  assert.equal(gitDiffLineClass(' context'), 'git-diff-context')
})

test('parseGitDiff parses empty and whitespace diffs', () => {
  assert.deepEqual(parseGitDiff(''), [])
  assert.deepEqual(parseGitDiff('   \n  '), [])
})

test('parseGitDiff parses multi-file diffs with additions, deletions and status', () => {
  const multiDiff = `diff --git a/src/app.ts b/src/app.ts
index abc1234..def5678 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,4 +1,5 @@
 import React from 'react'
-const old = 1
+const next = 2
+const extra = 3
 export default App
diff --git a/src/newFile.ts b/src/newFile.ts
new file mode 100644
index 0000000..9999999
--- /dev/null
+++ b/src/newFile.ts
@@ -0,0 +1,2 @@
+export const created = true
+export const version = '1.0'
diff --git a/old-name.ts b/renamed.ts
similarity index 100%
rename from old-name.ts
rename to renamed.ts
diff --git a/removed.ts b/removed.ts
deleted file mode 100644
--- a/removed.ts
+++ /dev/null
@@ -1,2 +0,0 @@
-line1
-line2`

  const files = parseGitDiff(multiDiff)
  assert.equal(files.length, 4)

  assert.equal(files[0].path, 'src/app.ts')
  assert.equal(files[0].status, 'modified')
  assert.equal(files[0].additions, 2)
  assert.equal(files[0].deletions, 1)

  assert.equal(files[1].path, 'src/newFile.ts')
  assert.equal(files[1].status, 'added')
  assert.equal(files[1].additions, 2)
  assert.equal(files[1].deletions, 0)

  assert.equal(files[2].path, 'renamed.ts')
  assert.equal(files[2].oldPath, 'old-name.ts')
  assert.equal(files[2].status, 'renamed')
  assert.equal(files[2].additions, 0)
  assert.equal(files[2].deletions, 0)

  assert.equal(files[3].path, 'removed.ts')
  assert.equal(files[3].status, 'deleted')
  assert.equal(files[3].additions, 0)
  assert.equal(files[3].deletions, 2)
})

test('parseGitDiff parses quoted paths', () => {
  const quotedDiff = `diff --git "a/path with spaces/file 1.ts" "b/path with spaces/file 1.ts"
index abc..def 100644
--- "a/path with spaces/file 1.ts"
+++ "b/path with spaces/file 1.ts"
@@ -1,1 +1,2 @@
+added
`
  const files = parseGitDiff(quotedDiff)
  assert.equal(files.length, 1)
  assert.equal(files[0].path, 'path with spaces/file 1.ts')
  assert.equal(files[0].additions, 1)
  assert.equal(files[0].deletions, 0)
})

test('parseGitDiff fallback parses single file diff without diff --git header', () => {
  const fallbackDiff = `--- a/fallback.ts
+++ b/fallback.ts
@@ -1,1 +1,2 @@
-hello
+world
`
  const files = parseGitDiff(fallbackDiff)
  assert.equal(files.length, 1)
  assert.equal(files[0].path, 'fallback.ts')
  assert.equal(files[0].status, 'modified')
  assert.equal(files[0].additions, 1)
  assert.equal(files[0].deletions, 1)
})
