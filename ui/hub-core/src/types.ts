import type {
  RequestPermissionRequest,
  ToolCallStatus,
  ToolKind,
} from '@agentclientprotocol/sdk'

export type ProjectKind = 'software' | 'docs' | 'automation' | 'other'
export type ProjectStatus = 'active' | 'paused' | 'archived'

export interface Project {
  slug: string
  title: string
  description: string
  path: string
  workingDirs: string[]
  kind: ProjectKind
  language?: string
  status: ProjectStatus
  tags: string[]
  emailRecipients?: string[]
  notes: string
  sourcePath: string
  lastActivityAt?: string | null
}

export interface InsightEntry {
  name: string
  kind: 'dir' | 'file'
}

export interface GitInsight {
  isRepo: boolean
  branch?: string
  dirty?: boolean
  ahead?: number
  behind?: number
  changed?: number
  lastCommit?: { sha: string; subject: string; at: string }
}

export interface SessionSummary {
  id: string
  name: string
  updatedAt: string
  sessionType: string
}

export interface JobRun {
  id: string
  jobId: string
  projectId?: string | null
  sessionId?: string | null
  trigger: string
  status: string
  startedAt: string
  finishedAt?: string | null
  error?: string | null
  durationMs?: number | null
}

export interface ProjectInsight {
  exists: boolean
  entryCount: number
  entries: InsightEntry[]
  truncated: boolean
  git?: GitInsight
}

export interface ProjectInsightResponse {
  insight: ProjectInsight
  recents?: {
    sessions: SessionSummary[]
    jobRuns: JobRun[]
  }
}

export interface ProjectDetail {
  project: Project
  insight: ProjectInsight
  recents?: {
    sessions: SessionSummary[]
    jobRuns: JobRun[]
  }
}

export interface ProjectInput {
  slug: string
  title?: string
  description: string
  path: string
  kind: ProjectKind
  language?: string
  status: ProjectStatus
  tags: string[]
  emailRecipients?: string[]
  notes: string
  allowUntrustedPath?: boolean
}

export interface ProjectRoot {
  path: string
  name: string
  available: boolean
}

export interface ProjectDirectory {
  name: string
  path: string
}

export interface Job {
  id: string
  source: string
  cron: string
  lastRun?: string | null
  currentlyRunning: boolean
  paused: boolean
  currentSessionId?: string | null
  processStartTime?: string | null
  projectId?: string | null
  workingDir?: string | null
  recipe?: {
    version?: string
    title?: string
    description?: string
    prompt?: string
  } | null
  nextRunAt?: string | null
}

export interface CreateJobInput {
  id: string
  cron: string
  recipe: {
    version: string
    title: string
    description: string
    prompt: string
  }
}

export type PermissionAction =
  | 'allow_once'
  | 'always_allow'
  | 'deny_once'
  | 'always_deny'
  | 'cancel'

export type ChatRole = 'user' | 'assistant' | 'system'

export type ToolCallEntry = {
  toolCallId: string
  title: string
  status: ToolCallStatus
  kind?: ToolKind
  summary?: string
}

export type ChatMessage = {
  id: string
  role: ChatRole
  text: string
  streaming?: boolean
  toolCalls?: ToolCallEntry[]
}

export type PendingPermission = {
  key: string
  request: RequestPermissionRequest
}

export interface ProjectFileEntry {
  name: string
  path: string
  kind: 'dir' | 'file'
  size?: number
  modifiedAt?: string
  extension?: string
}

export interface FileListResponse {
  currentPath: string
  parentPath?: string | null
  entries: ProjectFileEntry[]
}

export interface FileSearchResponse {
  query: string
  entries: ProjectFileEntry[]
}

export interface FileContentResponse {
  path: string
  content: string
  size: number
  isBinary: boolean
}

export interface CreateFileInput {
  path: string
  kind: 'dir' | 'file'
  content?: string
}

export interface RenameFileInput {
  oldPath: string
  newPath: string
}

export interface ExecCommandInput {
  command: string
  cwd?: string
  timeoutSecs?: number
}

export interface ExecCommandOutput {
  stdout: string
  stderr: string
  exitCode: number | null
  success: boolean
  durationMs: number
  cwd: string
}

export interface GitFileChange {
  path: string
  originalPath?: string
  indexStatus: string
  worktreeStatus: string
  staged: boolean
  unstaged: boolean
  untracked: boolean
  conflict: boolean
}

export interface GitStatusResponse {
  isRepo: boolean
  branch?: string
  detached: boolean
  ahead: number
  behind: number
  files: GitFileChange[]
}

export interface GitCommitEntry {
  sha: string
  shortSha: string
  parents: string[]
  authorName: string
  authorEmail: string
  authoredAt: string
  subject: string
  refs: string[]
}

export interface GitLogResponse {
  isRepo: boolean
  head?: string
  branch?: string
  detached: boolean
  commits: GitCommitEntry[]
  truncated: boolean
}

export interface GitDiffResponse {
  path: string
  staged: boolean
  diff: string
  truncated: boolean
}

export interface GitShowResponse {
  commit: GitCommitEntry
  body: string
  diff: string
  truncated: boolean
}

export interface GitCommitResponse {
  sha: string
  subject: string
}

export interface GitCommitMessageResponse {
  message: string
  source: 'model' | 'heuristic' | string
}

// ---------------- Harness Types ----------------

export type HarnessRunStatus =
  | 'success'
  | 'failure'
  | 'timeout'
  | 'error'
  | 'running'
  | 'cancelled'
  | 'skipped'
  | string

export interface HarnessTaskSpec {
  id: string
  dataset: string
  repo?: string | null
  baseCommit?: string | null
  problemStatement: string
  environment?: {
    baseImage?: string | null
    setupCommands: string[]
    envVars: Record<string, string>
  }
  maxTurns?: number
  timeoutSeconds?: number
}

export interface HarnessVerificationResult {
  passed: boolean
  verifierType: string
  stdout?: string | null
  stderr?: string | null
  details?: Record<string, unknown>
}

export interface HarnessTaskResult {
  taskId: string
  dataset: string
  status: HarnessRunStatus
  passed: boolean
  stepCount: number
  toolCallsCount: number
  durationMs: number
  verification?: HarnessVerificationResult | null
  error?: string | null
}

export interface HarnessEvalMetrics {
  totalTasks: number
  passedTasks: number
  failedTasks: number
  errorTasks: number
  passRate: number
  avgDurationMs: number
  avgSteps: number
  totalToolCalls: number
}

export interface HarnessEvaluationReport {
  suiteName: string
  policyName: string
  startedAt: string
  completedAt: string
  metrics: HarnessEvalMetrics
  taskResults: HarnessTaskResult[]
  metadata: Record<string, string>
}

export interface HarnessReportSummary {
  id: string
  fileName: string
  path: string
  suiteName: string
  timestamp: string
  totalTasks: number
  passedTasks: number
  failedTasks: number
  passRate: number
  avgDurationMs: number
  totalToolCalls: number
}

export interface HarnessCassetteFrame {
  key: string
  query: string
  response: string
  isTool: boolean
  timestamp: string
}

export interface HarnessCassetteSummary {
  name: string
  fileName: string
  path: string
  createdAt: string
  frameCount: number
  taskId?: string | null
  problemStatement?: string | null
}

export interface HarnessCassetteDetail {
  name: string
  createdAt: string
  taskSpec?: HarnessTaskSpec | null
  frames: Record<string, HarnessCassetteFrame>
}

export interface ActiveRuleSummary {
  name: string
  description?: string
  global: boolean
  path: string
}

export interface TokenUsageSummary {
  inputTokens?: number
  outputTokens?: number
  totalTokens?: number
}

export interface HarnessJudgmentRecord {
  point: string
  mode: 'off' | 'shadow' | 'active'
  stateDigest?: string
  questionType?: string
  criteria?: Record<string, string>
  verdict: {
    kind: string
    value?: number
    threshold?: number
    triggered?: boolean
    selected?: string
    reason?: string
    message?: string
  }
  confidence?: number
  probabilities?: Record<string, number>
  latencyMs: number
  timestamp: string
}

export interface HarnessTrajectoryStep {
  stepIndex: number
  timestamp: string
  agentAction: unknown
  toolResults?: unknown[]
  durationMs: number
  tokenUsage?: TokenUsageSummary | null
  llmRequest?: Record<string, unknown> | null
  llmResponse?: Record<string, unknown> | null
  judgments?: HarnessJudgmentRecord[]
}

export interface HarnessTrajectoryRecord {
  taskId: string
  policyName: string
  startedAt: string
  completedAt?: string | null
  steps: HarnessTrajectoryStep[]
  finalStatus?: HarnessRunStatus | null
  systemPrompt?: string | null
  activeRules?: ActiveRuleSummary[] | null
}

export interface HarnessActiveJob {
  jobId: string
  taskId: string
  jobType: string
  description: string
  startedAt: string
  currentStatus: string
  provider?: string | null
  model?: string | null
  projectSlug?: string | null
}

export interface HarnessHistorySummary {
  id: string
  fileName: string
  taskId: string
  taskName?: string | null
  jobType: string
  status: HarnessRunStatus
  stepCount: number
  toolCallsCount: number
  durationMs: number
  finalAnswer?: string | null
  startedAt: string
  recordedCassettePath?: string | null
  projectSlug?: string | null
  error?: string | null
  continuable?: boolean
  progressSummary?: string | null
}

export interface ContinuationCheckpoint {
  taskId: string
  sourceJobId?: string
  createdAt: string
  stopReason: string
  error?: string | null
  compactedSummary: string
  completedSubtasks: Array<{
    subtaskId: string
    title: string
    status: HarnessRunStatus
    modifiedFiles: string[]
    summary: string
    stepCount: number
    stepsTaken?: number
  }>
  resumeSubtaskId?: string | null
  subtaskPlan: Array<{
    id: string
    title: string
    description: string
    targetFiles?: string[] | null
    maxTurns?: number | null
  }>
  stepCount: number
}

export interface HarnessOverview {
  availableDatasets: string[]
  reports: HarnessReportSummary[]
  cassettes: HarnessCassetteSummary[]
  activeJobs: HarnessActiveJob[]
  history: HarnessHistorySummary[]
  defaultProvider?: string | null
  defaultModel?: string | null
}

export interface HarnessEvalRequest {
  dataset: string
  concurrency?: number
  maxTurns?: number
  provider?: string
  model?: string
  echo?: boolean
  outputDir?: string
}

export interface HarnessRunRequest {
  taskFile?: string
  prompt?: string
  maxTurns?: number
  provider?: string
  model?: string
  echo?: boolean
  recordPath?: string
  sandbox?: 'local' | 'container'
}

export interface HarnessReplayRequest {
  cassettePath: string
  taskFile?: string
}

export interface HarnessRunResponse {
  taskId: string
  status: HarnessRunStatus
  stepCount: number
  toolCallsCount: number
  durationMs: number
  finalAnswer?: string | null
  trajectory: HarnessTrajectoryRecord
  recordedCassettePath?: string | null
  systemPrompt?: string | null
  activeRules?: ActiveRuleSummary[] | null
  continuation?: ContinuationCheckpoint | null
  projectSlug?: string | null
}

export interface HarnessJobInspect {
  job: HarnessActiveJob
  live: boolean
  snapshot?: HarnessRunResponse | null
}

// ---------------- Project Harness Types ----------------

export interface ProjectHarnessOverview {
  config: ProjectHarnessConfig
  tasks: ProjectTaskSummary[]
  reports: HarnessReportSummary[]
  cassettes: HarnessCassetteSummary[]
  activeJobs: HarnessActiveJob[]
  history: HarnessHistorySummary[]
}

export interface ProjectHarnessConfig {
  version: string
  policy: {
    provider?: string
    model?: string
    systemPrompt?: string
  }
  sandbox: {
    kind: string
    containerImage?: string
    setupCommands: string[]
    envVars: Record<string, string>
  }
  execution: {
    maxTurns: number
    timeoutSeconds: number
    concurrency: number
  }
  paths: {
    tasksDir: string
    resultsDir: string
    cassettesDir: string
  }
}

export type TaskVerifierSpec =
  | {
      type?: 'command'
      command: string
      expectedExitCode?: number
      expectedStdout?: string
    }
  | {
      type: 'diff'
      filePath: string
      expectedContent: string
    }

export interface SubtaskSpec {
  id: string
  title: string
  description: string
  targetFiles?: string[]
  maxTurns?: number
  explorationBudget?: number
}

export interface ProjectTaskDefinition {
  id: string
  name?: string
  category?: string
  tags: string[]
  prompt: string
  environment?: {
    baseImage?: string
    setupCommands: string[]
    envVars: Record<string, string>
  }
  verifier?: TaskVerifierSpec
  maxTurns?: number
  timeoutSeconds?: number
  cron?: string
  schedulePaused?: boolean
  autoDecompose?: boolean
  subtasks?: SubtaskSpec[]
  phaseMaxTurns?: number[]
  /** When true, a run can append extra instructions without rewriting the saved prompt. */
  dynamicPrompt?: boolean
}

export interface ProjectTaskSummary {
  id: string
  name: string
  category?: string
  tags: string[]
  filePath: string
  hasVerifier: boolean
  createdAt: string
  cron?: string
  schedulePaused?: boolean
  nextRunAt?: string
  currentlyRunning?: boolean
  dynamicPrompt?: boolean
}

export interface ProjectTaskSchedulePatch {
  cron?: string
  paused?: boolean
}

export interface ProjectTaskRunRequest {
  provider?: string
  model?: string
  maxTurns?: number
  echo?: boolean
  record?: boolean
  recordPath?: string
  continueFromRunId?: string
  extraTurns?: number
  /** One-run instructions. The task must have dynamicPrompt enabled. */
  extraPrompt?: string
}

export interface ProjectHarnessEvalRequest {
  category?: string
  tags?: string[]
  taskIds?: string[]
  concurrency?: number
  maxTurns?: number
  provider?: string
  model?: string
  echo?: boolean
  outputDir?: string
}

export interface ProjectTaskEstimateRequest {
  prompt: string
  category?: string
  feedback?: string
  previousEstimate?: ProjectTaskEstimateResponse
}

export interface ProjectTaskEstimateResponse {
  suggestedMaxTurns: number
  suggestedTimeoutSeconds: number
  complexity: 'simple' | 'medium' | 'complex' | 'multi-step' | string
  reasoning: string
  source: 'model' | 'heuristic' | string
}

// ---------------- SSE Real-Time Event Types ----------------

export type HarnessProjectEvent =
  | {
      type: 'task_status_changed'
      projectSlug: string
      taskId: string
      jobId?: string
      status: string
    }
  | {
      type: 'task_upserted'
      projectSlug: string
      taskId: string
    }
  | {
      type: 'task_deleted'
      projectSlug: string
      taskId: string
    }
  | {
      type: 'report_generated'
      projectSlug: string
      reportId: string
      report: HarnessReportSummary
    }
  | {
      type: 'overview_invalidated'
      projectSlug: string
    }

export type HarnessJobLiveEvent =
  | {
      type: 'step_update'
      jobId: string
      stepIndex: number
      step: HarnessTrajectoryStep
      durationMs: number
    }
  | {
      type: 'snapshot'
      jobId: string
      snapshot: HarnessRunResponse
    }
  | {
      type: 'finished'
      jobId: string
      status: HarnessRunStatus
      durationMs: number
      finalAnswer?: string | null
    }



