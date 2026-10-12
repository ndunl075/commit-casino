import { expect, it } from 'vitest';
import fixtures from '../../../tests/fixtures/claude/agent-classifier-v1.json';
import { classifyTurn, type TurnEvidence } from './index.js';
it.each(fixtures)('shared Rust/TypeScript classifier policy: $name', fixture => {
  expect(classifyTurn(fixture.evidence as TurnEvidence)).toEqual(fixture.expected);
});
