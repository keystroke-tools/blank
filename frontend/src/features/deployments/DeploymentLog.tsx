import { useEffect, useRef, useState } from 'react'
import { queryClient, queryKeys } from '../../lib/query-client'
import type { Deployment } from '../../lib/api'

const terminal = new Set(['success', 'failed', 'cancelled'])
type DeploymentEvent = { chunk: string; status: string; done: boolean }

export function DeploymentLog({ deploymentId, siteId, initialLog, initialStatus }: { deploymentId: string; siteId: string; initialLog: string; initialStatus: string }) {
  const active = !terminal.has(initialStatus)
  const [log, setLog] = useState(active ? '' : initialLog)
  const [following, setFollowing] = useState(true)
  const [connected, setConnected] = useState(active)
  const viewer = useRef<HTMLPreElement>(null)

  useEffect(() => {
    if (!active) return
    const source = new EventSource(`/api/deployments/${deploymentId}/events`, { withCredentials: true })
    source.addEventListener('deployment', (raw) => {
      const event = JSON.parse((raw as MessageEvent).data) as DeploymentEvent
      if (event.chunk) setLog((current) => current + event.chunk)
      queryClient.setQueryData<Deployment>(queryKeys.deployment(deploymentId), (current) => current ? { ...current, status: event.status } : current)
      setConnected(true)
      if (event.done) {
        source.close()
        void queryClient.invalidateQueries({ queryKey: queryKeys.deployment(deploymentId) })
        void queryClient.invalidateQueries({ queryKey: queryKeys.deployments(siteId) })
      }
    })
    source.onerror = () => setConnected(false)
    return () => source.close()
  }, [active, deploymentId, siteId])

  useEffect(() => {
    if (following && viewer.current) viewer.current.scrollTop = viewer.current.scrollHeight
  }, [following, log])

  return <section className="mt-8 border border-[#333] bg-[#0d0d0d]">
    <div className="flex items-center justify-between border-b border-[#333] bg-[#151515] px-4 py-3 text-xs text-[#a6a6a6]"><span>{active && !connected ? 'Reconnecting to log stream…' : active ? 'Live output' : 'Historical output'}</span><button type="button" onClick={() => setFollowing((value) => !value)} className="font-semibold text-primary">{following ? 'Pause follow' : 'Follow latest'}</button></div>
    <pre ref={viewer} className="h-[32rem] overflow-auto whitespace-pre-wrap p-5 font-mono text-xs leading-6 text-[#dedede]">{log || 'Waiting for the deployment worker…'}</pre>
  </section>
}
