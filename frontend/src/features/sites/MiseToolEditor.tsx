import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { queryKeys } from '../../lib/query-client'

export function MiseToolEditor({
	value,
	onChange,
}: {
	value: string
	onChange: (value: string) => void
}) {
	const [tools, setTools] = useState(() => rows(value))
	useEffect(() => setTools(rows(value)), [value])
	const update = (next: string[]) => {
		setTools(next)
		onChange(
			next
				.map((tool) => tool.trim())
				.filter(Boolean)
				.join('\n'),
		)
	}
	return (
		<div className="sm:col-span-2">
			<div className="flex items-center justify-between gap-4">
				<div>
					<h3 className="text-sm font-medium">Dependencies</h3>
					<p className="mt-1 text-xs text-muted">
						Saved entries override automatic dependency detection.
					</p>
				</div>
				<button
					type="button"
					onClick={() => setTools((current) => [...current, ''])}
					className="inline-flex size-9 items-center justify-center border border-border bg-background text-xl text-primary hover:border-primary"
					aria-label="Add dependency"
				>
					+
				</button>
			</div>
			<div className="mt-4 space-y-2">
				{tools.map((tool, index) => (
					<ToolRow
						key={index}
						tool={tool}
						onChange={(next) =>
							update(
								tools.map((item, itemIndex) =>
									itemIndex === index ? next : item,
								),
							)
						}
						onRemove={() =>
							update(
								tools.filter(
									(_, itemIndex) => itemIndex !== index,
								),
							)
						}
					/>
				))}
			</div>
			{tools.length === 0 && (
				<button
					type="button"
					onClick={() => setTools([''])}
					className="mt-4 text-xs font-semibold text-primary"
				>
					+ Add a tool
				</button>
			)}
		</div>
	)
}

function ToolRow({
	tool,
	onChange,
	onRemove,
}: {
	tool: string
	onChange: (value: string) => void
	onRemove: () => void
}) {
	const auth = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const [candidate, setCandidate] = useState(tool.trim())
	useEffect(() => {
		const timer = window.setTimeout(() => setCandidate(tool.trim()), 450)
		return () => window.clearTimeout(timer)
	}, [tool])
	const validation = useQuery({
		queryKey: ['mise-tool-validation', candidate],
		queryFn: () =>
			api.validateMiseTool(candidate, auth.data?.csrf_token ?? ''),
		enabled: Boolean(candidate && auth.data?.csrf_token),
		staleTime: 5 * 60 * 1000,
		retry: false,
	})
	const title = !candidate
		? 'Enter a dependency and version'
		: validation.isPending
			? 'Checking availability…'
			: validation.data?.valid
				? `Resolves to ${validation.data.resolved_version}`
				: (validation.data?.error ??
					validation.error?.message ??
					'Dependency could not be resolved')
	return (
		<div className="flex items-center gap-2">
			<span
				title={title}
				aria-label={title}
				className={`w-5 shrink-0 text-center text-base font-bold ${!candidate || validation.isPending ? 'text-muted' : validation.data?.valid ? 'text-emerald-400' : 'text-danger'}`}
			>
				{!candidate || validation.isPending
					? '·'
					: validation.data?.valid
						? '✓'
						: '×'}
			</span>
			<input
				value={tool}
				onChange={(event) => onChange(event.target.value)}
				placeholder="node@24"
				className="h-10 min-w-0 flex-1 border border-border bg-surface px-3 font-mono text-sm outline-none transition focus:border-primary"
				aria-invalid={Boolean(
					candidate && validation.data && !validation.data.valid,
				)}
			/>
			<button
				type="button"
				onClick={onRemove}
				className="h-10 border border-border px-3 text-xs text-muted hover:border-danger hover:text-danger"
				aria-label={`Remove ${tool || 'dependency'}`}
			>
				Remove
			</button>
		</div>
	)
}

function rows(value: string) {
	return value
		.split(/[,\n\s]+/)
		.map((tool) => tool.trim())
		.filter(Boolean)
}
