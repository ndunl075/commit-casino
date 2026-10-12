import { catalog } from '@commit-casino/roulette-engine';

export type TableBet = (typeof catalog.bets)[number];
export interface BetTarget { bet: TableBet; x: number; y: number; w: number; h: number; label: string; compact: boolean }
export const TABLE_WIDTH = 940;
export const TABLE_HEIGHT = 368;
export const reds = new Set<number>(catalog.redNumbers);
export const colorOf = (n: number) => n === 0 ? 'green' : reds.has(n) ? 'red' : 'black';
export const formatChips = (n: number) => n.toLocaleString('en-US');
export const shortChips = (n: number) => n >= 1000 ? `${Number((n / 1000).toFixed(1))}k` : String(n);
export function betName(bet: TableBet): string {
  const kind = bet.kind.replaceAll('-', ' ').replaceAll('_', ' ');
  if (bet.id.startsWith('outside:')) return bet.id.split(':')[1]!.replace('low', '1–18').replace('high', '19–36');
  if (bet.numbers.length === 1) return `Straight ${bet.numbers[0]}`;
  if (bet.numbers.length === 12 && bet.numbers[1]! - bet.numbers[0]! === 1) return `${bet.numbers[0]}–${bet.numbers[11]} dozen`;
  if (bet.numbers.length === 12) return `Column ${bet.numbers[0]}`;
  return `${kind} ${bet.numbers.join('–')}`;
}
const point = (n: number) => ({ x: 80 + Math.floor((n - 1) / 3) * 64 + 32, y: 24 + (2 - ((n - 1) % 3)) * 70 + 35 });
const target = (bet: TableBet, x: number, y: number, w: number, h: number, label = '', compact = true): BetTarget => ({ bet, x, y, w, h, label, compact });

/** One non-overlapping, semantic target per canonical bet. Insets reserve edges and intersections. */
export function geometryFor(bet: TableBet): BetTarget {
  const ns = bet.numbers;
  if (ns.length === 1) {
    if (ns[0] === 0) return target(bet, 18, 24, 51, 210, '0', false);
    const p = point(ns[0]!); return target(bet, p.x - 23, p.y - 26, 46, 52, String(ns[0]), false);
  }
  if (ns.includes(0)) {
    if (ns.length === 4) return target(bet, 70, 235, 20, 20, '4');
    if (ns.length === 3) return target(bet, 70, ns.includes(1) ? 154 : 84, 20, 20);
    const p = point(ns[1]!); return target(bet, 70, p.y - 24, 20, 48);
  }
  if (ns.length === 2) {
    const a = point(ns[0]!); const b = point(ns[1]!);
    return a.x === b.x ? target(bet, a.x - 22, (a.y + b.y) / 2 - 9, 44, 18) : target(bet, (a.x + b.x) / 2 - 9, a.y - 25, 18, 50);
  }
  if (ns.length === 3) return target(bet, point(ns[0]!).x - 22, 235, 44, 20, '3');
  if (ns.length === 4) {
    const a = point(ns[0]!); const b = point(ns[3]!);
    return target(bet, (a.x + b.x) / 2 - 9, (a.y + b.y) / 2 - 9, 18, 18);
  }
  if (ns.length === 6) return target(bet, point(ns[0]!).x + 23, 235, 18, 20, '6');
  if (ns.length === 12) {
    if (ns[1]! - ns[0]! === 3) return target(bet, 852, point(ns[0]!).y - 34, 67, 68, '2 : 1', false);
    const index = Math.floor((ns[0]! - 1) / 12);
    return target(bet, 80 + index * 256, 262, 254, 40, ['1st 12', '2nd 12', '3rd 12'][index]!, false);
  }
  const suffix = bet.id.split(':')[1];
  const index = ['low', 'even', 'red', 'black', 'odd', 'high'].indexOf(suffix!);
  const labels = ['1 – 18', 'EVEN', 'RED', 'BLACK', 'ODD', '19 – 36'];
  return target(bet, 80 + index * 128, 312, 126, 44, labels[index]!, false);
}
export const tableTargets = catalog.bets.map(geometryFor);
