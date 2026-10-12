import { useEffect, useState } from 'react';

export interface PassiveSession {
  sessionId: string;
  status: 'idle' | 'working' | 'attention' | 'stopped' | 'ended' | 'stale';
  turnId: string | null;
  sequence: number;
  lastEventAtMs: number;
}
export interface IntegrationStatus {
  available: boolean;
  connected: boolean;
  providerVersion: string | null;
  supportedVersion: string;
  autoLaunch: boolean;
  sessions: PassiveSession[];
  message: string;
}
export interface IntegrationPreview { projectPath: string; settingsPath: string; entries: unknown[]; warning: string }
export interface IntegrationPort {
  status(): Promise<IntegrationStatus>;
  preview(projectPath: string): Promise<IntegrationPreview>;
  install(projectPath: string): Promise<IntegrationStatus>;
  disconnect(projectPath: string): Promise<IntegrationStatus>;
  check(): Promise<IntegrationStatus>;
}
const labels: Record<PassiveSession['status'], string> = { idle: 'Ready for a turn', working: 'Agent is working', attention: 'Your attention is needed', stopped: 'Agent stopped responding', ended: 'Session ended', stale: 'Status is out of date' };
export function SessionBadge({ session }: { session: PassiveSession }) {
  return <span className={`session-badge session-${session.status}`}><span className="status-light"/><span>{labels[session.status]}</span><small>SESSION {session.sessionId.slice(0, 8)}</small></span>;
}
export function IntegrationPanel({ integration, onStatus }: { integration: IntegrationPort; onStatus: (status: IntegrationStatus) => void }) {
  const [status, setStatus] = useState<IntegrationStatus | null>(null);
  const [path, setPath] = useState(''); const [preview, setPreview] = useState<IntegrationPreview | null>(null);
  const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const [disconnectConfirm, setDisconnectConfirm] = useState(false); const [checking, setChecking] = useState(false);
  useEffect(() => {
    let live = true; const update = () => { void integration.status().then(next => { if (live) { setStatus(next); onStatus(next); } }).catch(cause => { if (live) setError(cause instanceof Error ? cause.message : 'Could not read local integration status.'); }); };
    update(); const timer = window.setInterval(update, 2000); return () => { live = false; window.clearInterval(timer); };
  }, [integration]);
  async function run(action: () => Promise<void>) { setError(''); setBusy(true); try { await action(); } catch (cause) { setError(cause instanceof Error ? cause.message : 'The integration action could not complete.'); } finally { setBusy(false); } }
  function accept(next: IntegrationStatus) { setStatus(next); onStatus(next); }
  return <div className="integration-panel">
    <p className="modal-intro">Let your agent work.<br/>Keep a seat at the table.</p>
    <p>Optional Claude Code hooks can show turn activity and attention notices while you play. They do not change prompts, approve permissions, or control the agent.</p>
    <div className="connection-status"><span className={`status-light ${status?.connected ? '' : 'status-off'}`}/><div><strong>{status?.connected ? 'Local bridge connected' : status?.available ? 'Ready to connect' : status ? 'Integration unavailable' : 'Checking local availability'}</strong><small>{status?.message ?? 'Reading the local helper status…'}</small></div></div>
    {status ? <p className="muted">Claude Code {status.providerVersion ?? 'not detected'} · Supported: {status.supportedVersion}{status.autoLaunch ? ' · Auto-open enabled' : ''}</p> : null}
    <label className="search-label">Project folder<input type="text" value={path} placeholder="C:\\Users\\you\\your-project" autoComplete="off" spellCheck={false} onChange={event => { setPath(event.target.value); setPreview(null); setDisconnectConfirm(false); }}/></label>
    <p className="muted">Choose the project where you use Claude Code. Setup updates only its <code>.claude/settings.local.json</code> after you review the entries below.</p>
    <div className="integration-actions"><button disabled={busy || !path.trim() || status?.available === false} onClick={() => void run(async () => { setPreview(await integration.preview(path.trim())); setDisconnectConfirm(false); })}>Preview connection</button><button disabled={busy || !status?.available} onClick={() => void run(async () => { accept(await integration.check()); setChecking(true); })}>Check local bridge</button></div>
    {checking ? <p className="integration-note" role="status">The bridge check uses a synthetic local event. Confirm the full connection with a real Claude Code session after installation; existing sessions may need a restart.</p> : null}
    {preview ? <section className="hook-preview"><h3>Review the hook entries</h3><p>These entries will be added to <code>{preview.settingsPath}</code>.</p>{preview.warning ? <p className="integration-note">{preview.warning}</p> : null}<pre tabIndex={0} aria-label="Exact hook entries to install">{JSON.stringify(preview.entries, null, 2)}</pre><p>Existing settings and unrelated hooks are preserved. A backup is created before changes. No prompt text or source files are read for gameplay.</p><button className="primary-button" disabled={busy} onClick={() => void run(async () => { accept(await integration.install(preview.projectPath)); setPreview(null); })}>{busy ? 'Connecting…' : 'Install these hooks'}</button></section> : null}
    {status?.sessions.length ? <section><h3>Observed sessions</h3><div className="session-list">{status.sessions.map(session => <SessionBadge key={session.sessionId} session={session}/>)}</div><p className="muted">A stopped session is a lifecycle observation, not proof that a coding task or its tests succeeded.</p></section> : null}
    <section className="settings-section"><h3>Disconnect this project</h3><p>Remove only Commit Casino’s installed entries. If an entry was edited afterward, it is preserved for review.</p>{disconnectConfirm ? <div className="confirm-actions"><button onClick={() => setDisconnectConfirm(false)}>Keep connection</button><button disabled={busy || !path.trim()} onClick={() => void run(async () => { accept(await integration.disconnect(path.trim())); setDisconnectConfirm(false); setPreview(null); })}>Disconnect project</button></div> : <button disabled={busy || !path.trim()} onClick={() => setDisconnectConfirm(true)}>Review disconnect</button>}</section>
    <p className="integration-note">Agent Roulette is not available yet. Classic Roulette uses its own secure randomness and works with or without an agent connection.</p>
    {error ? <div className="error-banner" role="alert">{error}</div> : null}
  </div>;
}

