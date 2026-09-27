import type {
  CreateFileInput,
  CreateJobInput,
  ExecCommandInput,
  ExecCommandOutput,
  FileContentResponse,
  GitCommitMessageResponse,
  GitCommitResponse,
  GitDiffResponse,
  GitLogResponse,
  GitShowResponse,
  GitStatusResponse,
  FileListResponse,
  FileSearchResponse,
  HarnessActiveJob,
  HarnessCassetteDetail,
  HarnessCassetteSummary,
  HarnessEvalRequest,
  HarnessEvaluationReport,
  HarnessHistorySummary,
  HarnessJobInspect,
  HarnessOverview,
  HarnessReplayRequest,
  HarnessReportSummary,
  HarnessRunRequest,
  HarnessRunResponse,
  Job,
  JobRun,
  Project,
  ProjectDetail,
  ProjectDirectory,
  ProjectInsightResponse,
  ProjectRoot,
  ProjectHarnessConfig,
  ProjectHarnessEvalRequest,
  ProjectHarnessOverview,
  ProjectInput,
  ProjectTaskDefinition,
  ProjectTaskEstimateRequest,
  ProjectTaskEstimateResponse,
  ProjectTaskRunRequest,
  ProjectTaskSummary,
  RenameFileInput,
  SessionSummary,
  HarnessProjectEvent,
  HarnessJobLiveEvent,
} from './types.ts'
import {
  subscribeJobLiveStream,
  subscribeProjectEvents,
} from './events.ts'
import { absoluteServerBase } from './url.ts'

export function apiUrl(baseUrl: string, path: string): string {
  const base = absoluteServerBase(baseUrl)
  const route = path.startsWith('/') ? path : `/${path}`
  return `${base}${route}`
}

export async function readErrorMessage(response: Response): Promise<string> {
  const fallback = `Request failed (${response.status})`
  const text = (await response.text()).trim()
  if (!text) return fallback
  try {
    const parsed = JSON.parse(text) as { error?: unknown }
    if (typeof parsed?.error === 'string' && parsed.error.trim()) {
      return parsed.error
    }
  } catch {
    // Response body is not JSON; surface the raw text.
  }
  return text
}

function requestTimeoutMs(path: string): number | null {
  const route = path.toLowerCase()
  if (
    route.includes('/run') ||
    route.includes('/eval') ||
    route.includes('/replay') ||
    route.includes('/exec') ||
    route.includes('/estimate') ||
    route.includes('/stream')
  ) {
    return null
  }
  return 12_000
}

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs: number | null,
): Promise<Response> {
  if (!timeoutMs) {
    return fetch(url, init)
  }
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    return await fetch(url, { ...init, signal: controller.signal })
  } catch (err) {
    if (controller.signal.aborted) {
      throw new Error(`Request timed out after ${timeoutMs}ms`)
    }
    throw err
  } finally {
    clearTimeout(timer)
  }
}

export function createApi(baseUrl: string, secret = '') {
  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers)
    headers.set('Accept', 'application/json')
    if (secret) headers.set('X-Secret-Key', secret)
    if (init.body) headers.set('Content-Type', 'application/json')
    const response = await fetchWithTimeout(
      apiUrl(baseUrl, path),
      { ...init, headers },
      requestTimeoutMs(path),
    )
    if (!response.ok) throw new Error(await readErrorMessage(response))
    return response.status === 204 ? (undefined as T) : response.json() as Promise<T>
  }
  return {
    listProjects: () => request<{ projects: Project[] }>('/api/v1/projects'),
    getProject: (slug: string, options?: { lazy?: boolean }) =>
      request<ProjectDetail>(
        `/api/v1/projects/${encodeURIComponent(slug)}${options?.lazy ? '?lazy=true' : ''}`,
      ),
    getProjectInsight: (slug: string) =>
      request<ProjectInsightResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/insight`,
      ),
    createProject: (input: ProjectInput) => request<{ project: Project }>('/api/v1/projects', { method: 'POST', body: JSON.stringify(input) }),
    listProjectRoots: () => request<{ roots: ProjectRoot[] }>('/api/v1/project-roots'),
    addProjectRoot: (input: { path: string; allowUntrustedPath?: boolean }) =>
      request<{ root: ProjectRoot }>('/api/v1/project-roots', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    removeProjectRoot: (path: string) =>
      request<void>(`/api/v1/project-roots?path=${encodeURIComponent(path)}`, { method: 'DELETE' }),
    listProjectRootDirectories: (root: string) =>
      request<{ root: string; directories: ProjectDirectory[]; truncated: boolean }>(
        `/api/v1/project-roots/directories?root=${encodeURIComponent(root)}`,
      ),
    createProjectRootDirectory: (input: { root: string; name: string }) =>
      request<{ directory: ProjectDirectory; created: boolean }>(
        '/api/v1/project-roots/directories',
        { method: 'POST', body: JSON.stringify(input) },
      ),
    patchProject: (slug: string, patch: Partial<ProjectInput>) =>
      request<{ project: Project }>(`/api/v1/projects/${encodeURIComponent(slug)}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
    archiveProject: (slug: string) => request<{ project: Project }>(`/api/v1/projects/${encodeURIComponent(slug)}`, { method: 'PATCH', body: JSON.stringify({ status: 'archived' }) }),
    deleteProject: (slug: string) => request<void>(`/api/v1/projects/${encodeURIComponent(slug)}`, { method: 'DELETE' }),
    listProjectSessions: (slug: string) => request<{ sessions: SessionSummary[] }>(`/api/v1/projects/${encodeURIComponent(slug)}/sessions`),
    listJobs: () => request<{ jobs: Job[] }>('/api/v1/jobs'),
    listProjectJobs: (slug: string) => request<{ jobs: Job[] }>(`/api/v1/projects/${encodeURIComponent(slug)}/jobs`),
    createJob: (slug: string, input: CreateJobInput) => request<{ job: Job }>(`/api/v1/projects/${encodeURIComponent(slug)}/jobs`, { method: 'POST', body: JSON.stringify(input) }),
    patchJob: (jobId: string, body: { cron?: string; paused?: boolean }) => request<{ job: Job }>(`/api/v1/jobs/${encodeURIComponent(jobId)}`, { method: 'PATCH', body: JSON.stringify(body) }),
    deleteJob: (jobId: string) => request<void>(`/api/v1/jobs/${encodeURIComponent(jobId)}`, { method: 'DELETE' }),
    runJob: (jobId: string) => request<{ runId: string; sessionId: string }>(`/api/v1/jobs/${encodeURIComponent(jobId)}/run`, { method: 'POST' }),
    killJob: (jobId: string) => request<{ message: string }>(`/api/v1/jobs/${encodeURIComponent(jobId)}/kill`, { method: 'POST' }),
    listJobRuns: (jobId: string) => request<{ runs: JobRun[] }>(`/api/v1/jobs/${encodeURIComponent(jobId)}/runs`),
    listFiles: (slug: string, path?: string) =>
      request<FileListResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files${path ? `?path=${encodeURIComponent(path)}` : ''}`,
      ),
    searchFiles: (slug: string, q: string, limit?: number) =>
      request<FileSearchResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files/search?q=${encodeURIComponent(q)}${limit ? `&limit=${limit}` : ''}`,
      ),
    readFile: (slug: string, path: string) =>
      request<FileContentResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files/content?path=${encodeURIComponent(path)}`,
      ),
    writeFile: (slug: string, path: string, content: string) =>
      request<{ success: boolean; path: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files/content`,
        { method: 'PUT', body: JSON.stringify({ path, content }) },
      ),
    writeFileBytes: async (slug: string, path: string, content: Uint8Array | ArrayBuffer) => {
      const headers = new Headers()
      headers.set('Accept', 'application/json')
      headers.set('Content-Type', 'application/octet-stream')
      if (secret) headers.set('X-Secret-Key', secret)
      const bytes = content instanceof Uint8Array ? content : new Uint8Array(content)
      const payload = new ArrayBuffer(bytes.byteLength)
      new Uint8Array(payload).set(bytes)
      const response = await fetchWithTimeout(
        apiUrl(
          baseUrl,
          `/api/v1/projects/${encodeURIComponent(slug)}/files/bytes?path=${encodeURIComponent(path)}`,
        ),
        { method: 'PUT', headers, body: payload },
        30_000,
      )
      if (!response.ok) throw new Error(await readErrorMessage(response))
      return response.json() as Promise<{ success: boolean; path: string }>
    },
    getFileBytesUrl: (slug: string, path: string) => {
      const params = new URLSearchParams({ path })
      if (secret) {
        params.set('token', secret)
      }
      return apiUrl(
        baseUrl,
        `/api/v1/projects/${encodeURIComponent(slug)}/files/bytes?${params.toString()}`,
      )
    },
    readFileBytes: async (slug: string, path: string) => {
      const headers = new Headers()
      if (secret) headers.set('X-Secret-Key', secret)
      const response = await fetchWithTimeout(
        apiUrl(
          baseUrl,
          `/api/v1/projects/${encodeURIComponent(slug)}/files/bytes?path=${encodeURIComponent(path)}`,
        ),
        { method: 'GET', headers },
        30_000,
      )
      if (!response.ok) throw new Error(await readErrorMessage(response))
      return response.blob()
    },
    createFileOrDir: (slug: string, input: CreateFileInput) =>
      request<{ success: boolean; path: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files/create`,
        { method: 'POST', body: JSON.stringify(input) },
      ),
    deleteFile: (slug: string, path: string) =>
      request<void>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files?path=${encodeURIComponent(path)}`,
        { method: 'DELETE' },
      ),
    renameFile: (slug: string, input: RenameFileInput) =>
      request<{ success: boolean; oldPath: string; newPath: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/files/rename`,
        { method: 'POST', body: JSON.stringify(input) },
      ),
    execCommand: (slug: string, input: ExecCommandInput) =>
      request<ExecCommandOutput>(
        `/api/v1/projects/${encodeURIComponent(slug)}/terminal/exec`,
        { method: 'POST', body: JSON.stringify(input) },
      ),
    getGitStatus: (slug: string) =>
      request<GitStatusResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/status`,
      ),
    getGitLog: (slug: string, limit?: number) =>
      request<GitLogResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/log${
          typeof limit === 'number' ? `?limit=${encodeURIComponent(String(limit))}` : ''
        }`,
      ),
    getGitDiff: (slug: string, path: string, staged = false) =>
      request<GitDiffResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/diff?path=${encodeURIComponent(path)}${
          staged ? '&staged=true' : ''
        }`,
      ),
    getGitShow: (slug: string, sha: string) =>
      request<GitShowResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/show?sha=${encodeURIComponent(sha)}`,
      ),
    stageGitFiles: (slug: string, paths: string[]) =>
      request<GitStatusResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/stage`,
        { method: 'POST', body: JSON.stringify({ paths }) },
      ),
    unstageGitFiles: (slug: string, paths: string[]) =>
      request<GitStatusResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/unstage`,
        { method: 'POST', body: JSON.stringify({ paths }) },
      ),
    discardGitFiles: (slug: string, paths: string[]) =>
      request<GitStatusResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/discard`,
        { method: 'POST', body: JSON.stringify({ paths }) },
      ),
    commitGitChanges: (slug: string, message: string, paths: string[] = []) =>
      request<GitCommitResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/commit`,
        { method: 'POST', body: JSON.stringify({ message, paths }) },
      ),
    generateGitCommitMessage: (slug: string, paths: string[] = []) =>
      request<GitCommitMessageResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/git/commit-message`,
        { method: 'POST', body: JSON.stringify({ paths }) },
      ),
    getHarnessOverview: () => request<HarnessOverview>('/api/v1/harness/overview'),
    listActiveHarnessJobs: () => request<HarnessActiveJob[]>('/api/v1/harness/jobs'),
    inspectHarnessJob: (jobId: string) =>
      request<HarnessJobInspect>(`/api/v1/harness/jobs/${encodeURIComponent(jobId)}`),
    listHarnessHistory: () => request<HarnessHistorySummary[]>('/api/v1/harness/history'),
    getHarnessHistoryDetail: (runId: string) =>
      request<HarnessRunResponse>(`/api/v1/harness/history/${encodeURIComponent(runId)}`),
    listHarnessReports: (dir?: string) =>
      request<HarnessReportSummary[]>(
        `/api/v1/harness/reports${dir ? `?dir=${encodeURIComponent(dir)}` : ''}`,
      ),
    getHarnessReport: (reportId: string, dir?: string) =>
      request<HarnessEvaluationReport>(
        `/api/v1/harness/reports/${encodeURIComponent(reportId)}${dir ? `?dir=${encodeURIComponent(dir)}` : ''}`,
      ),
    listHarnessCassettes: () => request<HarnessCassetteSummary[]>('/api/v1/harness/cassettes'),
    getHarnessCassette: (name: string) =>
      request<HarnessCassetteDetail>(`/api/v1/harness/cassettes/${encodeURIComponent(name)}`),
    runHarnessEval: (input: HarnessEvalRequest) =>
      request<HarnessEvaluationReport>('/api/v1/harness/eval', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    runHarnessTask: (input: HarnessRunRequest) =>
      request<HarnessRunResponse>('/api/v1/harness/run', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    runHarnessReplay: (input: HarnessReplayRequest) =>
      request<HarnessRunResponse>('/api/v1/harness/replay', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    // Project Harness
    getProjectHarnessOverview: (slug: string) =>
      request<ProjectHarnessOverview>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/overview`,
      ),
    getProjectHarnessConfig: (slug: string) =>
      request<ProjectHarnessConfig>(`/api/v1/projects/${encodeURIComponent(slug)}/harness/config`),
    updateProjectHarnessConfig: (slug: string, config: ProjectHarnessConfig) =>
      request<ProjectHarnessConfig>(`/api/v1/projects/${encodeURIComponent(slug)}/harness/config`, {
        method: 'PUT',
        body: JSON.stringify(config),
      }),
    listProjectTasks: (slug: string) =>
      request<ProjectTaskSummary[]>(`/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks`),
    estimateProjectTask: (slug: string, req: ProjectTaskEstimateRequest) =>
      request<ProjectTaskEstimateResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/estimate`,
        {
          method: 'POST',
          body: JSON.stringify(req),
        },
      ),
    createProjectTask: (slug: string, task: ProjectTaskDefinition) =>
      request<ProjectTaskDefinition>(`/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks`, {
        method: 'POST',
        body: JSON.stringify(task),
      }),
    getProjectTask: (slug: string, taskId: string) =>
      request<ProjectTaskDefinition>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}`,
      ),
    updateProjectTask: (slug: string, taskId: string, task: ProjectTaskDefinition) =>
      request<ProjectTaskDefinition>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}`,
        {
          method: 'PUT',
          body: JSON.stringify(task),
        },
      ),
    deleteProjectTask: (slug: string, taskId: string) =>
      request<{ success: boolean; deleted: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}`,
        {
          method: 'DELETE',
        },
      ),
    patchProjectTaskSchedule: (
      slug: string,
      taskId: string,
      body: { cron?: string; paused?: boolean },
    ) =>
      request<ProjectTaskDefinition>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}/schedule`,
        {
          method: 'PATCH',
          body: JSON.stringify(body),
        },
      ),
    runProjectTask: (slug: string, taskId: string, input: ProjectTaskRunRequest) =>
      request<HarnessRunResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}/run`,
        {
          method: 'POST',
          body: JSON.stringify(input),
        },
      ),
    stopProjectTask: (slug: string, taskId: string) =>
      request<{ success: boolean; message: string; taskId: string; jobId: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/tasks/${encodeURIComponent(taskId)}/stop`,
        {
          method: 'POST',
        },
      ),
    runProjectHarnessEval: (slug: string, input: ProjectHarnessEvalRequest) =>
      request<HarnessEvaluationReport>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/eval`,
        {
          method: 'POST',
          body: JSON.stringify(input),
        },
      ),
    listProjectReports: (slug: string) =>
      request<HarnessReportSummary[]>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/reports`,
      ),
    getProjectReport: (slug: string, reportId: string) =>
      request<HarnessEvaluationReport>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/reports/${encodeURIComponent(reportId)}`,
      ),
    listProjectCassettes: (slug: string) =>
      request<HarnessCassetteSummary[]>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/cassettes`,
      ),
    listProjectActiveJobs: (slug: string) =>
      request<HarnessActiveJob[]>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/jobs`,
      ),
    inspectProjectHarnessJob: (slug: string, jobId: string) =>
      request<HarnessJobInspect>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/jobs/${encodeURIComponent(jobId)}`,
      ),
    stopProjectHarnessJob: (slug: string, jobId: string) =>
      request<{ success: boolean; message: string; taskId: string; jobId: string }>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/jobs/${encodeURIComponent(jobId)}/stop`,
        {
          method: 'POST',
        },
      ),
    stopHarnessJob: (jobId: string) =>
      request<{ success: boolean; message: string; taskId: string; jobId: string }>(
        `/api/v1/harness/jobs/${encodeURIComponent(jobId)}/stop`,
        {
          method: 'POST',
        },
      ),
    listProjectHistory: (slug: string) =>
      request<HarnessHistorySummary[]>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/history`,
      ),
    getProjectHistoryDetail: (slug: string, runId: string) =>
      request<HarnessRunResponse>(
        `/api/v1/projects/${encodeURIComponent(slug)}/harness/history/${encodeURIComponent(runId)}`,
      ),
    subscribeProjectEvents: (
      slug: string,
      onEvent: (event: HarnessProjectEvent) => void,
      onError?: (err: globalThis.Event) => void,
    ) => subscribeProjectEvents(baseUrl, secret, slug, onEvent, onError),
    subscribeJobLiveStream: (
      slug: string,
      jobId: string,
      onEvent: (event: HarnessJobLiveEvent) => void,
      onComplete?: () => void,
      onError?: (err: globalThis.Event) => void,
    ) => subscribeJobLiveStream(baseUrl, secret, slug, jobId, onEvent, onComplete, onError),
  }
}


