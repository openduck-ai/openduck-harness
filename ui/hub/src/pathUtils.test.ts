import assert from 'node:assert/strict'
import test from 'node:test'
import {
  isImagePath,
  resolveProjectAssetPath,
  normalizeMentionedPath,
  isPotentialFilePath,
  extractMentionedFiles,
} from './pathUtils.ts'

test('resolveProjectAssetPath resolves relative paths relative to current markdown file', () => {
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', './images/flow.png'),
    'docs/images/flow.png',
  )
  assert.equal(
    resolveProjectAssetPath('docs/sub/guide.md', '../assets/logo.svg'),
    'docs/assets/logo.svg',
  )
  assert.equal(
    resolveProjectAssetPath('README.md', 'diagram.png'),
    'diagram.png',
  )
  assert.equal(
    resolveProjectAssetPath('README.md', './diagram.png'),
    'diagram.png',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', '/root-img.png'),
    'root-img.png',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', '/assets/icon.png'),
    'assets/icon.png',
  )
})

test('resolveProjectAssetPath preserves absolute URLs and special protocols', () => {
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', 'https://example.com/pic.png'),
    'https://example.com/pic.png',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', 'http://example.com/pic.png'),
    'http://example.com/pic.png',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', 'data:image/png;base64,123'),
    'data:image/png;base64,123',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', 'blob:http://localhost/123'),
    'blob:http://localhost/123',
  )
  assert.equal(
    resolveProjectAssetPath('docs/guide.md', '#section'),
    '#section',
  )
  assert.equal(resolveProjectAssetPath('docs/guide.md', ''), '')
})

test('isImagePath identifies image file extensions correctly', () => {
  assert.equal(isImagePath('photo.png'), true)
  assert.equal(isImagePath('photo.PNG'), true)
  assert.equal(isImagePath('banner.jpg'), true)
  assert.equal(isImagePath('banner.jpeg'), true)
  assert.equal(isImagePath('logo.svg'), true)
  assert.equal(isImagePath('anim.gif'), true)
  assert.equal(isImagePath('icon.webp'), true)
  assert.equal(isImagePath('favicon.ico'), true)
  assert.equal(isImagePath('doc.pdf'), false)
  assert.equal(isImagePath('readme.md'), false)
  assert.equal(isImagePath('script.js'), false)
})

test('normalizeMentionedPath strips whitespace, quotes, punctuation, file://, and leading ./', () => {
  assert.equal(normalizeMentionedPath(' `e2e_test/tests/bms.ts`: '), 'e2e_test/tests/bms.ts')
  assert.equal(normalizeMentionedPath('"docs/report.md."'), 'docs/report.md')
  assert.equal(normalizeMentionedPath('./full_e2e_test.sh'), 'full_e2e_test.sh')
  assert.equal(normalizeMentionedPath('file:///mnt/e/repo/file.ts'), '/mnt/e/repo/file.ts')
  assert.equal(normalizeMentionedPath('(tests\\helper.ts)'), 'tests/helper.ts')
})

test('isPotentialFilePath validates genuine file paths and rejects non-paths', () => {
  assert.equal(isPotentialFilePath('e2e_test/tests/helpers/bms_export.ts'), true)
  assert.equal(isPotentialFilePath('tests/bms_history_export.spec.ts'), true)
  assert.equal(isPotentialFilePath('e2e_test/screenshots/bms_history_ui_before_export.png'), true)
  assert.equal(isPotentialFilePath('docs/bms_history_export_report.md'), true)
  assert.equal(isPotentialFilePath('./full_e2e_test.sh'), true)
  assert.equal(isPotentialFilePath('Cargo.toml'), true)
  assert.equal(isPotentialFilePath('package.json'), true)
  assert.equal(isPotentialFilePath('.env'), true)
  assert.equal(isPotentialFilePath('Dockerfile'), true)

  // Rejections
  assert.equal(isPotentialFilePath('cargo +nightly fmt'), false)
  assert.equal(isPotentialFilePath('E2E_UI_EXPORT=1 npx playwright test'), false)
  assert.equal(isPotentialFilePath('openBmsHistory'), false)
  assert.equal(isPotentialFilePath('[data-testid="button"]'), false)
  assert.equal(isPotentialFilePath('fixed: \'right\''), false)
  assert.equal(isPotentialFilePath('.ant-table-scroll'), false)
  assert.equal(isPotentialFilePath('--test'), false)
  assert.equal(isPotentialFilePath('1.0'), false)
  assert.equal(isPotentialFilePath('POST /admin/device/bms'), false)
  assert.equal(isPotentialFilePath('https://example.com/test.ts'), false)
})

test('extractMentionedFiles extracts all files from task run 20260919_002947_task_task-758057_01a0b711 final answer', () => {
  const sampleFinalAnswer = `### Summary of Work Completed

1. **Investigated & Fixed BMS History UI Action Flow**:
   - In \`e2e_test/tests/helpers/bms_export.ts\`:
     - Identified that Ant Design Vue table uses separate split tables for fixed action columns (\`fixed: 'right'\`).
     - Updated \`openBmsHistory\` to resolve \`[data-testid="device-battery-details-button"]\` directly.

2. **Verified Playwright BMS History Export E2E Suite**:
   - Ran \`E2E_UI_EXPORT=1 npx playwright test tests/bms_history_export.spec.ts\`:
     - **Test 1**: \`POST /admin/device/bms/info/:id/:type/export returns a valid xlsx workbook\` (Passed)
   - Verified that all 6 tests in \`tests/bms_history_export.spec.ts\` passed.

3. **Snapshots & Test Report**:
   - Captured all UI step snapshots:
     - \`e2e_test/screenshots/bms_history_ui_before_export.png\`
     - \`e2e_test/screenshots/bms_history_ui_export_clicked.png\`
     - \`e2e_test/screenshots/bms_history_ui_after_download.png\`
   - Generated the report with snapshot links at \`docs/bms_history_export_report.md\`.

4. **Code Quality & Full E2E Pipeline**:
   - Formatted Rust code with \`cargo +nightly fmt\`.
   - Verified unit tests: \`cargo test -p rmqx_admin --test xlsx_export_test\` passed.
   - Ran \`./full_e2e_test.sh\` in the foreground to completion.`

  const extracted = extractMentionedFiles(sampleFinalAnswer)
  assert.deepEqual(extracted, [
    'e2e_test/tests/helpers/bms_export.ts',
    'tests/bms_history_export.spec.ts',
    'e2e_test/screenshots/bms_history_ui_before_export.png',
    'e2e_test/screenshots/bms_history_ui_export_clicked.png',
    'e2e_test/screenshots/bms_history_ui_after_download.png',
    'docs/bms_history_export_report.md',
    'full_e2e_test.sh',
  ])
})
