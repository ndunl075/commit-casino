// Explicit expected categories form the policy contract; explanations are frozen
// from the TS implementation and compared byte-for-byte by Rust and TS tests.
import fs from 'node:fs';
import { classifyTurn } from '../../packages/agent-roulette/src/index.ts';
const run = (p = {}) => ({ runId:'r1', sessionId:'s', turnId:'t', suiteId:'unit', kind:'test', startedSequence:3, completedSequence:5, passed:2, failed:0, skipped:0, observedFailedAttempts:0, exitCode:0, integrity:'valid', supported:true, canceled:false, ...p });
const e = (p = {}) => ({ sessionId:'s', turnId:'t', requiredSuiteIds:['unit'], requiredBuildIds:[], runs:[run()], boundary:'next-turn', boundarySequence:10, consumedSequence:10, latestStopSequence:9, lastMutationSequence:2, continuousCoverage:true, attributionValid:true, mutationTrackingComplete:true, unaccountedActivity:false, unresolvedAttention:false, ...p });
const failure = run({runId:'early',startedSequence:1,completedSequence:2,passed:0,failed:1,exitCode:1,observedFailedAttempts:1});
const build = run({runId:'build',suiteId:'build',kind:'build',passed:0,failed:0,exitCode:1,startedSequence:6,completedSequence:7});
const cases = [
  ['clean','Clean Run',e()],
  ['session-end-clean','Clean Run',e({boundary:'session-end'})],
  ['session-end-without-stop-attention','Needs Help',e({boundary:'session-end',latestStopSequence:null,unresolvedAttention:true,runs:[]})],
  ['session-end-without-stop-no-tests','Unknown',e({boundary:'session-end',latestStopSequence:null,runs:[]})],
  ['session-end-without-stop-fresh-tests','Clean Run',e({boundary:'session-end',latestStopSequence:null})],
  ['comeback','Comeback',e({runs:[failure,run()]})],
  ['last-run-fails','Broken Build',e({runs:[run(),run({runId:'last',startedSequence:6,completedSequence:7,passed:0,failed:1,exitCode:1})]})],
  ['required-failure-not-hidden','Broken Build',e({runs:[run({...failure,startedSequence:3,completedSequence:5}),run({runId:'other',suiteId:'unrelated',startedSequence:6,completedSequence:7})]})],
  ['parallel-required-suites','Clean Run',e({requiredSuiteIds:['unit','e2e'],runs:[run(),run({runId:'e2e',suiteId:'e2e',startedSequence:4,completedSequence:6})]})],
  ['overlapping-same-suite','Unknown',e({runs:[run({completedSequence:8}),run({runId:'overlap',startedSequence:4,completedSequence:6})]})],
  ['unresolved-attention','Needs Help',e({unresolvedAttention:true,runs:[]})],
  ['attention-after-gap','Unknown',e({unresolvedAttention:true,continuousCoverage:false})],
  ['resolved-attention-clean','Clean Run',e({unresolvedAttention:false})],
  ['build-failure-after-tests','Broken Build',e({runs:[run(),build]})],
  ['build-failure-without-tests','Broken Build',e({runs:[build]})],
  ['required-build-missing','Unknown',e({requiredBuildIds:['build']})],
  ['required-build-fails','Broken Build',e({requiredBuildIds:['build'],runs:[run(),build]})],
  ['build-only-pass','Unknown',e({requiredSuiteIds:[],runs:[run({kind:'build'})]})],
  ['build-comeback','Comeback',e({runs:[run(),run({...build,startedSequence:1,completedSequence:2}),run({...build,runId:'build-pass',exitCode:0,startedSequence:6,completedSequence:7})]})],
  ['missing-suite','Unknown',e({requiredSuiteIds:['unit','e2e']})],
  ['no-tests','Unknown',e({runs:[]})],
  ['no-required-suites','Unknown',e({requiredSuiteIds:[]})],
  ['missing-boundary','Unknown',e({boundary:null})],
  ['undrained-watermark','Unknown',e({consumedSequence:9})],
  ['stop-candidate-only','Unknown',e({latestStopSequence:null})],
  ['stop-after-boundary','Unknown',e({latestStopSequence:10})],
  ['stale-verification','Unknown',e({lastMutationSequence:4})],
  ['mutation-tracking-incomplete','Unknown',e({mutationTrackingComplete:false})],
  ['unaccounted-tool','Unknown',e({unaccountedActivity:true})],
  ['attribution-conflict','Unknown',e({attributionValid:false})],
  ['event-gap','Unknown',e({continuousCoverage:false})],
  ['zero-tests','Unknown',e({runs:[run({passed:0})]})],
  ['flaky-final','Unknown',e({runs:[run({observedFailedAttempts:1})]})],
  ['exit-report-conflict','Unknown',e({runs:[run({exitCode:1})]})],
  ['canceled-final','Unknown',e({runs:[run(),run({runId:'canceled',startedSequence:6,completedSequence:7,canceled:true})]})],
  ['incomplete-run','Unknown',e({runs:[run({integrity:'incomplete'})]})],
  ['unsupported-version','Unknown',e({runs:[run({supported:false})]})],
  ['wrong-session','Unknown',e({runs:[run({sessionId:'other'})]})],
  ['wrong-turn','Unknown',e({runs:[run({turnId:'other'})]})],
  ['duplicate-run','Unknown',e({runs:[run(),run()]})],
  ['late-completion','Unknown',e({runs:[run({completedSequence:11})]})],
  ['ambiguous-build-before-attention','Unknown',e({unresolvedAttention:true,runs:[build,run({...build,runId:'other-build',startedSequence:6,completedSequence:8})]})],
  ['duplicate-policy-suite','Unknown',e({requiredSuiteIds:['unit','unit']})],
];
const fixtures = cases.map(([name,expectedOutcome,evidence]) => {
  const expected = classifyTurn(evidence);
  if(expected.outcome!==expectedOutcome) throw new Error(`Policy fixture ${name}: expected ${expectedOutcome}, received ${expected.outcome}`);
  return {name,evidence,expected};
});
fs.writeFileSync(new URL('../../tests/fixtures/claude/agent-classifier-v1.json',import.meta.url),JSON.stringify(fixtures,null,2)+'\n');
