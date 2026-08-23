import { useQuery } from '@tanstack/react-query'
import { Navigate, Outlet, createRootRoute, createRoute, createRouter } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { AuthForm } from './features/auth/AuthForm'
import { api } from './lib/api'
import { queryKeys } from './lib/query-client'
import { Dashboard } from './routes/Dashboard'
import { NewSite } from './routes/NewSite'
import { DeploymentPage, SitePage, SiteSettings } from './routes/SitePage'
import { SiteConfiguration } from './routes/SiteConfiguration'

function Root() {
  const auth = useQuery({ queryKey: queryKeys.auth, queryFn: api.authStatus })
  if (auth.isPending) return <div className="grid min-h-screen place-items-center text-sm text-muted">Starting Blank…</div>
  if (auth.isError) return <div className="grid min-h-screen place-items-center px-6 text-center text-sm text-danger">Blank could not load: {auth.error.message}</div>
  return <Outlet />
}

function RequireAuth({ children }: { children: ReactNode }) {
  const { data } = useQuery({ queryKey: queryKeys.auth, queryFn: api.authStatus })
  if (data?.setup_required) return <Navigate to="/setup" />
  if (!data?.authenticated) return <Navigate to="/login" />
  return children
}

const rootRoute = createRootRoute({ component: Root })
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: () => {
  const { data } = useQuery({ queryKey: queryKeys.auth, queryFn: api.authStatus })
  if (data?.setup_required) return <Navigate to="/setup" />
  if (!data?.authenticated) return <Navigate to="/login" />
  return <Dashboard />
} })
const setupRoute = createRoute({ getParentRoute: () => rootRoute, path: '/setup', component: () => {
  const { data } = useQuery({ queryKey: queryKeys.auth, queryFn: api.authStatus })
  return data?.setup_required ? <AuthForm mode="setup" /> : <Navigate to="/" />
} })
const loginRoute = createRoute({ getParentRoute: () => rootRoute, path: '/login', component: () => {
  const { data } = useQuery({ queryKey: queryKeys.auth, queryFn: api.authStatus })
  return data?.authenticated || data?.setup_required ? <Navigate to="/" /> : <AuthForm mode="login" />
} })
const newSiteRoute = createRoute({ getParentRoute: () => rootRoute, path: '/sites/new', component: () => <RequireAuth><NewSite /></RequireAuth> })
const siteRoute = createRoute({ getParentRoute: () => rootRoute, path: '/sites/$siteId', component: () => { const { siteId } = siteRoute.useParams(); return <RequireAuth><SitePage siteId={siteId} /></RequireAuth> } })
const siteSettingsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/sites/$siteId/settings', component: () => { const { siteId } = siteSettingsRoute.useParams(); return <RequireAuth><SiteSettings siteId={siteId} /></RequireAuth> } })
const siteConfigurationRoute = createRoute({ getParentRoute: () => rootRoute, path: '/sites/$siteId/configuration', component: () => { const { siteId } = siteConfigurationRoute.useParams(); return <RequireAuth><SiteConfiguration siteId={siteId} /></RequireAuth> } })
const deploymentRoute = createRoute({ getParentRoute: () => rootRoute, path: '/sites/$siteId/deployments/$deploymentId', component: () => { const { siteId, deploymentId } = deploymentRoute.useParams(); return <RequireAuth><DeploymentPage siteId={siteId} deploymentId={deploymentId} /></RequireAuth> } })

export const router = createRouter({ routeTree: rootRoute.addChildren([indexRoute, setupRoute, loginRoute, newSiteRoute, siteRoute, siteSettingsRoute, siteConfigurationRoute, deploymentRoute]), defaultPreload: 'intent', scrollRestoration: true })

declare module '@tanstack/react-router' { interface Register { router: typeof router } }
