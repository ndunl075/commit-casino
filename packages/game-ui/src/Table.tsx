import { useState } from 'react';
import type { BetInput } from '@commit-casino/casino-contracts';
import { betName, colorOf, formatChips, shortChips, tableTargets, TABLE_HEIGHT, TABLE_WIDTH, type TableBet } from './geometry';

export function RouletteTable({ bets, locked, outcome, onPlace, onHover }: { bets: readonly BetInput[]; locked: boolean; outcome?: number; onPlace: (id: string) => void; onHover: (bet: TableBet | null) => void }) {
  const [hovered, setHovered] = useState<TableBet | null>(null);
  function preview(bet: TableBet | null) { setHovered(bet); onHover(bet); }
  return <div className="table-scroll" aria-label="Roulette betting table">
    <div className="roulette-table" style={{ aspectRatio: `${TABLE_WIDTH}/${TABLE_HEIGHT}` }}>
      <svg className="felt-grid" viewBox={`0 0 ${TABLE_WIDTH} ${TABLE_HEIGHT}`} aria-hidden="true">
        <defs><pattern id="felt-grain" width="4" height="4" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r="0.45" fill="#fff" opacity=".075"/></pattern></defs>
        <rect width={TABLE_WIDTH} height={TABLE_HEIGHT} fill="url(#felt-grain)"/>
        <path d="M80 24H848V234H80Z M80 94H848 M80 164H848" fill="none" stroke="#b2c5ad" strokeOpacity=".65" strokeWidth="1.3"/>
        {Array.from({ length: 11 }, (_, i) => <path key={i} d={`M${144 + i * 64} 24V234`} stroke="#b2c5ad" strokeOpacity=".65" strokeWidth="1.3"/>)}
        <path d="M80 24H24L8 129 24 234H80 M848 24H924V234H848 M848 94H924 M848 164H924" fill="none" stroke="#b2c5ad" strokeOpacity=".65" strokeWidth="1.3"/>
        {tableTargets.filter(t => t.bet.numbers.length === 1 && t.bet.numbers[0] !== 0).map(t => <rect key={t.bet.id} x={t.x + 4} y={t.y + 6} width={t.w - 8} height={t.h - 12} rx="3" fill={colorOf(t.bet.numbers[0]!) === 'red' ? '#ad3839' : '#14241e'} opacity=".9"/>)}
        {[0, 1, 2].map(i => <rect key={i} x={80 + i * 256} y="262" width="256" height="40" fill="none" stroke="#b2c5ad" strokeOpacity=".65"/>)}
        {[0, 1, 2, 3, 4, 5].map(i => <rect key={i} x={80 + i * 128} y="312" width="128" height="44" fill="none" stroke="#b2c5ad" strokeOpacity=".65"/>)}
      </svg>
      {tableTargets.map(t => {
        const stake = bets.find(b => b.betId === t.bet.id)?.stake;
        const covered = hovered && t.bet.numbers.length === 1 && hovered.numbers.includes(t.bet.numbers[0]!);
        const won = outcome !== undefined && t.bet.numbers.length === 1 && t.bet.numbers[0] === outcome;
        return <button key={t.bet.id} type="button" data-bet-id={t.bet.id} disabled={locked}
          className={`bet-target ${t.compact ? 'geometry-target' : 'cell-target'} ${covered ? 'covered' : ''} ${won ? 'winning-pocket' : ''} ${t.bet.id === 'outside:red' ? 'red-outside' : ''} ${t.bet.id === 'outside:black' ? 'black-outside' : ''}`}
          style={{ left: `${t.x / TABLE_WIDTH * 100}%`, top: `${t.y / TABLE_HEIGHT * 100}%`, width: `${t.w / TABLE_WIDTH * 100}%`, height: `${t.h / TABLE_HEIGHT * 100}%` }}
          aria-label={`${betName(t.bet)}, pays ${t.bet.profitOdds} to 1${stake ? `, ${formatChips(stake)} chips placed` : ''}`}
          onPointerEnter={() => preview(t.bet)} onPointerLeave={() => preview(null)} onFocus={() => preview(t.bet)} onBlur={() => preview(null)} onClick={() => onPlace(t.bet.id)}>
          <span className="target-label">{t.label}</span>
          {stake ? <span className="placed-chip" aria-hidden="true">{shortChips(stake)}</span> : null}
        </button>;
      })}
    </div>
  </div>;
}
