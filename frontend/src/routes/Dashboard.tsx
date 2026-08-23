import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { AppShell } from '../components/AppShell'
import { api } from '../lib/api'
import { queryKeys } from '../lib/query-client'

export function Dashboard() {
  const sites = useQuery({ queryKey: queryKeys.sites, queryFn: api.sites })
  return <AppShell><main className="mx-auto max-w-6xl px-6 py-14">
    <div className="flex items-end justify-between gap-4"><div><p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted">Workspace</p><h1 className="mt-2 text-3xl font-semibold tracking-tight">Sites</h1></div><Link to="/sites/new" className="inline-flex h-10 items-center bg-primary px-4 text-sm font-semibold text-primary-ink hover:bg-[#f5ffc2]">New site</Link></div>
    {sites.isPending && <p className="mt-8 text-sm text-muted">Loading sites…</p>}
    {sites.isError && <p role="alert" className="mt-8 text-sm text-danger">{sites.error.message}</p>}
    {sites.data?.length === 0 && <section className="mt-8 border border-dashed border-border bg-surface/40 px-6 py-16 text-center"><h2 className="font-medium">No sites yet</h2><p className="mx-auto mt-2 max-w-md text-sm leading-6 text-muted">Add a Git repository and define where Blank should find its frontend.</p></section>}
    <div className="mt-8 divide-y divide-border border-y border-border">
      {sites.data?.map((site) => <Link key={site.id} to="/sites/$siteId" params={{ siteId: site.id }} className="group grid gap-3 py-5 sm:grid-cols-[1fr_1fr_auto] sm:items-center">
        <div><h2 className="font-medium group-hover:text-primary">{site.name}</h2><p className="mt-1 text-sm text-muted">{site.domains[0] ?? 'No domain configured'}</p></div>
        <div className="min-w-0"><p className="truncate text-sm">{site.repository_url}</p><p className="mt-1 font-mono text-xs text-muted">{site.branch} · {site.project_directory}</p></div>
        <span className="text-sm text-muted group-hover:text-ink">Open →</span>
      </Link>)}
    </div>
  </main></AppShell>
}
