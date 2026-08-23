export type AuthStatus = {
	setup_required: boolean
	authenticated: boolean
	identifier: string | null
	csrf_token: string | null
}

type Credentials = { identifier: string; password: string }

export type Site = {
	id: string
	name: string
	repository_url: string
	branch: string
	project_directory: string
	mise_tools: string
	detected_framework: string | null
	install_command: string | null
	build_command: string | null
	publish_directory: string
	build_enabled: boolean
	auto_deploy: boolean
	domains: string[]
	created_at: string
	updated_at: string
}

export type SiteInput = Omit<Site, 'id' | 'created_at' | 'updated_at'>

export type RemoteInspection = {
	default_branch: string | null
	branches: string[]
}
export type CommitMetadata = {
	sha: string
	message: string
	author_name: string
	author_email: string
	authored_at: string
}
export type RepositoryRefresh = {
	inspection: RemoteInspection
	commit: CommitMetadata
}
export type BuildSuggestions = {
	repository_mise: boolean
	tools: string[]
	install_command: string | null
	build_command: string | null
	publish_directory: string
	detected_framework: string | null
}
export type MiseToolValidation = {
	tool: string
	valid: boolean
	resolved_version: string | null
	error: string | null
}
export type Deployment = {
	id: string
	site_id: string
	commit_sha: string | null
	commit_message: string | null
	commit_author: string | null
	status: string
	triggered_by: string
	build_settings_snapshot: string
	config_snapshot: string | null
	release_path: string | null
	error_summary: string | null
	log: string
	created_at: string
	started_at: string | null
	finished_at: string | null
	rollback_of_deployment_id: string | null
}
export type DnsResult = {
	domain: string
	addresses: string[]
	canonical_names: string[]
	matches_expected: boolean | null
	error: string | null
}
export type SiteAnalytics = {
	total_requests: number
	error_requests: number
	average_duration_ms: number
	daily: {
		day: string
		requests: number
		errors: number
		average_duration_ms: number
	}[]
	requests: {
		id: number
		created_at: string
		host: string | null
		method: string
		path: string
		status: number
		duration_ms: number
		protocol: string
		ip_address: string | null
		country: string | null
		device_type: string
		user_agent: string | null
		referer: string | null
	}[]
	request_total: number
	request_offset: number
	request_limit: number
}
export type RepositoryEntry = {
	name: string
	path: string
	kind: 'tree' | 'blob' | string
}
export type Health = {
	status: string
	database: string
	chimney: { state: string; active_sites: number; error: string | null }
	site_http_port: number
	site_https_port: number | null
}

export type ChimneySite = {
	name: string
	root: string
	domain_names: string[]
	fallback_file: string | null
	default_index_file: string | null
	https_config: {
		auto_redirect: boolean
		cert_file: string | null
		key_file: string | null
		ca_file: string | null
	} | null
	response_headers: Record<string, string>
	redirects: Record<string, string>
	rewrites: Record<string, string>
}
export type ChimneyConfiguration = {
	config: ChimneySite
	toml: string
	origin: 'generated' | 'repository' | 'dashboard'
	imported_hash: string | null
	imported_commit: string | null
	upstream_hash: string | null
	upstream_changed: boolean
	updated_at: string
}

export class ApiError extends Error {
	constructor(
		public status: number,
		message: string,
	) {
		super(message)
	}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await fetch(`/api${path}`, {
		credentials: 'same-origin',
		...init,
		headers: { 'content-type': 'application/json', ...init?.headers },
	})
	if (!response.ok) {
		const body = (await response
			.json()
			.catch(() => ({ error: response.statusText }))) as {
			error?: string
		}
		throw new ApiError(response.status, body.error ?? 'Request failed')
	}
	return response.status === 204
		? (undefined as T)
		: (response.json() as Promise<T>)
}

export const api = {
	health: () => request<Health>('/health'),
	authStatus: () => request<AuthStatus>('/auth/status'),
	setup: (credentials: Credentials) =>
		request<AuthStatus>('/auth/setup', {
			method: 'POST',
			body: JSON.stringify(credentials),
		}),
	login: (credentials: Credentials) =>
		request<AuthStatus>('/auth/login', {
			method: 'POST',
			body: JSON.stringify(credentials),
		}),
	logout: (csrfToken: string) =>
		request<void>('/auth/logout', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	sites: () => request<Site[]>('/sites'),
	site: (id: string) => request<Site>(`/sites/${id}`),
	createSite: (input: SiteInput, csrfToken: string) =>
		request<Site>('/sites', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify(input),
		}),
	updateSite: (id: string, input: SiteInput, csrfToken: string) =>
		request<Site>(`/sites/${id}`, {
			method: 'PUT',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify(input),
		}),
	deleteSite: (id: string, csrfToken: string) =>
		request<void>(`/sites/${id}`, {
			method: 'DELETE',
			headers: { 'x-csrf-token': csrfToken },
		}),
	inspectRepository: (repositoryUrl: string, csrfToken: string) =>
		request<RemoteInspection>('/repositories/inspect', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify({ repository_url: repositoryUrl }),
		}),
	refreshRepository: (siteId: string, csrfToken: string) =>
		request<RepositoryRefresh>(`/sites/${siteId}/repository/refresh`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	repositoryTree: (siteId: string, branch: string, path: string) =>
		request<RepositoryEntry[]>(
			`/sites/${siteId}/repository/tree?branch=${encodeURIComponent(branch)}&path=${encodeURIComponent(path === '.' ? '' : path)}`,
		),
	detectBuild: (siteId: string, csrfToken: string) =>
		request<BuildSuggestions>(`/sites/${siteId}/repository/detect`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	detectDraftBuild: (
		input: {
			repository_url: string
			branch: string
			project_directory: string
		},
		csrfToken: string,
	) =>
		request<BuildSuggestions>('/repositories/detect', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify(input),
		}),
	validateMiseTool: (tool: string, csrfToken: string) =>
		request<MiseToolValidation>('/mise/tools/validate', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify({ tool }),
		}),
	deployments: (
		siteId: string,
		filters: {
			search?: string
			status?: string
			offset?: number
			limit?: number
		} = {},
	) =>
		request<{
			items: Deployment[]
			total: number
			offset: number
			limit: number
		}>(
			`/sites/${siteId}/deployments?${new URLSearchParams(
				Object.entries(filters)
					.filter(([, value]) => value !== undefined && value !== '')
					.map(([key, value]) => [key, String(value)]),
			)}`,
		),
	analytics: (
		siteId: string,
		filters: {
			search?: string
			status?: string
			method?: string
			device?: string
			country?: string
			offset?: number
			limit?: number
		} = {},
	) =>
		request<SiteAnalytics>(
			`/sites/${siteId}/analytics?${new URLSearchParams(
				Object.entries(filters)
					.filter(([, value]) => value !== undefined && value !== '')
					.map(([key, value]) => [key, String(value)]),
			)}`,
		),
	deployment: (id: string) => request<Deployment>(`/deployments/${id}`),
	createDeployment: (siteId: string, csrfToken: string) =>
		request<Deployment>(`/sites/${siteId}/deployments`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	rollbackDeployment: (id: string, csrfToken: string) =>
		request<Deployment>(`/deployments/${id}/rollback`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	checkDns: (domains: string[], csrfToken: string) =>
		request<DnsResult[]>('/dns/check', {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify({ domains }),
		}),
	deployKey: (siteId: string) =>
		request<{ public_key: string | null }>(
			`/sites/${siteId}/repository/deploy-key`,
		),
	generateDeployKey: (siteId: string, csrfToken: string) =>
		request<{ public_key: string }>(
			`/sites/${siteId}/repository/deploy-key`,
			{ method: 'POST', headers: { 'x-csrf-token': csrfToken } },
		),
	deleteDeployKey: (siteId: string, csrfToken: string) =>
		request<void>(`/sites/${siteId}/repository/deploy-key`, {
			method: 'DELETE',
			headers: { 'x-csrf-token': csrfToken },
		}),
	configuration: (siteId: string) =>
		request<ChimneyConfiguration>(`/sites/${siteId}/configuration`),
	updateConfiguration: (
		siteId: string,
		input: { toml: string } | { config: ChimneySite },
		csrfToken: string,
	) =>
		request<ChimneyConfiguration>(`/sites/${siteId}/configuration`, {
			method: 'PUT',
			headers: { 'x-csrf-token': csrfToken },
			body: JSON.stringify(input),
		}),
	importConfiguration: (siteId: string, csrfToken: string) =>
		request<ChimneyConfiguration>(`/sites/${siteId}/configuration/import`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
	checkUpstreamConfiguration: (siteId: string, csrfToken: string) =>
		request<ChimneyConfiguration>(
			`/sites/${siteId}/configuration/check-upstream`,
			{ method: 'POST', headers: { 'x-csrf-token': csrfToken } },
		),
	renewCertificates: (siteId: string, csrfToken: string) =>
		request<{ status: string }>(`/sites/${siteId}/certificates/renew`, {
			method: 'POST',
			headers: { 'x-csrf-token': csrfToken },
		}),
}
