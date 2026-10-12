import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { CasinoApp } from '../src/index';
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
    this.state = { ...this.state, revision: 2, wallet: { balance: 27500, revision: 2, epoch: 0 }, history: [this.storedRound], stats: { ...emptyStats(), spins: 1 } }; this.publish();
    if (this.lostResponse) throw new Error('Response lost after commit');
    return structuredClone(this.storedRound);
  }
  async acknowledgePresentation() { if (this.state.history[0]) this.state.history[0].presentation.presentationState = 'shown'; this.state.revision++; this.publish(); }
  async resetWallet() { if (this.failReset) throw new Error('Reset write denied'); this.state = initial(); this.publish(); return this.getSnapshot(); }
  async refillWallet() { return this.getSnapshot(); }
  async clearHistory() { return this.getSnapshot(); }
  async updateSettings(patch: Parameters<CasinoService['updateSettings']>[0]) { if (this.failSettings) throw new Error('Settings write denied'); this.state.settings = { ...this.state.settings, ...patch }; this.state.revision++; this.publish(); return this.getSnapshot(); }
}
const oldService = new FakeService(); const nextService = new FakeService(); nextService.state = initial(4321);
const scenario = new URLSearchParams(location.search).get('scenario');
oldService.deferred = scenario === 'swap'; oldService.lostResponse = scenario === 'retry'; oldService.failSettings = scenario === 'errors'; oldService.failReset = scenario === 'errors';
declare global { interface Window { regression: { swap: () => void; resolveOld: () => void; observer: () => void; settleObserver: () => void; sounds: string[]; requests: string[] } } }
function Fixture() {
  const [service, setService] = useState<CasinoService>(oldService);
  window.regression = { sounds, requests: oldService.requests, swap: () => setService(nextService), resolveOld: () => { oldService.state.revision = 999; oldService.state.wallet.balance = 999999; oldService.resolveCommit?.(round()); oldService.publish(); }, observer: () => { const observed = round(); observed.status = 'committed'; observed.presentation.durationMs = 5000; observed.presentation.roundId = observed.roundId; oldService.state.settings.animationMode = 'normal'; oldService.state.activeRound = observed; oldService.state.revision++; oldService.publish(); }, settleObserver: () => { oldService.state.activeRound = null; oldService.state.revision++; oldService.publish(); } };
  return <CasinoApp service={service}/>;
}
createRoot(document.getElementById('root')!).render(<Fixture/>);
