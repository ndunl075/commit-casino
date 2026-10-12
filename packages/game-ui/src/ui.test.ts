import { describe, expect, it } from 'vitest';
import { catalog } from '@commit-casino/roulette-engine';
import { tableTargets, TABLE_HEIGHT, TABLE_WIDTH } from './geometry';
import { createDraftStore } from './draft';
import { landingAngle, POCKET_ANGLE, wheelPose } from './wheelMath';

describe('canonical table geometry', () => {
  it('provides a distinct, in-bounds target for all 157 canonical bets', () => {
    expect(tableTargets).toHaveLength(157);
    expect(new Set(tableTargets.map(t => t.bet.id)).size).toBe(157);
    for (const target of tableTargets) {
      expect(target.x).toBeGreaterThanOrEqual(0); expect(target.y).toBeGreaterThanOrEqual(0);
      expect(target.x + target.w).toBeLessThanOrEqual(TABLE_WIDTH); expect(target.y + target.h).toBeLessThanOrEqual(TABLE_HEIGHT);
      const center = { x: target.x + target.w / 2, y: target.y + target.h / 2 };
      const matches = tableTargets.filter(t => center.x > t.x && center.x < t.x + t.w && center.y > t.y && center.y < t.y + t.h);
      expect(matches.map(t => t.bet.id), `ambiguous center of ${target.bet.id}`).toEqual([target.bet.id]);
    }
  });
  it('covers every zero-end geometry', () => {
    expect(tableTargets.filter(t => t.bet.numbers.includes(0))).toHaveLength(7);
  });
});
describe('presentation wheel geometry', () => {
  it('lands the ball in the exact committed pocket for every outcome and seed', () => {
    for (const seed of [0, 129, 1000, 4294967295]) for (const [index] of catalog.wheelOrder.entries()) {
      const pose = wheelPose(1, index, seed);
      expect(pose.ball).toBeCloseTo(landingAngle(index, pose.wheel), 10);
      expect((pose.ball - pose.wheel) / POCKET_ANGLE).toBeCloseTo(index, 10);
      expect(pose.radius).toBe(157);
    }
  });
});
describe('draft operation history', () => {
  it('undoes an entire double or rebet as one operation and keeps stores isolated', () => {
    const store = createDraftStore(); const second = createDraftStore();
    store.getState().place('straight:7'); store.getState().place('outside:red');
    const original = store.getState().bets;
    store.getState().replace(original.map(b => ({ ...b, stake: b.stake * 2 })));
    store.getState().undo(); expect(store.getState().bets).toEqual(original);
    store.getState().replace([{ betId: 'straight:0', stake: 500 }]);
    store.getState().undo(); expect(store.getState().bets).toEqual(original);
    expect(second.getState().bets).toEqual([]);
  });
  it('aggregates repeated placements and undo restores each operation', () => {
    const store = createDraftStore(); store.getState().place('straight:7'); store.getState().place('straight:7');
    expect(store.getState().bets).toEqual([{ betId: 'straight:7', stake: 200 }]);
    store.getState().undo(); expect(store.getState().bets).toEqual([{ betId: 'straight:7', stake: 100 }]);
    store.getState().clear(); store.getState().undo(); expect(store.getState().bets).toHaveLength(1);
    store.getState().fresh(); expect(store.getState().undoStack).toEqual([]);
  });
});
