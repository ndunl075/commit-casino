import { useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import type { BetInput, CasinoService, CasinoSnapshot, CommittedRound } from '@commit-casino/casino-contracts';
import { catalog, maxGrossReturn } from '@commit-casino/roulette-engine';
import { CasinoAudio } from './audio';
import { createDraftStore } from './draft';
import { betName, colorOf, formatChips, shortChips, type TableBet } from './geometry';
import { Modal } from './Modal';
import { RouletteTable } from './Table';
import { Wheel } from './Wheel';
import './styles.css';

export interface CasinoAppProps {
  service: CasinoService;
  temporary?: boolean;
  onTemporaryPlay?: () => void;
  onWindowMode?: (mode: 'compact' | 'standard' | 'fullscreen') => void | Promise<void>;
}
type Panel = 'settings' | 'history' | 'stats' | 'rules' | 'picker' | 'reset' | 'clear-history' | 'rebet' | null;
type CommitRequest = Parameters<CasinoService['commitClassic']>[0];
const denominations = [10, 50, 100, 500, 1000];
const signed = (n: number) => `${n > 0 ? '+' : n < 0 ? '−' : ''}${formatChips(Math.abs(n))}`;
const errorMessage = (error: unknown) => error instanceof Error ? error.message : 'The local service could not complete this action. Please try again.';

export function CasinoApp({ service, temporary = false, onTemporaryPlay, onWindowMode }: CasinoAppProps) {
  const [draftStore] = useState(createDraftStore); const draft = useStore(draftStore);
  const [audio] = useState(() => new CasinoAudio());
  const [snapshot, setSnapshot] = useState<CasinoSnapshot | null>(null);
  const snapshotRef = useRef<CasinoSnapshot | null>(null);
  const [loadError, setLoadError] = useState('');
  const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const [panel, setPanel] = useState<Panel>(null); const [preview, setPreview] = useState<TableBet | null>(null);
  const [presentation, setPresentation] = useState<CommittedRound | null>(null);
  const [result, setResult] = useState<CommittedRound | null>(null);
  const completed = useRef(new Set<string>()); const pending = useRef<CommitRequest | null>(null);
  const [uncertain, setUncertain] = useState(false); const [query, setQuery] = useState('');
  const [historyPage, setHistoryPage] = useState(0); const [expanded, setExpanded] = useState(false);
  const [systemReducedMotion, setSystemReducedMotion] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches);

  function accept(next: CasinoSnapshot) {
    if (!snapshotRef.current || next.revision >= snapshotRef.current.revision) { snapshotRef.current = next; setSnapshot(next); }
  }
  async function refresh() { try { accept(await service.getSnapshot()); setLoadError(''); } catch (cause) { setLoadError(errorMessage(cause)); } }
  useEffect(() => {
    let live = true; snapshotRef.current = null; setSnapshot(null); completed.current.clear(); pending.current = null; setUncertain(false); setPresentation(null); setResult(null); draftStore.getState().fresh();
    const receive = (next: CasinoSnapshot) => { if (!live) return; const previous = snapshotRef.current; accept(next); if (previous && next.revision > previous.revision + 1) void service.getSnapshot().then(value => { if (live) accept(value); }).catch(() => undefined); };
    const unsubscribe = service.subscribe(receive);
    void service.getSnapshot().then(value => { if (live) { receive(value); setLoadError(''); } }).catch(cause => { if (live) setLoadError(errorMessage(cause)); });
    const visibility = () => { if (!document.hidden) void service.getSnapshot().then(receive).catch(cause => { if (live) setError(errorMessage(cause)); }); };
    document.addEventListener('visibilitychange', visibility);
    return () => { live = false; unsubscribe(); document.removeEventListener('visibilitychange', visibility); };
  }, [service, draftStore]);
  useEffect(() => { const media = window.matchMedia('(prefers-reduced-motion: reduce)'); const change = () => setSystemReducedMotion(media.matches); media.addEventListener('change', change); return () => media.removeEventListener('change', change); }, []);
  useEffect(() => () => audio.destroy(), [audio]);
  useEffect(() => { if (snapshot) audio.configure(snapshot.settings.mute, snapshot.settings.volume); }, [audio, snapshot?.settings.mute, snapshot?.settings.volume]);
  useEffect(() => {
    if (!snapshot) return;
    const unshown = snapshot.activeRound ?? snapshot.history.find(r => r.presentation.presentationState === 'pending');
    if (unshown && !completed.current.has(unshown.roundId)) setPresentation(current => current?.roundId === unshown.roundId ? current : unshown);
    if (result) { const updated = snapshot.history.find(r => r.roundId === result.roundId); if (updated && updated.status !== result.status) setResult(updated); }
  }, [snapshot, result]);

  const total = draft.bets.reduce((sum, b) => sum + b.stake, 0);
  const locked = busy || uncertain || presentation !== null || snapshot?.activeRound?.status === 'committed';
  const lastRound = snapshot?.history[0];
  let potential = 0; try { potential = draft.bets.length ? maxGrossReturn(draft.bets) : 0; } catch { /* Authority rejects unsafe arithmetic. */ }
  const reduced = systemReducedMotion || snapshot?.settings.animationMode === 'reduced-motion';
  const shownBets = draft.bets.length ? draft.bets : presentation?.bets ?? result?.bets ?? [];

  function place(id: string) {
    if (locked || !snapshot) return;
    audio.unlock();
    if (!Number.isSafeInteger(total + draft.chip) || total + draft.chip > snapshot.wallet.balance) { setError('Not enough chips for that placement. Choose a smaller chip or undo a bet.'); return; }
    setResult(null); setError(''); draft.place(id); audio.play('chip');
  }
  function replaceDraft(bets: readonly BetInput[]) {
    if (locked || !snapshot) return;
    const nextTotal = bets.reduce((sum, bet) => sum + bet.stake, 0);
    if (!Number.isSafeInteger(nextTotal) || bets.some(b => !Number.isSafeInteger(b.stake)) || nextTotal > snapshot.wallet.balance) { setError('That complete bet exceeds your available chips. Your draft has not changed.'); return; }
    setResult(null); setError(''); draft.replace(bets); audio.unlock(); audio.play('chip');
  }
  async function spin() {
    if (!snapshot || busy || presentation || snapshot.activeRound?.status === 'committed') return;
    if (!pending.current && (!draft.bets.length || total > snapshot.wallet.balance)) return;
    audio.unlock(); setBusy(true); setError('');
    pending.current ??= { requestId: crypto.randomUUID(), expectedWalletRevision: snapshot.wallet.revision, rulesVersion: 'european-v1', bets: draft.bets.map(b => ({ ...b })), animationMode: reduced ? 'reduced-motion' : snapshot.settings.animationMode };
    try {
      const round = await service.commitClassic(pending.current);
      pending.current = null; setUncertain(false); draft.fresh(); setResult(null);
      if (!completed.current.has(round.roundId)) setPresentation(round);
      audio.play('spin'); await refresh();
    } catch (cause) {
      const code = typeof cause === 'object' && cause !== null && 'code' in cause ? String(cause.code) : '';
      const rejected = ['INSUFFICIENT_FUNDS', 'STALE_REVISION', 'ROUND_ACTIVE', 'INVALID_BET', 'OVERFLOW', 'RNG_UNAVAILABLE', 'VERSION_MISMATCH'].includes(code);
      if (rejected) pending.current = null;
      setUncertain(!rejected); setError(`${errorMessage(cause)}${!rejected ? ' Retry safely to recover the same spin request.' : ''}`);
      await refresh();
    } finally { setBusy(false); }
  }
  function finish(round: CommittedRound) {
    if (completed.current.has(round.roundId)) return;
    completed.current.add(round.roundId); setResult(round); setPresentation(null);
    const recent = Date.now() - Date.parse(round.presentation.committedAt) < round.presentation.durationMs + 1500;
    if (recent) audio.play(round.netChange > 0 ? 'win' : 'result');
    void service.acknowledgePresentation(round.roundId).then(() => service.getSnapshot()).then(accept).catch(cause => setError(errorMessage(cause)));
  }
  async function action(run: () => Promise<CasinoSnapshot>) { setBusy(true); setError(''); try { accept(await run()); setPanel(null); } catch (cause) { setError(errorMessage(cause)); } finally { setBusy(false); } }
  async function settings(patch: Parameters<CasinoService['updateSettings']>[0]) { try { accept(await service.updateSettings(patch)); } catch (cause) { setError(errorMessage(cause)); } }

  if (!snapshot) return <main className="casino-shell loading-screen"><div className="brand-symbol">C<span>•</span>C</div><p className="eyebrow">COMMIT CASINO</p><h1>{loadError ? 'Your saved game is unavailable.' : 'Setting your table…'}</h1>{loadError ? <><p>{loadError}</p><p>Your stored wallet has not been reset.</p><button className="primary-button" onClick={() => void refresh()}>Try again</button>{onTemporaryPlay ? <button className="text-button" onClick={onTemporaryPlay}>Play temporarily without saving</button> : null}</> : <p>Opening your local chip wallet.</p>}</main>;

  return <main className={`casino-shell ${expanded ? 'expanded-table' : ''}`}>
    <header className="topbar"><a className="brand" href="#" aria-label="Commit Casino home" onClick={e => e.preventDefault()}><span className="brand-symbol">C<span>•</span>C</span><span>COMMIT<span className="brand-light"> CASINO</span></span></a>
      <div className="local-status"><span className="status-light" />{temporary ? 'TEMPORARY PLAY' : 'LOCAL PLAY'}</div>
      <nav aria-label="Game navigation"><button onClick={() => setPanel('history')}>History</button><button onClick={() => setPanel('stats')}>Statistics</button><button className="icon-button settings-button" onClick={() => setPanel('settings')} aria-label="Settings">⚙</button></nav>
    </header>
    <div className="room-heading"><div><p className="eyebrow">A LITTLE LUCK BETWEEN COMMITS</p><h1>European Roulette<span className="room-dot">.</span></h1></div><button className="rules-link" onClick={() => setPanel('rules')}>How to play <span aria-hidden="true">↗</span></button></div>
    {temporary ? <div className="notice">Temporary play — your chips and history disappear when this session closes.</div> : null}
    <section className="game-floor" aria-label="Classic Roulette">
      <aside className="wheel-panel"><div className="wheel-caption"><span>THE WHEEL</span><span>SINGLE ZERO · 37 POCKETS</span></div>
        <Wheel round={presentation ?? result} reducedMotion={reduced} onComplete={finish}/>
        <div className={`result-panel ${result && result.netChange > 0 ? 'positive-result' : ''}`} aria-live="polite" aria-atomic="true">
          {presentation ? <><span className="eyebrow">NO MORE BETS</span><h2>The wheel is turning</h2><p>Your bets are locked. Good luck.</p></> : result ? <><span className={`result-number pocket-${colorOf(result.outcome)}`}>{result.outcome}</span><div><span className="eyebrow">{colorOf(result.outcome).toUpperCase()} · {result.netChange > 0 ? 'NICE WIN' : result.netChange === 0 ? 'EVEN ROUND' : 'NEXT ROUND AWAITS'}</span><h2>{signed(result.netChange)} <small>net chips</small></h2><p>Gross return {formatChips(result.grossReturn)} · Stake {formatChips(result.stake)}</p></div></> : <><span className="eyebrow">YOUR TABLE IS READY</span><h2>Make your own luck.</h2><p>Select a chip. Pick your numbers.<br/>Enjoy a little time off the clock.</p></>}
        </div>
        <div className="wheel-footnote"><span className="tiny-diamond">◆</span> EVERY SPIN IS A FRESH START</div>
      </aside>
      <div className="table-panel"><div className="table-heading"><div><span className="eyebrow">PLACE YOUR BETS</span><p>{locked ? 'Table locked while this round completes' : 'Numbers, lines, corners. Follow your instinct.'}</p></div><div className="table-tools"><button onClick={() => setExpanded(!expanded)} aria-label={expanded ? 'Collapse table' : 'Expand table'}>{expanded ? '↙' : '↗'}</button><button onClick={() => setPanel('picker')} disabled={locked}>Bet picker</button></div></div>
        <RouletteTable bets={shownBets} locked={Boolean(locked)} {...(result && !draft.bets.length ? { outcome: result.outcome } : {})} onPlace={place} onHover={setPreview}/>
        <div className="bet-preview" aria-live="polite">{preview ? <><strong>{betName(preview)}</strong><span>{preview.profitOdds}:1 profit · {formatChips(draft.chip * (preview.profitOdds + 1))} gross return per {formatChips(draft.chip)} chip</span></> : <><span className="preview-mark">＋</span><span>{result && !draft.bets.length ? 'Last round shown on the felt. Place a chip to start a fresh draft.' : 'Hover or focus a position to see its coverage and payout.'}</span></>}</div>
        <div className="chip-rack"><span className="eyebrow">CHOOSE YOUR CHIP</span><div className="chips" role="group" aria-label="Chip denomination">{denominations.map(value => <button key={value} className={`chip chip-${value} ${draft.chip === value ? 'selected' : ''}`} aria-pressed={draft.chip === value} aria-label={`${formatChips(value)} chip`} disabled={Boolean(locked)} onClick={() => { draft.setChip(value); audio.unlock(); audio.play('chip'); }}><span>{shortChips(value)}</span></button>)}</div><div className="draft-actions"><button disabled={Boolean(locked) || !draft.undoStack.length} onClick={draft.undo}>↶ Undo</button><button disabled={Boolean(locked) || !draft.bets.length} onClick={() => { draft.clear(); setResult(null); }}>Clear</button></div></div>
        <div className="wager-bar"><div><span className="eyebrow">TOTAL BET</span><strong data-testid="total-bet">{formatChips(total)}<small> chips</small></strong></div><div className="return-preview"><span className="eyebrow">MAX. GROSS RETURN</span><strong>{formatChips(potential)}</strong></div><div className="bet-actions"><button disabled={Boolean(locked) || !lastRound} onClick={() => draft.bets.length ? setPanel('rebet') : lastRound && replaceDraft(lastRound.bets)}>Rebet</button><button disabled={Boolean(locked) || !draft.bets.length} onClick={() => replaceDraft(draft.bets.map(b => ({ ...b, stake: b.stake * 2 })))}>2×</button></div><button className="spin-button" disabled={busy || Boolean(presentation) || snapshot.activeRound?.status === 'committed' || (!uncertain && (!draft.bets.length || total > snapshot.wallet.balance))} onClick={() => void spin()}>{busy ? 'Committing…' : presentation || snapshot.activeRound?.status === 'committed' ? 'Spinning…' : uncertain ? 'Retry spin' : 'SPIN'}<span aria-hidden="true">↗</span></button></div>
      </div>
    </section>
    <section className="bottom-bar" aria-label="Wallet and recent results"><div className="wallet"><span className="wallet-icon" aria-hidden="true">◈</span><div><span className="eyebrow">YOUR CHIP WALLET</span><strong data-testid="wallet-balance">{formatChips(snapshot.wallet.balance)}</strong></div>{snapshot.wallet.balance === 0 ? <button className="refill-button" disabled={Boolean(locked)} onClick={() => void action(() => service.refillWallet(crypto.randomUUID()))}>Free refill +10,000</button> : <span className="wallet-unit">FICTIONAL CHIPS</span>}</div><div className="recent"><span className="eyebrow">RECENT<br/>RESULTS</span><div className="recent-numbers">{snapshot.history.length ? snapshot.history.slice(0, 20).map((round, i) => <span key={round.roundId} className={`recent-pocket pocket-${colorOf(round.outcome)} ${i === 0 ? 'latest-pocket' : ''}`} title={`Result ${round.outcome}; net ${signed(round.netChange)} chips`}>{round.outcome}</span>) : <span className="empty-history">Your first spin starts the story.</span>}</div></div></section>
    {error ? <div className="error-banner" role="alert"><span>{error}</span><button aria-label="Dismiss message" onClick={() => setError('')}>×</button></div> : null}
    <footer className="footer"><span>ALL THE GAME. NONE OF THE STAKES.</span><span>Free fictional chips · No purchases · No cash value</span><span className="agent-status">{snapshot.features.agentIntegration ? 'Agent integration available in desktop setup' : 'Classic mode · Agent integration unavailable'}</span></footer>

    {panel ? <Modal title={{ settings: 'Make yourself comfortable', history: 'Round history', stats: 'Your time at the table', rules: 'The game, at a glance', picker: 'Find your bet', reset: 'Start a fresh wallet?', 'clear-history': 'Clear visible history?', rebet: 'Replace your draft?' }[panel]} wide={panel === 'picker' || panel === 'history'} onClose={() => setPanel(null)}>
      {panel === 'picker' ? <><p>Every table position, available by keyboard. Choose a bet to place one {formatChips(draft.chip)} chip.</p><label className="search-label">Search by number or bet type<input autoFocus type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Try 7, split, red, or dozen"/></label><div className="picker-list">{catalog.bets.filter(b => `${betName(b)} ${b.id} ${b.numbers.join(' ')}`.toLowerCase().includes(query.toLowerCase())).map(b => <button key={b.id} disabled={Boolean(locked)} onClick={() => place(b.id)}><span><strong>{betName(b)}</strong><small>{b.numbers.join(' · ')}</small></span><span>{b.profitOdds}:1 <b>＋{shortChips(draft.chip)}</b></span></button>)}</div><p className="picker-total">Current draft: {formatChips(total)} chips <button className="primary-button" onClick={() => setPanel(null)}>Back to table</button></p></> : null}
      {panel === 'rules' ? <><p className="modal-intro">A classic European table.<br/>A little room to take a chance.</p><ol className="how-to"><li><strong>Choose a chip</strong><span>Pick 10, 50, 100, 500, or 1,000. Every click adds one chip.</span></li><li><strong>Place your bets</strong><span>Bet a number, the edge between numbers, a corner, or an outside area. The small 3 and 6 rail positions cover one or two streets.</span></li><li><strong>Give it a spin</strong><span>The spin locks your bets. A winning bet returns its stake plus the listed profit.</span></li></ol><div className="odds-grid">{[['Straight', '35:1'], ['Split', '17:1'], ['Street / zero trio', '11:1'], ['Corner / first four', '8:1'], ['Six line', '5:1'], ['Dozen / column', '2:1'], ['Even-money bets', '1:1']].map(([name, odds]) => <div key={name}><span>{name}</span><strong>{odds}</strong></div>)}</div><p>Zero is green and loses on every outside bet. Each of the 37 pockets has the same chance on every spin. Past results do not predict the next result.</p><p>These are free fictional chips with no monetary value. You can refill an empty wallet or reset it in Settings.</p></> : null}
      {panel === 'settings' ? <><div className="setting-row"><div><strong>Table sounds</strong><small>Original chip and result tones</small></div><button aria-pressed={!snapshot.settings.mute} onClick={() => { audio.unlock(); void settings({ mute: !snapshot.settings.mute }); }}>{snapshot.settings.mute ? 'Muted' : 'On'}</button></div><label className="setting-row"><span>Master volume</span><input type="range" min="0" max="1" step="0.05" value={snapshot.settings.volume} onChange={e => void settings({ volume: Number(e.target.value) })}/></label><label className="setting-row"><span>Spin animation</span><select value={snapshot.settings.animationMode} onChange={e => void settings({ animationMode: e.target.value as CasinoSnapshot['settings']['animationMode'] })}><option value="normal">Normal · about 6 seconds</option><option value="fast">Fast · about 2 seconds</option><option value="reduced-motion">Reduced motion</option></select></label>{systemReducedMotion ? <p className="muted">Your system’s reduced-motion preference is active.</p> : null}<div className="setting-row"><div><strong>Agent notification sound</strong><small>{snapshot.features.agentIntegration ? 'Separate from table sounds' : 'Available with desktop agent integration'}</small></div><button disabled={!snapshot.features.agentIntegration} aria-pressed={snapshot.settings.notificationSound} onClick={() => void settings({ notificationSound: !snapshot.settings.notificationSound })}>{snapshot.settings.notificationSound ? 'On' : 'Off'}</button></div>{onWindowMode ? <label className="setting-row"><span>Window size</span><select value={snapshot.settings.windowMode} onChange={e => { const mode = e.target.value as 'compact' | 'standard' | 'fullscreen'; void Promise.resolve(onWindowMode(mode)).then(() => settings({ windowMode: mode })).catch(cause => setError(errorMessage(cause))); }}><option value="compact">Compact</option><option value="standard">Standard</option><option value="fullscreen">Fullscreen</option></select></label> : null}<div className="settings-section"><h3>Your local data</h3><p>{temporary ? 'This session is temporary and will not be saved.' : 'Your wallet, settings, and round history stay on this device.'}</p><button disabled={Boolean(locked)} onClick={() => setPanel('reset')}>Reset wallet to 10,000</button><button disabled={Boolean(locked)} onClick={() => setPanel('clear-history')}>Clear visible history</button></div></> : null}
      {panel === 'stats' ? <><p>Your Classic Roulette record. Gross returns include returned stakes; net change is returns minus all stakes.</p><div className="statistics-grid">{[['Spins', snapshot.stats.spins], ['Total staked', snapshot.stats.totalStake], ['Gross returns', snapshot.stats.grossReturns], ['Net change', signed(snapshot.stats.netChange)], ['Largest net win', snapshot.stats.largestWin], ['Longest winning streak', snapshot.stats.longestWinStreak]].map(([label, value]) => <div key={label}><span className="eyebrow">{label}</span><strong>{typeof value === 'number' ? formatChips(value) : value}</strong></div>)}</div><h3>Little milestones</h3><div className="achievements">{snapshot.achievements.length ? snapshot.achievements.map(key => <span key={key}>◇ {key.replaceAll('-', ' ').replaceAll('_', ' ')}</span>) : <p>Your first spin is the first milestone.</p>}</div></> : null}
      {panel === 'history' ? <>{snapshot.history.length ? <><div className="history-table"><table><thead><tr><th>Result</th><th>Time</th><th>Stake</th><th>Gross return</th><th>Net change</th></tr></thead><tbody>{snapshot.history.slice(historyPage * 20, historyPage * 20 + 20).map(round => <tr key={round.roundId}><td><span className={`recent-pocket pocket-${colorOf(round.outcome)}`}>{round.outcome}</span></td><td>{new Date(round.presentation.committedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</td><td>{formatChips(round.stake)}</td><td>{formatChips(round.grossReturn)}</td><td className={round.netChange > 0 ? 'positive' : ''}>{signed(round.netChange)}</td></tr>)}</tbody></table></div><div className="pagination"><button disabled={historyPage === 0} onClick={() => setHistoryPage(v => v - 1)}>Previous</button><span>Page {historyPage + 1} of {Math.ceil(snapshot.history.length / 20)}</span><button disabled={(historyPage + 1) * 20 >= snapshot.history.length} onClick={() => setHistoryPage(v => v + 1)}>Next</button></div><p className="muted">Past results do not predict future spins.</p></> : <div className="empty-state"><span>◎</span><h3>A clean felt, a fresh start.</h3><p>Your settled rounds will appear here.</p></div>}</> : null}
      {panel === 'reset' ? <><p>Your wallet will start a new chapter with 10,000 free chips. Your round history stays available.</p><div className="confirm-actions"><button onClick={() => setPanel('settings')}>Keep my wallet</button><button className="primary-button" disabled={Boolean(locked)} onClick={() => void action(() => service.resetWallet(crypto.randomUUID())).then(() => { draft.fresh(); setResult(null); })}>Reset wallet</button></div></> : null}
      {panel === 'clear-history' ? <><p>This clears your visible rounds and statistics. Your chip balance stays the same.</p><div className="confirm-actions"><button onClick={() => setPanel('settings')}>Keep history</button><button className="primary-button" disabled={Boolean(locked)} onClick={() => void action(() => service.clearHistory(crypto.randomUUID())).then(() => { setHistoryPage(0); setResult(null); })}>Clear history</button></div></> : null}
      {panel === 'rebet' ? <><p>Replace your {formatChips(total)}-chip draft with the previous round’s {formatChips(lastRound?.stake ?? 0)} chips? This places the bets without starting a spin.</p><div className="confirm-actions"><button onClick={() => setPanel(null)}>Keep draft</button><button className="primary-button" onClick={() => { if (lastRound) replaceDraft(lastRound.bets); setPanel(null); }}>Replace draft</button></div></> : null}
    </Modal> : null}
  </main>;
}
