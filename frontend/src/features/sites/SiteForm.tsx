import { useForm } from '@tanstack/react-form'
import { useRef, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Button } from '../../components/Button'
import { api, type Site, type SiteInput } from '../../lib/api'
import { queryClient, queryKeys } from '../../lib/query-client'
import { DomainEditor } from './DomainEditor'
import { RepositoryDirectoryPicker } from './RepositoryDirectoryPicker'
import { MiseToolEditor } from './MiseToolEditor'

const empty: SiteInput = {
	name: '',
	repository_url: '',
	branch: 'main',
	project_directory: '.',
	mise_tools: '',
	detected_framework: null,
	install_command: null,
	build_command: null,
	publish_directory: '.',
	build_enabled: false,
	auto_deploy: false,
	domains: [],
}
const initial = (site?: Site): SiteInput =>
	site
		? {
				name: site.name,
				repository_url: site.repository_url,
				branch: site.branch,
				project_directory: site.project_directory,
				mise_tools: site.mise_tools,
				detected_framework: site.detected_framework,
				install_command: site.install_command,
				build_command: site.build_command,
				publish_directory: site.publish_directory,
				build_enabled: site.build_enabled,
				auto_deploy: site.auto_deploy,
				domains: site.domains,
			}
		: empty
const fieldClass =
	'mt-2 h-11 w-full border border-border bg-surface px-3 outline-none transition focus:border-primary'
const required =
	(label: string, max: number) =>
	({ value }: { value: string }) =>
		!value.trim()
			? `${label} is required.`
			: value.length > max
				? `${label} must be ${max} characters or fewer.`
				: /[\u0000-\u001f\u007f]/.test(value)
					? `${label} contains unsupported control characters.`
					: undefined
const pathRule =
	(label: string) =>
	({ value }: { value: string }) =>
		!value ||
		value.length > 512 ||
		value.startsWith('/') ||
		value.includes('\\') ||
		value.split('/').includes('..')
			? `${label} must be a relative path inside the repository.`
			: undefined
const branchRule = ({ value }: { value: string }) =>
	!value ||
	value.length > 255 ||
	value.startsWith('-') ||
	value.startsWith('/') ||
	value.endsWith('/') ||
	value.endsWith('.') ||
	value.endsWith('.lock') ||
	value.includes('//') ||
	value.includes('..') ||
	value.includes('@{') ||
	/[~^:?*[\\\s]/.test(value)
		? 'Enter a valid Git branch name.'
		: undefined

function Errors({ errors }: { errors: unknown[] }) {
	return errors.length ? (
		<span className="mt-2 block text-xs text-danger">
			{errors.map(String).join(' ')}
		</span>
	) : null
}

function repositoryName(value: string) {
	const trimmed = value.trim().replace(/[\\/]+$/, '')
	const segment =
		trimmed
			.split(/[/:]/)
			.pop()
			?.replace(/\.git$/i, '') ?? ''
	try {
		return decodeURIComponent(segment)
	} catch {
		return segment
	}
}

export function SiteForm({
	site,
	section = 'all',
}: {
	site?: Site
	section?: 'all' | 'project' | 'build' | 'domains'
}) {
	const navigate = useNavigate()
	const auth = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const github = useQuery({
		queryKey: ['github-status'],
		queryFn: api.githubStatus,
	})
	const githubRepositories = useQuery({
		queryKey: ['github-repositories'],
		queryFn: api.githubRepositories,
		enabled: github.data?.connected === true,
	})
	const inferredName = useRef(site?.name ?? '')
	const [privateRepository, setPrivateRepository] = useState(false)
	const [savedMessage, setSavedMessage] = useState<string | null>(null)
	const mutation = useMutation({
		mutationFn: (input: SiteInput) =>
			site
				? api.updateSite(site.id, input, auth.data?.csrf_token ?? '')
				: api.createSite(input, auth.data?.csrf_token ?? ''),
		onSuccess: async (saved) => {
			queryClient.setQueryData(queryKeys.site(saved.id), saved)
			await queryClient.invalidateQueries({ queryKey: queryKeys.sites })
			setSavedMessage(site ? 'Changes saved.' : 'Site created.')
			if (!site)
				await navigate({
					to: privateRepository
						? '/sites/$siteId/settings'
						: '/sites/$siteId',
					params: { siteId: saved.id },
				})
		},
	})
	const form = useForm({
		defaultValues: initial(site),
		onSubmit: ({ value }) => mutation.mutateAsync(value),
	})
	const applySuggestions = (
		result: Awaited<ReturnType<typeof api.detectBuild>>,
	) => {
		form.setFieldValue('mise_tools', result.tools.join('\n'))
		form.setFieldValue('detected_framework', result.detected_framework)
		form.setFieldValue('install_command', result.install_command)
		form.setFieldValue('build_command', result.build_command)
		form.setFieldValue('publish_directory', result.publish_directory)
		form.setFieldValue('build_enabled', Boolean(result.build_command))
	}
	const detection = useMutation({
		mutationFn: () =>
			site
				? api.detectBuild(site.id, auth.data?.csrf_token ?? '')
				: api.detectDraftBuild(
						{
							repository_url:
								form.getFieldValue('repository_url'),
							branch: form.getFieldValue('branch'),
							project_directory:
								form.getFieldValue('project_directory'),
						},
						auth.data?.csrf_token ?? '',
					),
		onSuccess: applySuggestions,
	})
	const inspection = useMutation({
		mutationFn: () =>
			api.inspectRepository(
				form.getFieldValue('repository_url'),
				auth.data?.csrf_token ?? '',
			),
		onSuccess: async (result) => {
			if (
				result.default_branch &&
				form.getFieldValue('branch') === 'main'
			)
				form.setFieldValue('branch', result.default_branch)
			await detection.mutateAsync()
		},
	})
	const showProject = section === 'all' || section === 'project'
	const showBuild = section === 'all' || section === 'build'
	const showDomains = section === 'all' || section === 'domains'

	return (
		<form
			onSubmit={(event) => {
				event.preventDefault()
				event.stopPropagation()
				void form.handleSubmit()
			}}
			className="mt-8 space-y-8"
			noValidate
		>
			{showProject && (
				<section className="grid gap-5 border border-border bg-surface p-6">
					<div>
						<h2 className="font-medium">Project source</h2>
						<p className="mt-1 text-sm text-muted">
							The repository, branch, and directory containing
							this project.
						</p>
					</div>
					{!site && (
						<div className="border border-border bg-background p-4">
							<div className="flex flex-wrap items-center justify-between gap-3">
								<div>
									<p className="text-sm font-semibold">
										GitHub
									</p>
									<p className="mt-1 text-xs text-muted">
										Connect once to browse private
										repositories and configure push
										deployments automatically.
									</p>
								</div>
								{github.data?.connected ? (
									<a
										href={github.data.install_url ?? '#'}
										className="inline-flex h-10 items-center border border-border px-4 text-xs font-semibold hover:border-primary"
									>
										Manage repositories
									</a>
								) : (
									<a
										href="/api/github/connect"
										className="inline-flex h-10 items-center bg-primary px-4 text-xs font-semibold text-primary-ink"
									>
										Connect GitHub
									</a>
								)}
							</div>
							{github.data?.connected && (
								<select
									className="mt-4 h-11 w-full border border-border bg-surface px-3 text-sm"
									defaultValue=""
									onChange={(event) => {
										const repository =
											githubRepositories.data?.find(
												(item) =>
													String(item.id) ===
													event.target.value,
											)
										if (!repository) return
										form.setFieldValue(
											'repository_url',
											repository.clone_url,
										)
										form.setFieldValue(
											'branch',
											repository.default_branch,
										)
										form.setFieldValue(
											'name',
											repositoryName(
												repository.full_name,
											),
										)
										form.setFieldValue('auto_deploy', true)
										setPrivateRepository(repository.private)
									}}
								>
									<option value="">
										Select a GitHub repository
									</option>
									{githubRepositories.data?.map(
										(repository) => (
											<option
												key={repository.id}
												value={repository.id}
											>
												{repository.full_name}
												{repository.private
													? ' (private)'
													: ''}
											</option>
										),
									)}
								</select>
							)}
							{github.data?.connected &&
								githubRepositories.data?.length === 0 && (
									<p className="mt-3 text-xs text-muted">
										Install the GitHub App on at least one
										repository, then refresh this page.
									</p>
								)}
						</div>
					)}
					<form.Field
						name="name"
						validators={{ onBlur: required('Site name', 100) }}
					>
						{(field) => (
							<label className="text-sm font-medium">
								Site name
								<input
									value={field.state.value}
									onBlur={field.handleBlur}
									onChange={(event) =>
										field.handleChange(event.target.value)
									}
									className={fieldClass}
								/>
								<Errors errors={field.state.meta.errors} />
							</label>
						)}
					</form.Field>
					<form.Field
						name="repository_url"
						validators={{
							onBlur: required('Repository URL', 2048),
						}}
					>
						{(field) => (
							<label className="text-sm font-medium">
								Repository URL
								<div className="mt-2 flex">
									<input
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) => {
											const previous =
												inferredName.current
											const next = repositoryName(
												event.target.value,
											)
											field.handleChange(
												event.target.value,
											)
											if (
												!site &&
												(!form.getFieldValue('name') ||
													form.getFieldValue(
														'name',
													) === previous)
											)
												form.setFieldValue('name', next)
											inferredName.current = next
											inspection.reset()
											detection.reset()
										}}
										placeholder="git@github.com:owner/site.git"
										className="h-11 min-w-0 flex-1 border border-border bg-surface px-3 outline-none transition focus:z-10 focus:border-primary"
									/>
									{(!privateRepository || site) && (
										<button
											type="button"
											disabled={
												!field.state.value ||
												inspection.isPending ||
												detection.isPending
											}
											onClick={() => inspection.mutate()}
											className="-ml-px inline-flex h-11 shrink-0 items-center gap-2 border border-border bg-background px-4 text-xs font-semibold text-primary transition hover:border-primary disabled:text-muted"
										>
											<svg
												viewBox="0 0 20 20"
												className="size-4"
												fill="none"
												stroke="currentColor"
												strokeWidth="1.7"
												aria-hidden="true"
											>
												<circle
													cx="8.5"
													cy="8.5"
													r="5.25"
												/>
												<path d="m12.4 12.4 4.1 4.1" />
											</svg>
											{inspection.isPending
												? 'Inspecting…'
												: detection.isPending
													? 'Detecting…'
													: 'Inspect'}
										</button>
									)}
								</div>
								<Errors errors={field.state.meta.errors} />
								{inspection.isError && (
									<span className="mt-2 block text-xs text-danger">
										{inspection.error.message}
									</span>
								)}
								{inspection.data && (
									<span className="mt-2 block text-xs text-muted">
										Found {inspection.data.branches.length}{' '}
										branches
										{inspection.data.default_branch
											? ` · default: ${inspection.data.default_branch}`
											: ''}
									</span>
								)}
							</label>
						)}
					</form.Field>
					{!site && (
						<label className="flex items-center gap-3 py-1 text-sm text-muted">
							<input
								type="checkbox"
								checked={privateRepository}
								onChange={(event) => {
									setPrivateRepository(event.target.checked)
									inspection.reset()
									detection.reset()
								}}
								className="size-4 accent-primary"
							/>
							This is a private repository
						</label>
					)}
					{!site && privateRepository && (
						<div className="border-l-2 border-primary bg-background px-4 py-4">
							<p className="text-sm font-medium">
								Deploy-key access will be configured after
								creation.
							</p>
							<p className="mt-1 text-xs leading-5 text-muted">
								Create the site, generate its read-only SSH
								deploy key, and add that key to your Git
								provider. Blank can then inspect the repository
								and suggest its build settings. Use an SSH
								repository URL such as
								git@github.com:owner/repository.git.
							</p>
						</div>
					)}
					<form.Field
						name="branch"
						validators={{ onBlur: branchRule }}
					>
						{(field) => (
							<label className="text-sm font-medium">
								Branch
								{inspection.data?.branches.length ? (
									<select
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) =>
											field.handleChange(
												event.target.value,
											)
										}
										className={fieldClass}
									>
										{!inspection.data.branches.includes(
											field.state.value,
										) && (
											<option value={field.state.value}>
												{field.state.value}
											</option>
										)}
										{inspection.data.branches.map(
											(branch) => (
												<option key={branch}>
													{branch}
												</option>
											),
										)}
									</select>
								) : (
									<input
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) =>
											field.handleChange(
												event.target.value,
											)
										}
										className={fieldClass}
									/>
								)}
								<Errors errors={field.state.meta.errors} />
							</label>
						)}
					</form.Field>
					<form.Field
						name="project_directory"
						validators={{ onBlur: pathRule('Project directory') }}
					>
						{(field) => (
							<label className="text-sm font-medium">
								Project directory
								<div className="mt-2 flex">
									<input
										value={field.state.value}
										onBlur={field.handleBlur}
										onChange={(event) =>
											field.handleChange(
												event.target.value,
											)
										}
										className="h-11 min-w-0 flex-1 border border-border bg-surface px-3 outline-none transition focus:z-10 focus:border-primary"
									/>
									{site && (
										<RepositoryDirectoryPicker
											siteId={site.id}
											branch={form.getFieldValue(
												'branch',
											)}
											value={field.state.value}
											onSelect={field.handleChange}
										/>
									)}
								</div>
								{!site && (
									<span className="mt-2 block text-xs text-muted">
										Create the site first to browse its
										repository.
									</span>
								)}
								<Errors errors={field.state.meta.errors} />
							</label>
						)}
					</form.Field>
				</section>
			)}
			{showBuild && (site || detection.data) && (
				<section className="grid gap-5 border border-border bg-surface p-6 sm:grid-cols-2">
					<div className="sm:col-span-2">
						<div className="flex flex-wrap items-start justify-between gap-4">
							<div>
								<h2 className="font-medium">
									Build and publish
								</h2>
								<p className="mt-1 text-sm text-muted">
									Dependencies, commands, and the directory
									Blank should publish.
								</p>
								<form.Field name="build_enabled">
									{(field) => (
										<label className="mt-4 flex items-center gap-3 text-sm text-muted">
											<input
												type="checkbox"
												checked={field.state.value}
												onChange={(event) =>
													field.handleChange(
														event.target.checked,
													)
												}
												className="size-4 accent-primary"
											/>
											Run a build before publishing
										</label>
									)}
								</form.Field>
							</div>
							<Button
								type="button"
								tone="quiet"
								onClick={() => detection.mutate()}
								disabled={detection.isPending}
							>
								{detection.isPending
									? 'Detecting…'
									: 'Detect settings'}
							</Button>
						</div>
						{detection.data && (
							<p className="mt-4 text-xs text-muted">
								Detected{' '}
								{detection.data.detected_framework ??
									(detection.data.build_command
										? 'a buildable web project'
										: 'a static site with no build required')}
								.{' '}
								{detection.data.repository_mise
									? 'Using repository dependency configuration.'
									: `Dependencies: ${detection.data.tools.join(', ') || 'none'}.`}
							</p>
						)}
						{detection.error && (
							<p className="mt-4 text-xs text-danger">
								{detection.error.message}
							</p>
						)}
					</div>
					<form.Subscribe
						selector={(state) => state.values.build_enabled}
					>
						{(enabled) =>
							enabled ? (
								<>
									<form.Field name="install_command">
										{(field) => (
											<label className="text-sm font-medium">
												Install command
												<input
													value={
														field.state.value ?? ''
													}
													onChange={(event) =>
														field.handleChange(
															event.target
																.value || null,
														)
													}
													className={fieldClass}
												/>
											</label>
										)}
									</form.Field>
									<form.Field name="build_command">
										{(field) => (
											<label className="text-sm font-medium">
												Build command
												<input
													value={
														field.state.value ?? ''
													}
													onChange={(event) =>
														field.handleChange(
															event.target
																.value || null,
														)
													}
													className={fieldClass}
												/>
											</label>
										)}
									</form.Field>
								</>
							) : null
						}
					</form.Subscribe>
					<form.Field name="mise_tools">
						{(field) => (
							<MiseToolEditor
								value={field.state.value}
								onChange={field.handleChange}
							/>
						)}
					</form.Field>
					<form.Field
						name="publish_directory"
						validators={{ onBlur: pathRule('Publish directory') }}
					>
						{(field) => (
							<label className="text-sm font-medium">
								Publish directory
								<input
									value={field.state.value}
									onBlur={field.handleBlur}
									onChange={(event) =>
										field.handleChange(event.target.value)
									}
									className={fieldClass}
								/>
								<Errors errors={field.state.meta.errors} />
							</label>
						)}
					</form.Field>
				</section>
			)}
			{showDomains && (
				<section className="border border-border bg-surface p-6">
					<form.Field name="domains" mode="array">
						{(field) => (
							<DomainEditor
								value={field.state.value}
								onChange={field.handleChange}
							/>
						)}
					</form.Field>
				</section>
			)}
			{mutation.error && (
				<p
					role="alert"
					className="border-l-2 border-danger pl-3 text-sm text-danger"
				>
					{mutation.error.message}
				</p>
			)}
			{savedMessage && (
				<p
					role="status"
					className="border-l-2 border-primary pl-3 text-sm text-primary"
				>
					{savedMessage}
				</p>
			)}
			<form.Subscribe
				selector={(state) =>
					[state.canSubmit, state.isSubmitting] as const
				}
			>
				{([canSubmit, isSubmitting]) => (
					<div className="flex justify-end">
						<Button
							type="submit"
							disabled={
								!canSubmit || isSubmitting || mutation.isPending
							}
						>
							{isSubmitting || mutation.isPending
								? 'Saving…'
								: site
									? 'Save changes'
									: 'Create site'}
						</Button>
					</div>
				)}
			</form.Subscribe>
		</form>
	)
}
