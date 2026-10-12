/** Fixture support is deliberately distinct from a claim of live compatibility. */
export const testedClaudeVersions: readonly string[] = ['2.1.287'];
export const claudeIntegrationStatus = { passive: true, wageringAuthority: 'native-evidence-gate', reason: 'Windows Claude2.1.287 real turn/tool/permission hooks verified with a local mock API; native evidence, attribution and settlement gate determines wagering availability' } as const;
export type ToolEffect = 'read' | 'possible-mutation' | 'other';
export type Observation =
  | { type: 'session-start' | 'session-end' | 'turn-start' | 'stop-candidate'; payload: Record<string, never> }
  | { type: 'tool-start' | 'tool-end'; payload: { toolId: string; effect: ToolEffect; status?: 'success' | 'failure' } }
  | { type: 'attention-requested'; payload: { reason: 'permission' | 'idle' } };
export type EventEnvelope = Observation & { version: 1; eventId: string; installationId: string; provider: 'claude'; sessionId: string; turnId?: string; sourceSequence: number; observedAt: string };
export interface NormalizationContext {
  providerVersion: string;
  supportedVersions?: readonly string[];
  installationId: string;
  eventId: string;
  sourceSequence: number;
  observedAt: string;
  turnId?: string;
}
export type NormalizationResult = { ok: true; event: EventEnvelope } | { ok: false; reason: 'VERSION_MISMATCH' | 'INVALID_INPUT' | 'UNSUPPORTED_EVENT' | 'AMBIGUOUS_ATTRIBUTION' };
const opaque = (v: unknown): v is string => typeof v === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(v);
const object = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
export function toolEffect(toolName: unknown): ToolEffect {
  if (['Read', 'Glob', 'Grep', 'WebFetch', 'WebSearch'].includes(String(toolName))) return 'read';
  if (['Write', 'Edit', 'NotebookEdit', 'Bash', 'PowerShell'].includes(String(toolName)) || typeof toolName !== 'string' || toolName.startsWith('mcp__')) return 'possible-mutation';
  // Unknown tools may mutate, so they cannot preserve freshness.
  return 'possible-mutation';
}

/** Never spread provider input: only explicitly allowlisted metadata leaves this function. */
export function normalizeClaudeHook(raw: unknown, c: NormalizationContext): NormalizationResult {
  if (!(c.supportedVersions ?? testedClaudeVersions).includes(c.providerVersion)) return { ok: false, reason: 'VERSION_MISMATCH' };
  if (!object(raw) || !opaque(raw.session_id) || !opaque(c.installationId) || !opaque(c.eventId) ||
      !Number.isSafeInteger(c.sourceSequence) || c.sourceSequence < 1 ||
      !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,3})?Z$/.test(c.observedAt) || !Number.isFinite(Date.parse(c.observedAt)) ||
      (c.turnId !== undefined && !opaque(c.turnId))) return { ok: false, reason: 'INVALID_INPUT' };
  if (raw.agent_id !== undefined || raw.agent_type !== undefined) return { ok: false, reason: 'AMBIGUOUS_ATTRIBUTION' };
  let observation: Observation;
  switch (raw.hook_event_name) {
    case 'SessionStart': observation = { type: 'session-start', payload: {} }; break;
    case 'SessionEnd': observation = { type: 'session-end', payload: {} }; break;
    case 'UserPromptSubmit': observation = { type: 'turn-start', payload: {} }; break;
    case 'Stop': observation = { type: 'stop-candidate', payload: {} }; break;
    case 'PreToolUse': case 'PostToolUse': case 'PostToolUseFailure':
      if (!opaque(raw.tool_use_id)) return { ok: false, reason: 'INVALID_INPUT' };
      observation = { type: raw.hook_event_name === 'PreToolUse' ? 'tool-start' : 'tool-end', payload: { toolId: raw.tool_use_id, effect: toolEffect(raw.tool_name), ...(raw.hook_event_name === 'PreToolUse' ? {} : { status: raw.hook_event_name === 'PostToolUse' ? 'success' as const : 'failure' as const }) } }; break;
    case 'PermissionRequest': observation = { type: 'attention-requested', payload: { reason: 'permission' } }; break;
    case 'Notification':
      if (raw.notification_type !== 'permission_prompt' && raw.notification_type !== 'idle_prompt') return { ok: false, reason: 'UNSUPPORTED_EVENT' };
      observation = { type: 'attention-requested', payload: { reason: raw.notification_type === 'permission_prompt' ? 'permission' : 'idle' } }; break;
    default: return { ok: false, reason: 'UNSUPPORTED_EVENT' };
  }
  return { ok: true, event: { version: 1, eventId: c.eventId, installationId: c.installationId, provider: 'claude', sessionId: raw.session_id, ...(c.turnId ? { turnId: c.turnId } : {}), sourceSequence: c.sourceSequence, observedAt: c.observedAt, ...observation } };
}

export interface SessionObservation {
  sessionId: string;
  sequence: number;
  currentTurnId: string | null;
  state: 'idle' | 'working' | 'attention' | 'stopped' | 'ended' | 'uncertain';
  lastMutationSequence: number;
  stopSequence: number | null;
  evidenceGap: boolean;
  toolTurns: Readonly<Record<string, string>>;
}
/** Presentation reducer. Financial arming must additionally synchronize the native journal. */
export function observeSession(previous: SessionObservation | null, event: EventEnvelope, nextTurnId?: string): SessionObservation {
  if (previous && previous.sessionId !== event.sessionId) throw new Error('SESSION_MISMATCH');
  const s: SessionObservation = previous ? { ...previous, toolTurns: { ...previous.toolTurns } } : { sessionId: event.sessionId, sequence: 0, currentTurnId: null, state: 'uncertain', lastMutationSequence: 0, stopSequence: null, evidenceGap: false, toolTurns: {} };
  if (event.sourceSequence <= s.sequence) return s;
  if (event.sourceSequence !== s.sequence + 1) s.evidenceGap = true;
  s.sequence = event.sourceSequence;
  switch (event.type) {
    case 'session-start': s.state = s.currentTurnId ? 'uncertain' : 'idle'; break;
    case 'turn-start':
      if (!nextTurnId || !opaque(nextTurnId) || (s.currentTurnId !== null && s.stopSequence === null)) { s.evidenceGap = true; s.state = 'uncertain'; break; }
      s.currentTurnId = nextTurnId; s.state = 'working'; s.stopSequence = null; break;
    case 'tool-start':
      if (!s.currentTurnId || s.toolTurns[event.payload.toolId]) { s.evidenceGap = true; s.state = 'uncertain'; break; }
      s.toolTurns = { ...s.toolTurns, [event.payload.toolId]: s.currentTurnId };
      if (event.payload.effect !== 'read') s.lastMutationSequence = event.sourceSequence;
      s.state = 'working'; s.stopSequence = null; break;
    case 'tool-end':
      if (!s.toolTurns[event.payload.toolId] || s.toolTurns[event.payload.toolId] !== s.currentTurnId) { s.evidenceGap = true; s.state = 'uncertain'; break; }
      s.toolTurns = Object.fromEntries(Object.entries(s.toolTurns).filter(([id]) => id !== event.payload.toolId));
      s.state = 'working'; s.stopSequence = null; break;
    case 'attention-requested': s.state = 'attention'; break;
    case 'stop-candidate': s.state = 'stopped'; s.stopSequence = event.sourceSequence; break;
    case 'session-end': s.state = 'ended'; break;
  }
  return s;
}
