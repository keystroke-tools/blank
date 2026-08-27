import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { AppShell } from '../components/AppShell'
import { api, type Site } from '../lib/api'
import { queryKeys } from '../lib/query-client'

export function Dashboard() {
	const [tab, setTab] = useState<'all' | 'published' | 'setup'>('all')
	const [search, setSearch] = useState('')
	const sites = useQuery({ queryKey: queryKeys.sites, queryFn: api.sites })
	const github = useQuery({
		queryKey: ['github-status'],
		queryFn: api.githubStatus,
	})
	const filteredSites = useMemo(() => {
		const needle = search.trim().toLowerCase()
		return (
			sites.data?.filter(
				(site) =>
					(tab === 'all' ||
						(tab === 'published'
							? site.domains.length > 0
							: site.domains.length === 0)) &&
					(!needle ||
						[
							site.name,
							site.repository_url,
							site.branch,
							...site.domains,
						].some((value) =>
							value.toLowerCase().includes(needle),
						)),
			) ?? []
		)
	}, [sites.data, tab, search])
	return (
		<AppShell>
			<main className="mx-auto max-w-6xl px-6 py-14">
				<div className="flex flex-wrap items-end justify-between gap-5">
					<div>
						<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted">
							Workspace
						</p>
						<h1 className="mt-2 text-3xl font-semibold tracking-tight">
							Your sites
						</h1>
						<p className="mt-2 text-sm text-muted">
							Deploy, inspect, and manage every static project.
						</p>
					</div>
					<Link
						to="/sites/new"
						className="inline-flex h-11 items-center gap-2 bg-primary px-5 text-sm font-semibold text-primary-ink hover:bg-[#f5ffc2]"
					>
						<span className="text-lg leading-none">+</span> New site
					</Link>
				</div>
				<section className="mt-8 flex flex-wrap items-center justify-between gap-4 border border-border bg-surface px-5 py-4">
					<div>
						<p className="text-sm font-semibold">
							GitHub integration
						</p>
						<p className="mt-1 text-xs text-muted">
							{github.data?.connected
								? `Connected as ${github.data.app_slug}. Install or update repository access at any time.`
								: 'Connect GitHub after setup to link existing sites, access private repositories, and receive push deployments.'}
						</p>
						{github.isError && (
							<p className="mt-2 text-xs text-danger">
								{github.error.message}
							</p>
						)}
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
							href="/api/github/connect?return_to=%2Fdashboard"
							className="inline-flex h-10 items-center bg-primary px-4 text-xs font-semibold text-primary-ink"
						>
							Connect GitHub
						</a>
					)}
				</section>
				{sites.isPending && <LoadingGrid />}
				{sites.isError && (
					<p
						role="alert"
						className="mt-8 border-l-2 border-danger pl-4 text-sm text-danger"
					>
						{sites.error.message}
					</p>
				)}
				{sites.data?.length === 0 && (
					<section className="mt-10 border border-dashed border-border bg-surface/40 px-6 py-20 text-center">
						<div className="mx-auto flex size-14 items-center justify-center border border-border bg-background text-2xl text-primary">
							+
						</div>
						<h2 className="mt-5 font-medium">
							Deploy your first site
						</h2>
						<p className="mx-auto mt-2 max-w-md text-sm leading-6 text-muted">
							Connect a Git repository and Blank will detect how
							to build and publish it.
						</p>
						<Link
							to="/sites/new"
							className="mt-6 inline-flex h-10 items-center border border-primary px-4 text-sm font-semibold text-primary hover:bg-primary hover:text-primary-ink"
						>
							Add a repository
						</Link>
					</section>
				)}
				{sites.data && sites.data.length > 0 && (
					<>
						<nav
							aria-label="Site filters"
							role="tablist"
							className="mt-10 flex gap-1 overflow-x-auto border-b border-border"
						>
							{(
								[
									['all', 'All sites'],
									['published', 'With domains'],
									['setup', 'Needs setup'],
								] as const
							).map(([value, label]) => (
								<button
									key={value}
									role="tab"
									type="button"
									onClick={() => setTab(value)}
									className={`border-b-2 px-4 py-3 text-sm font-semibold whitespace-nowrap transition ${tab === value ? 'border-primary text-ink' : 'border-transparent text-muted hover:text-ink'}`}
									aria-selected={tab === value}
								>
									{label}
									<span className="ml-2 font-mono text-xs text-muted">
										{value === 'all'
											? sites.data.length
											: value === 'published'
												? sites.data.filter(
														(site) =>
															site.domains
																.length > 0,
													).length
												: sites.data.filter(
														(site) =>
															site.domains
																.length === 0,
													).length}
									</span>
								</button>
							))}
						</nav>
						<div className="mt-5">
							<label htmlFor="site-search" className="sr-only">
								Search sites
							</label>
							<input
								id="site-search"
								value={search}
								onChange={(event) =>
									setSearch(event.target.value)
								}
								placeholder="Search sites, domains, repositories, or branches"
								className="h-11 w-full border border-border bg-surface px-4 text-sm outline-none focus:border-primary"
							/>
						</div>
						{filteredSites.length > 0 ? (
							<div className="mt-7 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
								{filteredSites.map((site) => (
									<SiteCard key={site.id} site={site} />
								))}
							</div>
						) : (
							<section className="mt-7 border border-dashed border-border bg-surface/40 px-6 py-16 text-center">
								<h2 className="font-medium">
									Nothing here yet
								</h2>
								<p className="mt-2 text-sm text-muted">
									Sites will appear in this view when they
									match the filter.
								</p>
							</section>
						)}
					</>
				)}
			</main>
		</AppShell>
	)
}

function SiteCard({ site }: { site: Site }) {
	const repository = repositoryLabel(site.repository_url)
	return (
		<Link
			to="/sites/$siteId"
			params={{ siteId: site.id }}
			className="group relative flex min-h-72 flex-col overflow-hidden border border-border bg-surface p-6 transition hover:-translate-y-0.5 hover:border-primary hover:shadow-[0_12px_40px_rgba(0,0,0,0.22)]"
		>
			<div className="absolute inset-x-0 top-0 h-px bg-primary opacity-0 transition group-hover:opacity-100" />
			<div className="flex items-start justify-between gap-4">
				<FrameworkMark
					framework={site.detected_framework}
					name={site.name}
				/>
				<span className="mt-1 text-lg text-muted transition group-hover:translate-x-1 group-hover:text-primary">
					→
				</span>
			</div>
			<div className="mt-6">
				<h2 className="truncate text-lg font-semibold group-hover:text-primary">
					{site.name}
				</h2>
				<p className="mt-2 truncate text-sm text-muted">
					{site.domains[0] ?? 'No domain configured'}
				</p>
			</div>
			<div className="mt-auto border-t border-border pt-5">
				<p className="truncate text-xs text-ink">{repository}</p>
				<div className="mt-2 flex min-w-0 items-center gap-2 font-mono text-[11px] text-muted">
					<span className="truncate">{site.branch}</span>
					<span>·</span>
					<span className="truncate">
						{site.project_directory === '.'
							? 'repository root'
							: site.project_directory}
					</span>
				</div>
				{site.domains.length > 1 && (
					<p className="mt-3 text-[11px] text-muted">
						+{site.domains.length - 1} more{' '}
						{site.domains.length === 2 ? 'domain' : 'domains'}
					</p>
				)}
			</div>
		</Link>
	)
}

function FrameworkMark({
	framework,
	name,
}: {
	framework: string | null
	name: string
}) {
	const kind = framework?.toLowerCase()
	if (kind === 'vite')
		return (
			<div
				className="flex size-12 items-center justify-center border border-[#bd8cff]/35 bg-[#8d5cff]/10"
				title="Vite"
			>
				<svg viewBox="0 0 24 24" className="size-7" aria-hidden="true">
					<path
						d="M13.2 2 5 3.5l5.7 17.9 2.1-7.1h3.8L13.2 2Z"
						fill="#bd8cff"
					/>
					<path
						d="m14.4 2.8-3.8 7.4h3l-1.2 5.1 5.8-8.2h-3.4l1.4-4.3Z"
						fill="#efff9a"
					/>
				</svg>
			</div>
		)
	if (kind === 'astro')
		return (
			<div
				className="flex size-12 items-center justify-center border border-[#ff8a65]/35 bg-[#ff6b45]/10"
				title="Astro"
			>
				<svg viewBox="0 0 24 24" className="size-8" aria-hidden="true">
					<path
						d="M8.2 15.5 11.4 5c.2-.7 1.3-.7 1.5 0l3.3 10.5c-2.5-1.3-5.4-1.3-8 0Z"
						fill="#fff"
					/>
					<path
						d="M7.1 18.2c1.5-.7 3-.9 4.3-.6-.7.6-1.1 1.4-1.2 2.5-1.4-.2-2.4-.8-3.1-1.9Zm9.8 0c-1.5-.7-3-.9-4.3-.6.7.6 1.1 1.4 1.2 2.5 1.4-.2 2.4-.8 3.1-1.9Z"
						fill="#ff6b45"
					/>
				</svg>
			</div>
		)
	return (
		<div className="flex size-12 items-center justify-center border border-border bg-background text-lg font-semibold text-primary">
			{name.trim().charAt(0).toUpperCase() || 'B'}
		</div>
	)
}

function repositoryLabel(value: string) {
	return value
		.replace(/^https?:\/\//, '')
		.replace(/^git@([^:]+):/, '$1/')
		.replace(/\.git$/, '')
}

function LoadingGrid() {
	return (
		<div className="mt-10 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
			{[0, 1, 2].map((item) => (
				<div
					key={item}
					className="h-72 animate-pulse border border-border bg-surface"
				/>
			))}
		</div>
	)
}
