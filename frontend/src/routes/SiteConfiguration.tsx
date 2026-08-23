import { Dialog } from '@base-ui/react/dialog'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { AppShell } from '../components/AppShell'
import { Button } from '../components/Button'
import { api, type ChimneySite } from '../lib/api'
import { queryClient, queryKeys } from '../lib/query-client'

export function SiteConfiguration({
	siteId,
	embedded = false,
}: {
	siteId: string
	embedded?: boolean
}) {
	const auth = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const query = useQuery({
		queryKey: queryKeys.configuration(siteId),
		queryFn: () => api.configuration(siteId),
	})
	const [advanced, setAdvanced] = useState(false)
	const [config, setConfig] = useState<ChimneySite | null>(null)
	const [raw, setRaw] = useState('')
	const [checkMessage, setCheckMessage] = useState<string | null>(null)
	const [saveMessage, setSaveMessage] = useState<string | null>(null)
	useEffect(() => {
		if (query.data) {
			setConfig(query.data.config)
			setRaw(query.data.toml)
		}
	}, [query.data])
	const accept = (data: Awaited<ReturnType<typeof api.configuration>>) => {
		queryClient.setQueryData(queryKeys.configuration(siteId), data)
		queryClient.invalidateQueries({ queryKey: queryKeys.site(siteId) })
	}
	const save = useMutation({
		mutationFn: () =>
			advanced
				? api.updateConfiguration(
						siteId,
						{ toml: raw },
						auth.data?.csrf_token ?? '',
					)
				: api.updateConfiguration(
						siteId,
						{ config: config! },
						auth.data?.csrf_token ?? '',
					),
		onSuccess: (data) => {
			accept(data)
			setSaveMessage('Routing settings saved.')
		},
	})
	const importConfig = useMutation({
		mutationFn: () =>
			api.importConfiguration(siteId, auth.data?.csrf_token ?? ''),
		onSuccess: (data) => {
			accept(data)
			setCheckMessage('Repository routing configuration imported.')
		},
	})
	const check = useMutation({
		mutationFn: () =>
			api.checkUpstreamConfiguration(siteId, auth.data?.csrf_token ?? ''),
		onSuccess: (data) => {
			accept(data)
			setCheckMessage(
				data.upstream_changed
					? 'The repository routing configuration has changed since it was imported.'
					: 'The repository routing configuration matches the last imported version.',
			)
		},
	})
	if (query.isPending || !config)
		return embedded ? (
			<p className="mt-8 text-sm text-muted">Loading routing settings…</p>
		) : (
			<AppShell>
				<main className="mx-auto max-w-4xl px-6 py-14 text-sm text-muted">
					Loading routing settings…
				</main>
			</AppShell>
		)
	if (query.isError)
		return embedded ? (
			<p className="mt-8 text-sm text-danger">{query.error.message}</p>
		) : (
			<AppShell>
				<main className="mx-auto max-w-4xl px-6 py-14 text-danger">
					{query.error.message}
				</main>
			</AppShell>
		)
	const field =
		'mt-2 h-11 w-full border border-border bg-surface px-3 outline-none focus:border-primary'
	const content = (
		<>
			<div
				className={`flex flex-wrap items-end justify-between gap-4 ${embedded ? 'mt-8' : 'mt-10'}`}
			>
				<div>
					<p className="text-xs font-semibold uppercase tracking-[0.18em] text-primary">
						Runtime
					</p>
					<h2
						className={`${embedded ? 'mt-2 text-2xl' : 'mt-2 text-3xl'} font-semibold tracking-tight`}
					>
						Routing
					</h2>
					<p className="mt-2 max-w-xl text-sm leading-6 text-muted">
						Control index files, fallback behavior, HTTPS redirects,
						and advanced routing.
					</p>
				</div>
				<a
					href={`/api/sites/${siteId}/configuration/export`}
					className="text-sm font-semibold text-primary"
				>
					Export server config
				</a>
			</div>
			{query.data.upstream_changed && (
				<div className="mt-8 border-l-2 border-[#f0b45d] bg-surface px-5 py-4">
					<p className="font-medium">
						Repository routing configuration changed.
					</p>
					<p className="mt-1 text-sm text-muted">
						Your current settings remain active until you confirm an
						import.
					</p>
					<ImportRepositoryButton
						pending={importConfig.isPending}
						onConfirm={() => importConfig.mutate()}
						className="mt-3"
					/>
				</div>
			)}
			<div className="mt-8 flex flex-wrap gap-3">
				<Button
					tone={!advanced ? 'primary' : 'quiet'}
					type="button"
					onClick={() => setAdvanced(false)}
				>
					Form editor
				</Button>
				<Button
					tone={advanced ? 'primary' : 'quiet'}
					type="button"
					onClick={() => setAdvanced(true)}
				>
					Config source
				</Button>
				<ImportRepositoryButton
					pending={importConfig.isPending}
					onConfirm={() => importConfig.mutate()}
				/>
				{query.data.imported_hash && (
					<Button
						tone="quiet"
						type="button"
						onClick={() => {
							setCheckMessage(null)
							check.mutate()
						}}
						disabled={check.isPending}
					>
						{check.isPending
							? 'Checking repository…'
							: 'Check repository version'}
					</Button>
				)}
			</div>
			{checkMessage && (
				<p
					role="status"
					className={`mt-4 border-l-2 pl-4 text-sm ${query.data.upstream_changed ? 'border-[#f0b45d] text-[#f0b45d]' : 'border-primary text-muted'}`}
				>
					{checkMessage}
				</p>
			)}
			{!advanced ? (
				<div className="mt-6 space-y-5">
					<section className="grid gap-5 border border-border bg-surface p-6 sm:grid-cols-2">
						<div className="sm:col-span-2">
							<h3 className="font-medium">Files and HTTPS</h3>
							<p className="mt-1 text-xs text-muted">
								Choose how files are resolved when a request
								reaches this site.
							</p>
						</div>
						<label className="text-sm font-medium">
							Site root
							<input
								value={config.root}
								onChange={(e) =>
									setConfig({
										...config,
										root: e.target.value,
									})
								}
								className={field}
							/>
						</label>
						<label className="text-sm font-medium">
							Default index file
							<input
								value={config.default_index_file ?? ''}
								onChange={(e) =>
									setConfig({
										...config,
										default_index_file:
											e.target.value || null,
									})
								}
								className={field}
							/>
						</label>
						<label className="text-sm font-medium">
							Single-page app fallback
							<input
								value={config.fallback_file ?? ''}
								onChange={(e) =>
									setConfig({
										...config,
										fallback_file: e.target.value || null,
									})
								}
								placeholder="index.html"
								className={field}
							/>
						</label>
						<label className="flex items-center gap-3 self-end pb-3 text-sm">
							<input
								type="checkbox"
								checked={
									config.https_config?.auto_redirect ?? true
								}
								onChange={(e) =>
									setConfig({
										...config,
										https_config: {
											auto_redirect: e.target.checked,
											cert_file: null,
											key_file: null,
											ca_file: null,
										},
									})
								}
								className="size-4 accent-primary"
							/>
							Redirect HTTP to HTTPS
						</label>
					</section>
					<KeyValueEditor
						title="Redirects"
						description="Send a request path to another path or URL."
						keyPlaceholder="/docs"
						valuePlaceholder="https://docs.example.com"
						value={config.redirects}
						onChange={(redirects) =>
							setConfig({ ...config, redirects })
						}
					/>
					<KeyValueEditor
						title="Rewrites"
						description="Serve content from another internal path without changing the browser URL."
						keyPlaceholder="/legacy"
						valuePlaceholder="/archive/index.html"
						value={config.rewrites}
						onChange={(rewrites) =>
							setConfig({ ...config, rewrites })
						}
					/>
					<KeyValueEditor
						title="Response headers"
						description="Attach an HTTP response header to every response from this site."
						keyPlaceholder="X-Frame-Options"
						valuePlaceholder="DENY"
						value={config.response_headers}
						onChange={(response_headers) =>
							setConfig({ ...config, response_headers })
						}
					/>
				</div>
			) : (
				<section className="mt-6">
					<textarea
						spellCheck={false}
						value={raw}
						onChange={(e) => setRaw(e.target.value)}
						className="min-h-[32rem] w-full border border-border bg-[#080a0d] p-5 font-mono text-sm leading-6 outline-none focus:border-primary"
					/>
				</section>
			)}
			{(save.error || importConfig.error || check.error) && (
				<p className="mt-5 border-l-2 border-danger pl-3 text-sm text-danger">
					{save.error?.message ??
						importConfig.error?.message ??
						check.error?.message}
				</p>
			)}
			{saveMessage && (
				<p
					role="status"
					className="mt-5 border-l-2 border-primary pl-3 text-sm text-primary"
				>
					{saveMessage}
				</p>
			)}
			<div className="mt-6 flex justify-end">
				<Button
					onClick={() => {
						setSaveMessage(null)
						save.mutate()
					}}
					disabled={save.isPending}
				>
					{save.isPending ? 'Validating…' : 'Save routing settings'}
				</Button>
			</div>
		</>
	)
	return embedded ? (
		content
	) : (
		<AppShell>
			<main className="mx-auto max-w-4xl px-6 py-12">
				<Link
					to="/sites/$siteId"
					params={{ siteId }}
					className="text-sm text-muted hover:text-ink"
				>
					← Site overview
				</Link>
				{content}
			</main>
		</AppShell>
	)
}

function ImportRepositoryButton({
	pending,
	onConfirm,
	className = '',
}: {
	pending: boolean
	onConfirm: () => void
	className?: string
}) {
	return (
		<Dialog.Root>
			<Dialog.Trigger
				render={
					<Button
						tone="quiet"
						type="button"
						className={className}
						disabled={pending}
					/>
				}
			>
				{pending ? 'Importing…' : 'Import from repository'}
			</Dialog.Trigger>
			<Dialog.Portal>
				<Dialog.Backdrop className="fixed inset-0 bg-black/70" />
				<Dialog.Popup className="fixed left-1/2 top-1/2 w-[min(30rem,calc(100%-2rem))] -translate-x-1/2 -translate-y-1/2 border border-border bg-surface p-6">
					<Dialog.Title className="text-lg font-semibold">
						Replace active routing settings?
					</Dialog.Title>
					<Dialog.Description className="mt-2 text-sm leading-6 text-muted">
						This imports chimney.toml from the repository and
						replaces the active routing configuration, including
						domains, fallback behavior, redirects, rewrites, and
						response headers.
					</Dialog.Description>
					<p className="mt-4 border-l-2 border-[#f0b45d] pl-4 text-xs leading-5 text-muted">
						The current dashboard configuration remains active if
						you cancel.
					</p>
					<div className="mt-6 flex justify-end gap-3">
						<Dialog.Close
							render={<Button tone="quiet" type="button" />}
						>
							Cancel
						</Dialog.Close>
						<Dialog.Close
							render={
								<Button type="button" onClick={onConfirm} />
							}
						>
							Replace and import
						</Dialog.Close>
					</div>
				</Dialog.Popup>
			</Dialog.Portal>
		</Dialog.Root>
	)
}

function KeyValueEditor({
	title,
	description,
	keyPlaceholder,
	valuePlaceholder,
	value,
	onChange,
}: {
	title: string
	description: string
	keyPlaceholder: string
	valuePlaceholder: string
	value: Record<string, string>
	onChange: (value: Record<string, string>) => void
}) {
	const [rows, setRows] = useState(() =>
		Object.entries(value).map(([key, itemValue]) => ({
			key,
			value: itemValue,
		})),
	)
	useEffect(
		() =>
			setRows(
				Object.entries(value).map(([key, itemValue]) => ({
					key,
					value: itemValue,
				})),
			),
		[value],
	)
	const update = (next: Array<{ key: string; value: string }>) => {
		setRows(next)
		onChange(
			Object.fromEntries(
				next
					.map((row) => [row.key.trim(), row.value])
					.filter(([key]) => key),
			),
		)
	}
	return (
		<section className="border border-border bg-surface p-6">
			<div className="flex items-start justify-between gap-4">
				<div>
					<h3 className="font-medium">{title}</h3>
					<p className="mt-1 text-xs leading-5 text-muted">
						{description}
					</p>
				</div>
				<button
					type="button"
					onClick={() =>
						setRows((current) => [
							...current,
							{ key: '', value: '' },
						])
					}
					className="inline-flex size-9 shrink-0 items-center justify-center border border-border bg-background text-xl text-primary hover:border-primary"
					aria-label={`Add ${title.toLowerCase()} entry`}
				>
					+
				</button>
			</div>
			<div className="mt-5 space-y-2">
				{rows.map((row, index) => (
					<div
						key={index}
						className="grid gap-2 sm:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)_auto]"
					>
						<input
							value={row.key}
							onChange={(event) =>
								update(
									rows.map((item, itemIndex) =>
										itemIndex === index
											? {
													...item,
													key: event.target.value,
												}
											: item,
									),
								)
							}
							placeholder={keyPlaceholder}
							aria-label={`${title} key`}
							className="h-10 min-w-0 border border-border bg-background px-3 font-mono text-xs outline-none focus:border-primary"
						/>
						<input
							value={row.value}
							onChange={(event) =>
								update(
									rows.map((item, itemIndex) =>
										itemIndex === index
											? {
													...item,
													value: event.target.value,
												}
											: item,
									),
								)
							}
							placeholder={valuePlaceholder}
							aria-label={`${title} value`}
							className="h-10 min-w-0 border border-border bg-background px-3 font-mono text-xs outline-none focus:border-primary"
						/>
						<button
							type="button"
							onClick={() =>
								update(
									rows.filter(
										(_, itemIndex) => itemIndex !== index,
									),
								)
							}
							className="h-10 border border-border px-3 text-xs text-muted hover:border-danger hover:text-danger"
						>
							Remove
						</button>
					</div>
				))}
			</div>
			{rows.length === 0 && (
				<p className="mt-5 text-xs text-muted">
					No {title.toLowerCase()} configured.
				</p>
			)}
		</section>
	)
}
