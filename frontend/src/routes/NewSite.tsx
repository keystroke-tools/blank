import { Link } from '@tanstack/react-router'
import { AppShell } from '../components/AppShell'
import { SiteForm } from '../features/sites/SiteForm'

export function NewSite() { return <AppShell><main className="mx-auto max-w-4xl px-6 py-12"><Link to="/" className="text-sm text-muted hover:text-ink">← Sites</Link><p className="mt-10 text-xs font-semibold uppercase tracking-[0.18em] text-primary">New site</p><h1 className="mt-2 text-3xl font-semibold tracking-tight">Connect a frontend</h1><SiteForm /></main></AppShell> }
