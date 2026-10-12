import { useEffect, useRef, useState } from 'react';
import { formatChips } from './geometry';

export const predictions = ['Clean Run', 'Comeback', 'Broken Build', 'Needs Help'] as const;
export type AgentPrediction = typeof predictions[number];
export interface AgentSession { sessionId: string; eligible: boolean; reason: string }
export interface AgentWager {
  wagerId: string; sessionId: string; turnId: string | null; prediction: AgentPrediction; stake: number;
  status: 'armed' | 'bound' | 'settled' | 'void'; outcome: AgentPrediction | 'Unknown' | null;
  grossReturn: number; reason: string; armedAt: string;
}
export interface AgentStatus { enabled: boolean; message: string; sessions: AgentSession[]; wagers: AgentWager[]; revision: number }
export interface AgentArmRequest { requestId: string; expectedWalletRevision: number; sessionId: string; prediction: AgentPrediction; stake: number }
export interface AgentPort {
  status(): Promise<AgentStatus>;
  arm(request: AgentArmRequest): Promise<AgentStatus>;
  cancel(requestId: string, wagerId: string): Promise<AgentStatus>;
  void(requestId: string, wagerId: string): Promise<AgentStatus>;
}
export type AgentPendingAction = { kind: 'arm'; request: AgentArmRequest } | { kind: 'cancel' | 'void'; requestId: string; wagerId: string };
const descriptions: Record<AgentPrediction, string> = {
  'Clean Run': 'All required checks finish passing, with no observed failures.',
  'Comeback': 'An observed failure is followed by fresh passes of every required check.',
  'Broken Build': 'A final required test or registered build check fails.',
  'Needs Help': 'User intervention remains unresolved at a verified terminal boundary.',
};

export function AgentOutcome({ wager }: { wager: AgentWager }) {
  const refunded = wager.status === 'void' || wager.outcome === 'Unknown';
  const title = wager.outcome === 'Unknown' ? 'Unknown · stake refunded' : wager.status === 'void' ? 'Prediction voided · stake refunded' : wager.outcome;
  return <div className={`agent-outcome ${refunded ? 'agent-refund' : ''}`} aria-live="polite" aria-atomic="true">
    <svg className="agent-category-wheel" viewBox="0 0 120 120" role="img" aria-label={`Recorded category: ${wager.outcome ?? 'void'}`}>
      {predictions.map((name, index) => {
        const a = index * Math.PI / 2 - Math.PI / 2 + 0.025; const b = a + Math.PI / 2 - 0.05;
        const point = (r: number, angle: number) => `${60 + Math.cos(angle) * r},${60 + Math.sin(angle) * r}`;
        return <g key={name}><path d={`M${point(54, a)} A54,54 0 0,1 ${point(54, b)} L${point(31, b)} A31,31 0 0,0 ${point(31, a)} Z`} fill={!refunded && wager.outcome === name ? '#c3a366' : '#29432c'} stroke="#71845b" strokeWidth=".6"/><text x={60 + Math.cos((a + b) / 2) * 43} y={63 + Math.sin((a + b) / 2) * 43} textAnchor="middle" fill={!refunded && wager.outcome === name ? '#15291b' : '#b9c7a8'} fontSize="8">{['CR', 'CB', 'BB', 'NH'][index]}</text></g>;
      })}<circle cx="60" cy="60" r="25" fill="#182b1b" stroke="#9c8857" strokeWidth=".8"/><text x="60" y="64" textAnchor="middle" fill="#d8c28a" fontSize={refunded ? 22 : 17}>{refunded ? '↶' : '◇'}</text>
    </svg>
    <div><span className="eyebrow">RECORDED AGENT OUTCOME</span><h3>{title}</h3><p>{wager.reason}</p><strong>{refunded ? `${formatChips(wager.grossReturn)} chips returned` : `${formatChips(wager.grossReturn)} gross return · ${wager.grossReturn - wager.stake > 0 ? '+' : ''}${formatChips(wager.grossReturn - wager.stake)} net chips`}</strong></div>
  </div>;
}

export function AgentPanel({ agent, wallet, status: suppliedStatus, onStatus, onWalletRefresh, pending }: { agent: AgentPort; wallet: { balance: number; revision: number }; status: AgentStatus | null; onStatus: (status: AgentStatus) => void; onWalletRefresh: () => void; pending: { current: AgentPendingAction | null } }) {
  const [status, setStatus] = useState<AgentStatus | null>(suppliedStatus);
  const [sessionId, setSessionId] = useState(''); const [prediction, setPrediction] = useState<AgentPrediction>('Clean Run');
  const [stake, setStake] = useState(100); const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const [historyPage, setHistoryPage] = useState(0);
  const [confirmation, setConfirmation] = useState<AgentWager | null>(null);
  const [uncertain, setUncertain] = useState(Boolean(pending.current));
  const retryNotice = 'A previous request is awaiting confirmation. Retry it to safely recover the same result.';
  const live = useRef(true); const latestStatus = useRef<AgentStatus | null>(suppliedStatus);
  const callbacks = useRef({ onStatus, onWalletRefresh }); callbacks.current = { onStatus, onWalletRefresh };
  function accept(next: AgentStatus) {
    if (!live.current || (next.revision !== undefined && latestStatus.current?.revision !== undefined && next.revision < latestStatus.current.revision)) return;
    latestStatus.current = next; setStatus(next); callbacks.current.onStatus(next);
    setSessionId(current => current || next.sessions.find(session => session.eligible)?.sessionId || next.sessions[0]?.sessionId || '');
  }
  useEffect(() => {
    live.current = true;
    const refresh = () => { if (!document.hidden) void agent.status().then(accept).catch(cause => { if (live.current) setError(cause instanceof Error ? cause.message : 'Agent status is unavailable.'); }); };
    refresh(); const timer = window.setInterval(refresh, 2000);
    return () => { live.current = false; window.clearInterval(timer); };
  }, [agent]);
  async function perform(action: AgentPendingAction) {
    if (busy) return;
    pending.current = action; setBusy(true); setError('');
    try {
      const next = action.kind === 'arm' ? await agent.arm(action.request) : action.kind === 'cancel' ? await agent.cancel(action.requestId, action.wagerId) : await agent.void(action.requestId, action.wagerId);
      if (!live.current) return;
      accept(next); pending.current = null; setUncertain(false); setConfirmation(null); callbacks.current.onWalletRefresh();
    } catch (cause) {
      if (!live.current) return;
      const code = typeof cause === 'object' && cause !== null && 'code' in cause ? String(cause.code) : '';
      const rejected = ['INVALID_BET', 'INSUFFICIENT_FUNDS', 'STALE_REVISION', 'ROUND_ACTIVE', 'OVERFLOW', 'INTEGRATION_UNAVAILABLE', 'VERSION_MISMATCH', 'REQUEST_CONFLICT'].includes(code);
      if (rejected) pending.current = null;
      setUncertain(!rejected); setError(`${cause instanceof Error ? cause.message : 'The local agent service could not complete this action.'}${rejected ? '' : ' Retry the same request to safely recover its result.'}`);
      callbacks.current.onWalletRefresh();
    } finally { if (live.current) setBusy(false); }
  }
  const selectedSession = status?.sessions.find(session => session.sessionId === sessionId);
  const outstanding = status?.wagers.filter(wager => wager.status === 'armed' || wager.status === 'bound') ?? [];
  const sessionWager = outstanding.find(wager => wager.sessionId === sessionId);
  const canArm = Boolean(status?.enabled && selectedSession?.eligible && !sessionWager && stake > 0 && stake <= wallet.balance && !busy && !uncertain);
  const resolved = status?.wagers.filter(wager => wager.status === 'settled' || wager.status === 'void') ?? [];
  const pageCount = Math.max(1, Math.ceil(resolved.length / 10));
  const currentPage = Math.min(historyPage, pageCount - 1);
  return <div className="agent-panel">
    <p className="eyebrow">A PREDICTION GAME · FIXED 1:1 PROFIT ODDS</p>
    <p className="modal-intro">Call the next turn.<br/>Let the evidence decide.</p>
    <p>Arm one prediction while a verified session is idle. The next observed turn locks it. Only registered local checks and verified boundaries determine the result.</p>
    <div className={`agent-availability ${status?.enabled ? 'agent-ready' : ''}`} role="status"><span className="status-light"/><span>{status?.message ?? 'Checking the local verification gate…'}</span></div>
    {!status?.enabled ? <p className="integration-note">Wagering stays unavailable until the native verification gate passes. Configure the supported launcher and required suites with the bundled helper, then complete a real lifecycle check.</p> : null}
    <label className="search-label agent-session-select">Verified session<select value={sessionId} disabled={busy || uncertain} onChange={event => setSessionId(event.target.value)}><option value="">Choose a session</option>{status?.sessions.map(session => <option key={session.sessionId} value={session.sessionId}>{session.sessionId.slice(0, 12)} · {session.eligible ? 'Ready for next turn' : 'Unavailable'}</option>)}</select></label>
    <p className="muted">{sessionWager ? 'This session already has an outstanding prediction.' : selectedSession?.reason || 'Only sessions reported by the local verification service appear here.'}</p>
    <div className="prediction-grid" role="group" aria-label="Prediction category">{predictions.map((name, index) => <button key={name} type="button" className={`prediction-card ${prediction === name ? 'selected' : ''}`} aria-pressed={prediction === name} disabled={busy || uncertain} onClick={() => setPrediction(name)}><span className="prediction-index">0{index + 1}</span><strong>{name}</strong><small>{descriptions[name]}</small><span className="prediction-odds">1:1 PROFIT</span></button>)}</div>
    <div className="agent-stake-row"><div><span className="eyebrow">YOUR PREDICTION STAKE</span><div className="agent-stakes" role="group" aria-label="Agent stake">{[10, 50, 100, 500, 1000].map(amount => <button key={amount} aria-pressed={stake === amount} disabled={busy || uncertain} onClick={() => setStake(amount)}>{formatChips(amount)}</button>)}</div></div><div className="agent-return"><span className="eyebrow">WINNING GROSS RETURN</span><strong>{formatChips(stake * 2)}</strong><small>chips, including your stake</small></div></div>
    <div className="agent-arm-row"><span>{formatChips(wallet.balance)} chips available</span><button className="primary-button" disabled={!canArm} onClick={() => void perform({ kind: 'arm', request: { requestId: crypto.randomUUID(), expectedWalletRevision: wallet.revision, sessionId, prediction, stake } })}>{busy ? 'Saving prediction…' : 'Arm for next turn'}</button></div>
    {stake > wallet.balance ? <p className="muted">Choose a smaller stake to fit your available wallet.</p> : null}
    {error || uncertain ? <div className="error-banner" role="alert"><span>{error || retryNotice}</span>{uncertain && pending.current ? <button disabled={busy} onClick={() => { if (pending.current) void perform(pending.current); }}>Retry request</button> : null}</div> : null}
    <p className="agent-policy">No verified result means <strong>Unknown</strong> and a full stake refund. An unbound prediction expires after 15 minutes; a bound prediction is voided after 15 minutes without a valid terminal boundary. You can cancel before binding or explicitly void a pending turn for a refund. These labels describe observed checks, not proof that the coding task is correct.</p>
    {outstanding.length ? <section className="outstanding-wagers"><h3>At the table</h3>{outstanding.map(wager => <div key={wager.wagerId} className="agent-wager"><div><span className="eyebrow">{wager.status === 'armed' ? 'ARMED · WAITING FOR NEXT TURN' : 'LOCKED · TURN IN PROGRESS'}</span><strong>{wager.prediction}</strong><p>{formatChips(wager.stake)} chips · Session {wager.sessionId.slice(0, 12)}</p><small>{wager.reason || (wager.status === 'armed' ? 'You can cancel until the turn binds.' : 'Waiting for a verified terminal boundary.')}</small></div><button disabled={busy || uncertain} onClick={() => setConfirmation(wager)}>{wager.status === 'armed' ? 'Cancel & refund' : 'Void & refund'}</button></div>)}</section> : null}
    {confirmation ? <section className="agent-confirmation" aria-label="Refund confirmation"><h3>{confirmation.status === 'armed' ? 'Cancel this prediction?' : 'Void this pending turn?'}</h3><p>{formatChips(confirmation.stake)} chips will be returned. This is a refund and does not count as a predicted win.</p><div className="confirm-actions"><button disabled={busy || uncertain} onClick={() => setConfirmation(null)}>Keep prediction</button><button className="primary-button" disabled={busy || uncertain} onClick={() => void perform({ kind: confirmation.status === 'armed' ? 'cancel' : 'void', requestId: crypto.randomUUID(), wagerId: confirmation.wagerId })}>Confirm refund</button></div></section> : null}
    {resolved.length ? <section className="resolved-wagers"><h3>Recorded outcomes</h3>{resolved.slice(currentPage * 10, currentPage * 10 + 10).map(wager => <AgentOutcome key={wager.wagerId} wager={wager}/>)}{pageCount > 1 ? <div className="pagination"><button disabled={currentPage === 0} onClick={() => setHistoryPage(currentPage - 1)}>Previous outcomes</button><span>Page {currentPage + 1} of {pageCount}</span><button disabled={currentPage + 1 === pageCount} onClick={() => setHistoryPage(currentPage + 1)}>Next outcomes</button></div> : null}</section> : null}
  </div>;
}
