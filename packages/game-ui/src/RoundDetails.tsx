import type { CommittedRound } from '@commit-casino/casino-contracts';
import { catalog, settle } from '@commit-casino/roulette-engine';
import { betName, formatChips } from './geometry';

export function RoundDetails({ round }: { round: CommittedRound }) {
  const date = new Date(round.presentation.committedAt);
  return <details className="round-details"><summary aria-label={`View bets for result ${round.outcome} at ${date.toLocaleString()}`}><time dateTime={round.presentation.committedAt} title={date.toLocaleString()}>{date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time><span>View bets</span></summary><ul>{round.bets.map(bet => {
    const definition = catalog.bets.find(entry => entry.id === bet.betId);
    return <li key={bet.betId}><strong>{definition ? betName(definition) : bet.betId}</strong><span>{formatChips(bet.stake)} staked · {formatChips(settle([bet], round.outcome).grossReturn)} returned</span></li>;
  })}</ul></details>;
}

export function BetFrequencies({ frequencies }: { frequencies: Record<string, number> | undefined }) {
  const entries = Object.entries(frequencies ?? {}).filter(([, count]) => count > 0).sort((a, b) => b[1] - a[1]);
  return <section className="bet-frequencies"><h3>Your most played bets</h3>{entries.length ? <ol>{entries.slice(0, 10).map(([id, count]) => {
    const definition = catalog.bets.find(entry => entry.id === id);
    return <li key={id}><span>{definition ? betName(definition) : id}</span><strong>{formatChips(count)} {count === 1 ? 'round' : 'rounds'}</strong></li>;
  })}</ol> : <p>Bet frequencies will appear after settled rounds.</p>}<p className="muted">Frequency describes your choices; it does not predict future results.</p></section>;
}
