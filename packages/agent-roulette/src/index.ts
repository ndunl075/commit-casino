/** Pure evidence policy. Call only after the authority drains the inbox watermark.
 * The native authority must persist outcome, payout and reason in one transaction. */
export type Prediction = 'Clean Run' | 'Comeback' | 'Broken Build' | 'Needs Help';
export type Outcome = Prediction | 'Unknown';
/** Availability is a native service capability, never a classifier constant.
 * Browser callers must always report unavailable. Financial commands recheck
 * session freshness, attribution, coverage and registered policy themselves. */
export function isAgentWageringAvailable(capability: { platform: 'desktop' | 'browser'; nativeEvidenceGatePassed: boolean; sessionEligible: boolean }): boolean {
  return capability.platform === 'desktop' && capability.nativeEvidenceGatePassed && capability.sessionEligible;
}
export const classifierVersion = 'agent-classifier-v1' as const;
export const payoutVersion = 'agent-v1' as const;
export const pendingTimeoutMs = 15 * 60 * 1000;
export interface VerificationRun {
  runId: string;
  sessionId: string;
  turnId: string;
  suiteId: string;
  kind: 'test' | 'build';
  startedSequence: number;
  completedSequence: number;
  passed: number;
  failed: number;
  skipped: number;
  observedFailedAttempts: number;
  exitCode: number;
  integrity: 'valid' | 'incomplete' | 'conflicting';
  supported: boolean;
  canceled: boolean;
}
export interface TurnEvidence {
  sessionId: string;
  turnId: string;
  requiredSuiteIds: readonly string[];
  requiredBuildIds: readonly string[];
  runs: readonly VerificationRun[];
  boundary: 'next-turn' | 'session-end' | null;
  boundarySequence: number;
  consumedSequence: number;
  latestStopSequence: number | null;
  lastMutationSequence: number;
  continuousCoverage: boolean;
  attributionValid: boolean;
  mutationTrackingComplete: boolean;
  unaccountedActivity: boolean;
  unresolvedAttention: boolean;
}
export interface Classification { outcome: Outcome; reason: string; classifierVersion: typeof classifierVersion; payoutVersion: typeof payoutVersion }
const result = (outcome: Outcome, reason: string): Classification => ({ outcome, reason, classifierVersion, payoutVersion });
const count = (n: number): boolean => Number.isSafeInteger(n) && n >= 0;
const positive = (n: number): boolean => count(n) && n > 0;

export function classifyTurn(e: TurnEvidence): Classification {
  const unknown = (reason: string) => result('Unknown', reason);
  if (!e.boundary || !positive(e.boundarySequence) || (e.boundary === 'next-turn' && e.latestStopSequence === null) ||
      (e.latestStopSequence !== null && (!count(e.latestStopSequence) || e.latestStopSequence >= e.boundarySequence)) ||
      !count(e.lastMutationSequence) || e.lastMutationSequence >= e.boundarySequence ||
      !count(e.consumedSequence) || e.consumedSequence < e.boundarySequence)
    return unknown('Terminal boundary or drained watermark unavailable');
  if (!e.continuousCoverage || !e.attributionValid || !e.mutationTrackingComplete || e.unaccountedActivity)
    return unknown('Evidence coverage or attribution is uncertain');
  const ids = new Set<string>();
  for (const r of e.runs) {
    if (ids.has(r.runId) || !r.runId || r.sessionId !== e.sessionId || r.turnId !== e.turnId ||
        !positive(r.startedSequence) || !positive(r.completedSequence) ||
        r.completedSequence <= r.startedSequence || r.completedSequence >= e.boundarySequence ||
        r.integrity !== 'valid' || !r.supported || r.canceled ||
        ![r.passed, r.failed, r.skipped, r.observedFailedAttempts].every(count) ||
        !Number.isSafeInteger(r.exitCode) || r.exitCode < 0 ||
        (r.kind === 'test' && r.passed + r.failed === 0) ||
        (r.exitCode === 0 && (r.failed > 0 || r.observedFailedAttempts > 0)) ||
        (r.kind === 'test' && r.exitCode !== 0 && r.failed === 0 && r.observedFailedAttempts === 0))
      return unknown('Unsupported, incomplete, canceled, conflicting or unattributed run');
    ids.add(r.runId);
  }
  const configured = [...e.requiredSuiteIds.map(id => ['test', id] as const), ...e.requiredBuildIds.map(id => ['build', id] as const)];
  if (configured.some(([kind, id], i) => !id || configured.slice(0, i).some(([k, n]) => k === kind && n === id)))
    return unknown('Invalid verification policy');
  const finals: VerificationRun[] = [];
  const observedChecks = new Map<string, VerificationRun[]>();
  for (const r of e.runs) {
    const key = JSON.stringify([r.kind, r.suiteId]);
    const checks = observedChecks.get(key) ?? [];
    checks.push(r); observedChecks.set(key, checks);
  }
  for (const runs of observedChecks.values()) {
    runs.sort((a, b) => a.startedSequence - b.startedSequence);
    if (runs.some((r, i) => i > 0 && (runs[i - 1]?.completedSequence ?? 0) >= r.startedSequence))
      return unknown('Overlapping runs of the same check are ambiguous');
  }
  for (const [kind, suiteId] of configured) {
    const runs = e.runs.filter(r => r.kind === kind && r.suiteId === suiteId).sort((a, b) => a.startedSequence - b.startedSequence);
    if (runs.some((r, i) => i > 0 && (runs[i - 1]?.completedSequence ?? 0) >= r.startedSequence))
      return unknown('Overlapping runs of the same check are ambiguous');
    const final = runs.at(-1);
    if (final) finals.push(final);
  }
  // Unresolved attention can be classified without tests, but never through an integrity error.
  if (e.unresolvedAttention) return result('Needs Help', 'User intervention unresolved at terminal boundary');
  if (finals.some(r => r.startedSequence <= e.lastMutationSequence))
    return unknown('Final verification predates possible workspace mutation');
  if (finals.some(r => r.exitCode !== 0 || r.failed > 0 || r.observedFailedAttempts > 0))
    return result('Broken Build', 'Final required verification failed');
  // Optional registered checks can contribute failures and cannot be hidden by required passes.
  const buildIds = new Set(e.runs.filter(r => r.kind === 'build').map(r => r.suiteId));
  for (const id of buildIds) {
    const runs = e.runs.filter(r => r.kind === 'build' && r.suiteId === id).sort((a, b) => a.startedSequence - b.startedSequence);
    if (runs.some((r, i) => i > 0 && (runs[i - 1]?.completedSequence ?? 0) >= r.startedSequence)) return unknown('Overlapping build checks');
    const final = runs.at(-1);
    if (final && final.exitCode !== 0) return result('Broken Build', 'Final registered build failed');
  }
  if (e.requiredSuiteIds.length === 0 || finals.length !== configured.length)
    return unknown('Every required test suite needs a fresh final pass');
  const earlierFailure = e.runs.some(r => r.exitCode !== 0 || r.failed > 0 || r.observedFailedAttempts > 0);
  return result(earlierFailure ? 'Comeback' : 'Clean Run', earlierFailure ? 'Fresh required suites pass after observed failure' : 'Fresh required suites pass without observed failures');
}

/** Refunds are neither wins nor losses. Safe-integer protection belongs at debit too. */
export function settlePrediction(prediction: Prediction, outcome: Outcome, stake: number): { grossReturn: number; netChange: number; void: boolean } {
  if (!['Clean Run', 'Comeback', 'Broken Build', 'Needs Help'].includes(prediction) || !positive(stake) || stake % 10 !== 0 || stake > Math.floor(Number.MAX_SAFE_INTEGER / 2)) throw new Error('INVALID_AGENT_STAKE');
  if (!['Clean Run', 'Comeback', 'Broken Build', 'Needs Help', 'Unknown'].includes(outcome)) throw new Error('INVALID_AGENT_OUTCOME');
  const grossReturn = outcome === 'Unknown' ? stake : prediction === outcome ? stake * 2 : 0;
  return { grossReturn, netChange: grossReturn - stake, void: outcome === 'Unknown' };
}
