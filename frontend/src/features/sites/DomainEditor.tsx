import { useMutation, useQuery } from '@tanstack/react-query'
import { type KeyboardEvent, useState } from 'react'
import { Button } from '../../components/Button'
import { api } from '../../lib/api'
import { queryKeys } from '../../lib/query-client'

function normalize(value: string) {
	return value.trim().toLowerCase().replace(/\.$/, '')
}
function valid(value: string) {
	return (
		value.length <= 253 &&
		value.includes('.') &&
		value
			.split('.')
			.every(
				(label) =>
					Boolean(label) &&
					label.length <= 63 &&
					!label.startsWith('-') &&
					!label.endsWith('-') &&
					/^[a-z0-9-]+$/.test(label),
			)
	)
}
function wwwAlias(value: string) {
	return value.split('.').length === 2 ? `www.${value}` : null
}

export function DomainEditor({
	value,
	onChange,
}: {
	value: string[]
	onChange: (domains: string[]) => void
}) {
	const auth = useQuery({
		queryKey: queryKeys.auth,
		queryFn: api.authStatus,
	})
	const [draft, setDraft] = useState('')
	const [error, setError] = useState<string | null>(null)
	const [notice, setNotice] = useState<string | null>(null)
	const dns = useMutation({
		mutationFn: () => api.checkDns(value, auth.data?.csrf_token ?? ''),
	})
	function add(raw = draft) {
		const candidates = raw
			.split(/[\s,]+/)
			.map(normalize)
			.filter(Boolean)
		const invalid = candidates.find((domain) => !valid(domain))
		if (invalid) {
			setError(`“${invalid}” is not a valid hostname.`)
			return
		}
		const aliases = candidates.flatMap((domain) => {
			const alias = wwwAlias(domain)
			return alias &&
				!candidates.includes(alias) &&
				!value.includes(alias)
				? [alias]
				: []
		})
		onChange([...new Set([...value, ...candidates, ...aliases])])
		setDraft('')
		setError(null)
		setNotice(aliases.length ? `Also added ${aliases.join(', ')}.` : null)
		dns.reset()
	}
	function keyDown(event: KeyboardEvent<HTMLInputElement>) {
		if (event.key === 'Enter' || event.key === ',') {
			event.preventDefault()
			add()
		}
		if (event.key === 'Backspace' && !draft && value.length)
			onChange(value.slice(0, -1))
	}
	return (
		<div className="sm:col-span-2">
			<div className="flex items-center justify-between gap-4">
				<div>
					<p className="text-sm font-medium">Domains</p>
					<p className="mt-1 text-xs text-muted">
						Add hostnames only, without protocols or paths. Bare
						domains also include their www hostname.
					</p>
				</div>
				{value.length > 0 && (
					<Button
						type="button"
						tone="quiet"
						onClick={() => dns.mutate()}
						disabled={dns.isPending}
					>
						{dns.isPending ? 'Checking DNS…' : 'Check DNS'}
					</Button>
				)}
			</div>
			<div className="mt-3 flex min-h-12 flex-wrap items-center gap-2 border border-border bg-background p-2 focus-within:border-primary">
				{value.map((domain) => (
					<span
						key={domain}
						className="inline-flex h-8 items-center gap-2 bg-surface-muted px-3 text-xs"
					>
						<span>{domain}</span>
						<button
							type="button"
							aria-label={`Remove ${domain}`}
							onClick={() => {
								onChange(
									value.filter((item) => item !== domain),
								)
								dns.reset()
							}}
							className="text-muted hover:text-primary"
						>
							×
						</button>
					</span>
				))}
				<input
					value={draft}
					onChange={(event) => setDraft(event.target.value)}
					onKeyDown={keyDown}
					onBlur={() => {
						if (draft.trim()) add()
					}}
					placeholder={value.length ? 'Add another…' : 'example.com'}
					className="h-8 min-w-44 flex-1 bg-transparent px-1 text-sm outline-none"
				/>
				{draft && (
					<button
						type="button"
						onMouseDown={(event) => event.preventDefault()}
						onClick={() => add()}
						className="h-8 bg-primary px-3 text-xs font-semibold text-primary-ink"
					>
						Add
					</button>
				)}
			</div>
			{error && <p className="mt-2 text-xs text-danger">{error}</p>}
			{notice && <p className="mt-2 text-xs text-primary">{notice}</p>}
			{dns.error && (
				<p className="mt-3 text-xs text-danger">{dns.error.message}</p>
			)}
			{dns.data && (
				<div className="mt-4 divide-y divide-border border-y border-border">
					{dns.data.map((result) => (
						<div
							key={result.domain}
							className="grid gap-2 py-3 text-xs sm:grid-cols-[minmax(10rem,1fr)_2fr_auto]"
						>
							<span className="font-semibold">
								{result.domain}
							</span>
							<span className="break-all text-muted">
								{result.error ??
									([
										...result.canonical_names.map(
											(name) => `CNAME ${name}`,
										),
										...result.addresses,
									].join(' · ') ||
										'No records')}
							</span>
							<span
								className={
									result.matches_expected === false ||
									result.error
										? 'text-danger'
										: result.matches_expected
											? 'text-primary'
											: 'text-muted'
								}
							>
								{result.error
									? 'Unresolved'
									: result.matches_expected === true
										? 'Points to Blank'
										: result.matches_expected === false
											? 'Wrong address'
											: 'Expected IPs not configured'}
							</span>
						</div>
					))}
				</div>
			)}
		</div>
	)
}
