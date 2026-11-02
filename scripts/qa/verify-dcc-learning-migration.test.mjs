import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { verifyOutcome } from './verify-dcc-learning-migration.mjs';

test('accepts nonzero passing Rust and frontend test counts', () => {
  assert.equal(verifyOutcome('rust', { status: 0, stdout: 'test result: ok. 89 passed; 0 failed;', stderr: '' }), 89);
  assert.equal(verifyOutcome('frontend', { status: 0, stdout: ' Test Files  1 passed (1)\n Tests  13 passed (13)', stderr: '' }), 13);
});

test('rejects failed, interrupted, zero-test and unrecognizable outcomes', () => {
  for (const result of [
    { status: 1, stdout: 'test result: ok. 1 passed; 0 failed;', stderr: '' },
    { status: null, signal: 'SIGTERM', stdout: '', stderr: '' },
    { status: 0, stdout: 'test result: ok. 0 passed; 0 failed;', stderr: '' },
    { status: 0, stdout: 'nothing ran', stderr: '' },
  ]) assert.throws(() => verifyOutcome('rust', result));
  assert.throws(() => verifyOutcome('frontend', { status: 0, stdout: 'Tests 0 passed', stderr: '' }));
});
