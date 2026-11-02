# Create Project Task：Dialog → 页面内嵌 + 草稿持久化

> 调研记录（Subtask 1）。所有行号基于当前工作树（HEAD `afddb7d`）。

## 相关文件

### 1. 主 dialog：“Create New Project Task”（Task Editor Drawer）
`ui/hub/src/components/ProjectHarnessTab.tsx`（4003 行，唯一的任务创建/编辑表单）

- 弹窗载体（L2003-2640）：
  - L2004 `{isEditingTask && (`
  - L2006 `className="modal-backdrop task-editor-backdrop"`，其 `onClick`（L2007-2013）在 `!savingTask` 时关闭
  - L2015-2028 `ref={taskEditorRef}` + `className="harness-card task-editor-drawer"` + `role="dialog"` +
    `aria-modal="true"` + `aria-labelledby="task-editor-title"` + IME composition 锁（`imeEscapeUntilRef`）
  - L2029-2047 头部 `task-editor-drawer-header`：标题 L2030-2032
    `{editingTaskId ? (taskForm.name || editingTaskId) : 'Create New Project Task'}`，右上 `modal-close-btn`（✕）
  - L2050-2078 `TaskDraftChat`（AI 起草对话）
  - L2080+ `form-row-2` / `form-group` 表单字段
  - L2613-2636 底部 `editor-button-row`：`Save Task (.goose/tasks/<id>.yaml)` + `Cancel`
- 状态（L215-240）：
  - L215 `isEditingTask` (bool，控制 dialog 显隐)
  - L216 `editingTaskId` (`string | null`，`null` = 新建，非 null = 编辑)
  - L217-228 `taskForm: ProjectTaskDefinition`（id/name/category/tags/prompt/maxTurns/timeoutSeconds/subtasks/dynamicPrompt）
  - L229 `taskTagsInput`（逗号分隔字符串，提交时 split）
  - L230 `schedule: FriendlySchedule`（从 `@aaif/goose-hub-core` 的 `defaultSchedule`）
  - L231-236 verifier：`verifierType`('none'|'command'|'diff')、`verifierCommand`、`verifierExpectedStdout`、
    `verifierExpectedExitCode`、`verifierFilePath`、`verifierExpectedContent`
  - L237-240 `estimateInfo` / `estimateFeedback` / `draftMessages` / `draftInput` / `drafting`
  - L241-243 `taskAttachments: File[]` / `existingAttachmentMentions` / `savingTask`
- 打开路径：
  - 新建：`handleOpenCreateTask()`（L946-976）——重置 `taskForm`（`id: task-${Date.now().slice(-6)}`、
    `maxTurns: config?.execution.maxTurns || 25`、`timeoutSeconds: 300`）、`schedule`、`taskTagsInput`、
    6 个 verifier 状态、`estimateInfo/estimateFeedback`、`draftMessages/draftInput/drafting`、
    `taskAttachments/existingAttachmentMentions`，最后 `setIsEditingTask(true)`
  - 编辑：`handleOpenEditTask(taskId)`（L1262-1288）→ `await api.getProjectTask(...)` 回填全部字段 → `setIsEditingTask(true)`
- 关闭路径（4 处，全部回落到 `setTaskAttachments([]) + setExistingAttachmentMentions([]) + setIsEditingTask(false)`）：
  - backdrop 点击 L2007-2013
  - 右上 ✕ L2033-2047
  - 底部 Cancel L2624-2635
  - ESC：`useEffect` L1743-1768，用 `escapeClosesTaskEditor`（`../taskEditorDismiss.ts`）判定
    （`defaultPrevented` / `isComposing` / IME 锁 / mention 弹层打开 / `savingTask` / 目标在编辑器内）
- 提交路径：`handleSaveTask()`（L1408-1494）
  - 校验：`!taskForm.id.trim()` → `Task ID is required`；`!taskForm.prompt.trim()` → `Task prompt is required`；
    `subtaskSaveError(taskForm.subtasks)`（`../taskDraft.ts`）
  - tags：`taskTagsInput.split(',').map(trim).filter(Boolean)`
  - verifier：`command` → `{type:'command', command, expectedExitCode, expectedStdout?}`；`diff` → `{type:'diff', filePath, expectedContent}`
  - 附件：`uploadAttachmentsAndComposePrompt`（`../taskAttachments.ts`）合成 prompt
  - payload：`cron = scheduleToCron(schedule)`、`schedulePaused`、`subtasks = subtasksForSave(...)`
  - 分支：`editingTaskId ? api.updateProjectTask(...) : api.createProjectTask(...)` →
    `setSuccessMsg(...)` → `setIsEditingTask(false)` → `void loadTasksAndJobs(slug, true)`；
    `catch` 只 `setError(...)`（**不清空表单**，失败后弹窗仍打开）
- 触发入口（两处，都是 `onClick={handleOpenCreateTask}`）：
  - L1978-1985 `tasks-actions-row` 里的 `btn-primary` “New Task”
  - L2648-2660 空态卡片 `empty-tasks-card` 里的 “Create your first Task”
- 关联 effect（改为内嵌后仍需保留）：L1031-1035（`isEditingTask` → `loadTaskMentionSources`）、
  L1037-1055（`isEditingTask + taskForm.id` → 列出已有附件 mention）
- 复用组件：`./TaskDraftPanel.tsx` 的 `TaskDraftChat`(L17) / `SubtaskEditor`(L118)；
  `../taskDraft.ts`、`../taskEditorDismiss.ts`、`../taskAttachments.ts`

### 2. 另一处同名创建表单（chat 抽屉，本次不改交互）
`ui/hub/src/components/ProjectChatTasksDrawer.tsx`（817 行）

- 载体：L313 `<div className="modal-backdrop task-editor-backdrop" onClick={onClose}>`，
  L316-321 `harness-card task-editor-drawer project-chat-tasks-drawer` + `role="dialog"`
- 状态：L76 `editingTask`、L77 `isCreatingNew`、L78 `formData: TaskFormData`（简化版：id/name/category/tagsString/
  prompt/maxTurns/dynamicPrompt/cron/verifierCommand）、L79-82 `savingTask/formError/successMessage`
- 打开 L131 `handleOpenCreate`、L139 `handleOpenEdit`；取消 L177 `handleCancelForm`；
  提交 L185 `handleSaveForm`（`api.createProjectTask` L236 / `updateProjectTask`）
- 入口：`ProjectChat.tsx` L796-805 渲染该抽屉；抽屉内 “New Task” 按钮 L338-345、“Create First Task” L597-603
- 结论：该处属于 Project **Chat** 页面，与 Project **Harness** 页面的任务列表是两个入口。本任务只把
  Harness 页面的创建改为内嵌；该抽屉保持原样（后续可复用同一个草稿模块）。

### 3. Project 页面结构 / 调用位置
- `ui/hub/src/components/ProjectDetail.tsx`（439 行）：`ProjectSubTab` L19（'harness'|'chat'|...），
  `lazy(() => import('./ProjectHarnessTab'))` L30-31，`ProjectChat` L33-34，按 `activeTab` 渲染（L357-359 传
  `api` / `selectedProjectSlug={detail.project.slug}` / `onNavigateToFiles`）
- `ProjectHarnessTab` 内部：`subTab` 子导航 'tasks'|'config'|'reports'|'cassettes'|'history'|'trajectory'（L110）；
  Tasks 视图容器 `{subTab === 'tasks' && (<div className="project-tasks-view">`（**L1923**，结束于 L3600 左右）
  - L1924-2000 `tasks-header-bar`（搜索 / tag 过滤 / 排序 / Run these / `tasks-actions-row`）
  - L2003-2640 Task Editor Drawer（要改造的 dialog）
  - L2642-2660+ 任务列表 / 空态（`tasks-grid`）
- 因此内嵌目标位置：`project-tasks-view` 内、`tasks-header-bar`（L1978 `tasks-actions-row`）之后、
  `{/* Tasks List */}`（L2642）之前。

### 4. 存储封装现状
- `ui/hub` **没有**统一 storage 封装，各处直接用 `localStorage`：
  - `App.tsx`（`goose-hub-url` / `goose-hub-secret`）
  - `api.ts`（同 key）
  - `components/ServerSettings.tsx`（`openduck.api.timeoutMs`、`openduck.harness.historyTimeoutMs`）
  - `components/ProjectHarnessTab.tsx`（`openduck.agentLog.pageSize`、`openduck.history.pageSize`，均 try/catch + 校验）
  - `components/TaskTrajectoryViewer.tsx`、`contexts/ThemeContext.tsx`
- 命名约定：`openduck.` 前缀；`ui/mobile/src/storage.ts` 已有 `load/save/clear` 三件套可参考
- 已存在（上一轮遗留的未跟踪文件，Subtask 2 需复核）：
  - `ui/hub/src/taskCreateDraft.ts`（402 行）导出 `TASK_DRAFT_STORAGE_PREFIX`(=`'openduck.taskCreateDraft.'`)、
    `TASK_DRAFT_VERSION`、`TASK_DRAFT_TTL_MS`、`TASK_CREATE_SECTIONS`、`TaskCreateDraftData/Form/Schedule/Verifier/Ui`、
    `taskDraftStorageKey`、`defaultTaskCreateDraft*`、`isMeaningfulTaskDraft`、`persistTaskDraft`、
    `saveTaskDraft` / `loadTaskDraft` / `clearTaskDraft`，支持注入 `{ storage, now }`
  - `ui/hub/src/taskCreateDraft.test.ts`（223 行，`pnpm test` 已纳入 `src/*.test.ts` glob）
- 测试/校验命令：`cd ui/hub && pnpm test`（=`node --experimental-strip-types --test src/*.test.ts`）、
  `pnpm run typecheck`、`pnpm run build`。`tsconfig.json` 排除 `src/**/*.test.ts`，测试文件不进 tsc。

### 5. 样式
`ui/hub/src/styles.css`：`.modal-backdrop` L1084、`.task-editor-backdrop` L8961、`.task-editor-drawer` L8969、
`.task-editor-drawer-header` L8997、`.modal-close-btn` L9007、`.task-editor-card` L9029、`.tasks-header-bar` L8092、
`.tasks-actions-row` L8120、`.form-grid` L4191、`.sub-section-title` L9528、`.subtask-editor` L9466、
`.editor-button-row` L9521。
注意：`.alert-banner`/`.alert-danger`/`.alert-success` 只在 `ProjectChatTasksDrawer.tsx` 用到，`styles.css` 中无定义
（既有缺口，内嵌提示条改用自定义 class 而不是复用它们）。

### 6. i18n
`ui/hub` 无 i18n 框架（所有文案为英文字面量），`package.json` 无 i18n 依赖；新增文案直接写英文即可。

## 改造方案

### A. 草稿持久化模块（独立、可测）
`ui/hub/src/taskCreateDraft.ts`：
- key：`openduck.taskCreateDraft.<projectId>`；envelope `{version, projectId, updatedAt, ui, form, schedule, verifier}`
  - `ui.section`（当前步骤）+ `ui.expanded: Record<TaskCreateSection, boolean>`（展开状态）
  - `form`: id/name/category/tagsInput/prompt/maxTurns/timeoutSeconds/dynamicPrompt/subtasks
  - `schedule`: kind/hour/minute/weekday；`verifier`: type + 5 个字段
  - `File[]` 附件与 chat 消息不持久化（句柄无法跨 reload）
- API：`saveTaskDraft(projectId, data, {storage?, now?})`、`loadTaskDraft(projectId, opts)`、
  `clearTaskDraft(projectId, opts)`、`persistTaskDraft(...)`（空表单则 clear）、`isMeaningfulTaskDraft(data)`
- 安全：所有 localStorage 访问 try/catch；损坏 JSON / version 不符 / projectId 不符 / `updatedAt` 超 30 天 TTL
  → 清 key 返回 null；字段级 sanitize

### B. 用内嵌面板替换 dialog
`ProjectHarnessTab.tsx`：
- 不新增 state：沿用 `isEditingTask` / `editingTaskId`，`isCreatingTask = isEditingTask && !editingTaskId`；
  面板渲染在 `project-tasks-view` 内（`tasks-actions-row`（L1978 块）之后、`{/* Tasks List */}`（L2642）之前）：
  - 容器 `.inline-task-creator`（普通卡片，非 `modal-backdrop`、无 `role="dialog"`/`aria-modal`、无 backdrop 关闭）
  - 编辑态与创建态共用该容器，仅标题/ID 输入禁用不同
  - 头部：标题 “Create New Project Task” + “Collapse”（替代 ✕）+ “Discard draft”
  - 表单 JSX（L2050-2636）整体搬入，字段/校验/`handleSaveTask` 逻辑不变
- 同一份表单同时服务创建与编辑：`isEditingTask` 一律渲染为**页面内嵌面板**（无 dialog 外壳），
  彻底去掉弹窗；编辑入口（任务卡片上的编辑按钮 → `handleOpenEditTask`）复用内嵌面板，字段禁用逻辑
  （`disabled={!!editingTaskId}` 的 Task ID）保持不变。
- 移除 dialog 相关残留：backdrop 点击关闭、`role="dialog"`、`aria-modal`、右上 ✕（仅创建态）；
  ESC 关闭逻辑改为对内嵌面板生效（`escapeClosesTaskEditor` 继续复用，`taskEditorRef` 指向内嵌容器）。
- `handleOpenCreateTask` 改为：先 `loadTaskDraft(slug)`，有实质内容则回填并 `setDraftRestored(true)`，
  否则默认值；然后 `setIsEditingTask(true)`。

#### B.1 要删/要留的代码清单（Subtask 3 施工用）
删除：
- L2006 `className="modal-backdrop task-editor-backdrop"` 外层 div 及其 `onClick`（L2007-2013）——整块 backdrop 消失（创建与编辑都不再有遮罩）
- L2019-2020 `role="dialog"` / `aria-modal="true"` / `aria-labelledby`（内嵌面板不用 dialog 语义）
- L2033-2047 右上 `modal-close-btn` ✕、L2624-2635 底部 Cancel 的 `setIsEditingTask(false)` 直连（改为
  `handleDiscardTaskDraft()` 统一走清理）
- 样式引用：`ProjectHarnessTab` 不再引用 `.modal-backdrop` / `.task-editor-backdrop` / `.task-editor-drawer` /
  `.task-editor-drawer-header` / `.modal-close-btn`（这些 class 仍被 chat 抽屉 `ProjectChatTasksDrawer.tsx`
  使用，因此 `styles.css` 里的定义**保留**，只移除本组件处的引用）；改为新增 `.inline-task-creator*` 样式
- `imeEscapeUntilRef` 保留：内嵌面板保留 `onCompositionStart/End`，避免中文输入法组词时按 ESC 误关

保留（不动的可复用部分）：
- 表单本体 L2050-2611（`TaskDraftChat`、`form-row-2`/`form-group`、verifier radios、estimate、SubtaskEditor…）
- `handleSaveTask` L1408-1494（含校验、`subtaskSaveError`、`uploadAttachmentsAndComposePrompt`、create/update 分支）
- `handleOpenEditTask` L1262-1288 后端拉取与回填逻辑（渲染改为内嵌面板）
- `escapeClosesTaskEditor`（`taskEditorDismiss.ts`）与其测试

### C. 防抖保存 + 恢复提示
- `debounce` 复用 `ui/hub/src/mention.ts` L50-76 的已有实现（返回的函数带 `.cancel()`），间隔 `INLINE_DRAFT_SAVE_MS = 600`；
  `useEffect` 依赖 `[selectedProjectSlug, isEditingTask, editingTaskId, taskForm, taskTagsInput, schedule,
  verifierType, verifierCommand, verifierExpectedStdout, verifierExpectedExitCode, verifierFilePath,
  verifierExpectedContent, taskCreatorSection, taskCreatorExpanded]`
- `savingTask` 期间、编辑态不写草稿；卸载时 `debounce.cancel()` + 立即 flush 一次（`beforeunload` 也要 flush）
- 恢复提示条：内嵌面板顶部 `.inline-draft-restored`：“Restored your unsubmitted draft (saved <relative time>)”
  + “Discard draft” 按钮；可 Dismiss（仅隐藏提示，不删草稿）

### D. 草稿清理时机
- 提交成功（`setSuccessMsg` 后、`setIsEditingTask(false)` 前）：`clearTaskDraft(slug)`（create 与 update 都清，
  避免遗留）
- 显式取消（创建态）：Cancel / Collapse / Discard / ESC → `clearTaskDraft(slug)`（视为用户清空）；编辑态 ESC
  只关闭面板、不触碰草稿
- 提交失败：catch 不清草稿（保证异常退出 + 报错后仍能恢复）
- 编辑态：既不读也不写草稿

### E. 测试
- `ui/hub/src/taskCreateDraft.test.ts`（已存在，需复核/补齐）：round-trip、按 projectId 隔离、clear、
  损坏 JSON、`isMeaningfulTaskDraft` 真假、`updatedAt` 刷新、`localStorage` 抛异常降级、TTL/version 失效
- `ui/hub/src/taskCreateInline.test.ts`（新增，纯函数 helper 从组件抽离）：
  `openInlineCreate()`（草稿存在→restore 标志 / 不存在→默认值）、`shouldPersistInlineDraft({editing, saving})`、
  `afterInlineSave({ok})`（成功 clear / 失败保留）、`retimeRestoredDraft(now)` 文案
- 命令：`cd ui/hub && pnpm test && pnpm run typecheck && pnpm run build`

### F. 新增文案（无 i18n 框架，直接英文）
- `Create New Project Task`（面板标题，创建态）
- `Restored your unsubmitted draft · saved <relative time>`（恢复提示）
- `Discard draft`（清空草稿按钮）
- `Dismiss`（仅隐藏提示条）
- `Collapse form`（收起面板，等价于取消）
- `Saving…` / `Save Task (.goose/tasks/<id>.yaml)`（沿用既有）

### G. 验证计划
1. `cd ui/hub && node --experimental-strip-types --test src/taskCreateDraft.test.ts src/taskCreateInline.test.ts`
2. `cd ui/hub && pnpm run typecheck`（`tsconfig` 不含测试文件，组件类型错误在此暴露）
3. `cd ui/hub && pnpm run build`
4. `cd ui/hub && pnpm test`（全量 hub 测试，确认没有回归）
5. 手工验证路径（内嵌面板）：
   - 点 “New Task” → 面板出现在任务列表上方（无遮罩、无 dialog）
   - 输入 name/prompt → 等 1 秒（防抖 600ms）后刷新页面 → 重新点 “New Task” → 字段恢复 + 出现恢复提示
   - 点 “Discard draft” → 清空表单且 localStorage 中 `openduck.taskCreateDraft.<slug>` 消失
   - 保存成功 → key 消失；构造后端报错 → key 仍在
