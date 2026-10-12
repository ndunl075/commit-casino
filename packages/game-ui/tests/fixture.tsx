import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { CasinoApp, type AgentPort, type AgentStatus, type AgentArmRequest } from '../src/index';
import { CasinoAudio } from '../src/audio';
import { defaultSettings, emptyStats, type CasinoService, type CasinoSnapshot, type CommitClassicRequest, type CommittedRound } from '@commit-casino/casino-contracts';

const initial = (balance = 10000): CasinoSnapshot => ({ revision: 0, wallet: { balance, revision: 0, epoch: 0 }, activeRound: null, history: [], settings: { ...defaultSettings, animationMode: 'reduced-motion' }, stats: emptyStats(), achievements: [], features: { agentIntegration: false, agentRoulette: false }, sessions: [] });
const round = (requestId: string = crypto.randomUUID()): CommittedRound => ({ roundId: crypto.randomUUID(), requestId, rulesVersion: 'european-v1', bets: [{ betId: 'straight:7', stake: 500 }], outcome: 7, stake: 500, grossReturn: 18000, netChange: 17500, status: 'settled', presentation: { roundId: '', outcome: 7, pocketIndex: 29, animationSeed: 33, durationMs: 180, committedAt: new Date().toISOString(), presentationState: 'pending' } });
const sounds: string[] = [];
CasinoAudio.prototype.play = function (kind) { sounds.push(kind); };
class FakeService implements CasinoService {
  state = initial(); listeners = new Set<(snapshot: CasinoSnapshot) => void>();
  failSettings = false; failReset = false; deferred = false; lostResponse = false;
  requests: string[] = []; storedRound: CommittedRound | null = null;
  resolveCommit: ((round: CommittedRound) => void) | null = null;
  getSnapshot = async () => structuredClone(this.state);
  publish() { for (const listener of this.listeners) listener(structuredClone(this.state)); }
  subscribe = (listener: (snapshot: CasinoSnapshot) => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  async commitClassic(request: CommitClassicRequest) {
    this.requests.push(request.requestId);
    if (this.deferred) return new Promise<CommittedRound>(resolve => { this.resolveCommit = resolve; });
    if (this.storedRound) return structuredClone(this.storedRound);
    this.storedRound = round(request.requestId); this.storedRound.presentation.roundId = this.storedRound.roundId;
    this.state = { ...this.state, revision: 2, wallet: { balance: 27500, revision: 2, epoch: 0 }, history: [this.storedRound], stats: { ...emptyStats(), spins: 1, betFrequencies: { 'straight:7': 1 } } }; this.publish();
    if (this.lostResponse) throw new Error('Response lost after commit');
    return structuredClone(this.storedRound);
  }
  async acknowledgePresentation() { if (this.state.history[0]) this.state.history[0].presentation.presentationState = 'shown'; this.state.revision++; this.publish(); }
  async resetWallet() { if (this.failReset) throw new Error('Reset write denied'); this.state = initial(); this.publish(); return this.getSnapshot(); }
  async refillWallet() { return this.getSnapshot(); }
  async clearHistory() { return this.getSnapshot(); }
  async updateSettings(patch: Parameters<CasinoService['updateSettings']>[0]) { if (this.failSettings) throw { code: 'PERSISTENCE', message: 'Settings write denied' }; this.state.settings = { ...this.state.settings, ...patch }; this.state.revision++; this.publish(); return this.getSnapshot(); }
}
const oldService = new FakeService(); const nextService = new FakeService(); nextService.state = initial(4321);
const scenario = new URLSearchParams(location.search).get('scenario');
if (scenario === 'already-shown') {
  const shown = round(); shown.status = 'committed'; shown.presentation.presentationState = 'shown'; shown.presentation.durationMs = 5000;
  oldService.state.activeRound = shown; oldService.state.settings.animationMode = 'normal';
}
oldService.deferred = scenario === 'swap'; oldService.lostResponse = scenario === 'retry'; oldService.failSettings = scenario === 'errors'; oldService.failReset = scenario === 'errors';
class FakeAgent implements AgentPort {
  state: AgentStatus = { revision: 0, enabled: scenario !== 'agent-disabled', message: scenario === 'agent-disabled' ? 'Verification gate has not passed.' : 'Verified session ready.', sessions: [{ sessionId: 'verified-session-1', eligible: true, reason: 'Idle with continuous verified boundaries.' }], wagers: [] };
  requests: string[] = []; private commands = new Map<string, AgentStatus>();
  status = async () => structuredClone(this.state);
  async arm(request: AgentArmRequest) {
    this.requests.push(request.requestId); const prior = this.commands.get(request.requestId); if (prior) return structuredClone(prior);
    this.state.wagers.unshift({ wagerId: 'test-wager', sessionId: request.sessionId, turnId: null, prediction: request.prediction, stake: request.stake, status: 'armed', outcome: null, grossReturn: 0, reason: 'Waiting for the next verified turn.', armedAt: new Date().toISOString() });
    this.state.revision = (this.state.revision ?? 0) + 1; oldService.state.wallet.balance -= request.stake; oldService.state.wallet.revision++; oldService.state.revision++; oldService.publish();
    this.commands.set(request.requestId, structuredClone(this.state));
    if (scenario === 'agent-retry') throw new Error('Agent response lost after debit');
    return this.status();
  }
  async cancel(requestId: string) { return this.refund(requestId); }
  async void(requestId: string) { return this.refund(requestId); }
  private async refund(requestId: string) {
    const prior = this.commands.get(requestId); if (prior) return structuredClone(prior);
    const wager = this.state.wagers[0]!; wager.status = 'void'; wager.outcome = 'Unknown'; wager.grossReturn = wager.stake; wager.reason = 'Explicit user refund.';
    this.state.revision = (this.state.revision ?? 0) + 1; oldService.state.wallet.balance += wager.stake; oldService.state.wallet.revision++; oldService.state.revision++; oldService.publish();
    this.commands.set(requestId, structuredClone(this.state)); return this.status();
  }
}
const agent = new FakeAgent();
declare global { interface Window { regression: { swap: () => void; resolveOld: () => void; observer: () => void; settleObserver: () => void; bindAgent: () => void; sounds: string[]; requests: string[]; agentRequests: string[] } } }
function Fixture() {
  const [service, setService] = useState<CasinoService>(oldService);
  window.regression = { sounds, requests: oldService.requests, agentRequests: agent.requests, swap: () => setService(nextService), resolveOld: () => { oldService.state.revision = 999; oldService.state.wallet.balance = 999999; oldService.resolveCommit?.(round()); oldService.publish(); }, observer: () => { const observed = round(); observed.status = 'committed'; observed.presentation.durationMs = 5000; observed.presentation.roundId = observed.roundId; oldService.state.settings.animationMode = 'normal'; oldService.state.activeRound = observed; oldService.state.revision++; oldService.publish(); }, settleObserver: () => { oldService.state.activeRound = null; oldService.state.revision++; oldService.publish(); }, bindAgent: () => { agent.state.wagers[0]!.status = 'bound'; agent.state.wagers[0]!.turnId = 'verified-turn'; agent.state.revision = (agent.state.revision ?? 0) + 1; } };
  return <CasinoApp service={service} {...(scenario?.startsWith('agent-') ? { agent } : {})}/>;
}
createRoot(document.getElementById('root')!).render(<Fixture/>);
