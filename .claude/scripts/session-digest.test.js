const assert = require('assert');
const {
  isPureExitStatusNoise,
  recoverySignature,
  withRepeatMeta,
  extractErrorRecoveries
} = require('./session-digest.js');

// Test suite counter
let passedTests = 0;
let totalTests = 0;

function test(name, fn) {
  totalTests++;
  try {
    fn();
    console.log(`✓ ${name}`);
    passedTests++;
  } catch (err) {
    console.error(`✗ ${name}`);
    console.error(err);
  }
}

// Helper to construct recovery messages by pairing each error with a recovery success immediately
function makeRecoveryMessages(toolName, pairs) {
  const messages = [];
  let toolUseId = 1;

  for (const p of pairs) {
    // 1. Error tool_use
    const errId = `tool_${toolUseId++}`;
    messages.push({
      _role: 'assistant',
      _content: [{
        type: 'tool_use',
        id: errId,
        name: toolName,
        input: {
          file_path: p.file,
          command: p.cmd
        }
      }]
    });
    // 2. Error tool_result
    messages.push({
      _role: 'user',
      _content: [{
        type: 'tool_result',
        tool_use_id: errId,
        is_error: true,
        content: p.errorText
      }]
    });

    // 3. Success tool_use (recovery)
    const succId = `tool_${toolUseId++}`;
    messages.push({
      _role: 'assistant',
      _content: [{
        type: 'tool_use',
        id: succId,
        name: toolName,
        input: {
          file_path: p.file,
          command: p.cmd
        }
      }]
    });
    // 4. Success tool_result
    messages.push({
      _role: 'user',
      _content: [{
        type: 'tool_result',
        tool_use_id: succId,
        is_error: false,
        content: 'Success'
      }]
    });
  }

  return messages;
}

// ---------------------------------------------------------------------------
// Case 1: Pure exit code is filtered
// ---------------------------------------------------------------------------
test('Case 1: Pure exit code is filtered', () => {
  const messages = makeRecoveryMessages('Bash', 
    [{ errorText: 'Exit code 2', cmd: 'node run.js' }]
  );
  const signals = extractErrorRecoveries(messages);
  assert.strictEqual(signals.length, 0, 'Should filter out pure exit code 2');
});

// ---------------------------------------------------------------------------
// Case 2: Real format variants are all filtered
// ---------------------------------------------------------------------------
test('Case 2: Real format variants are all filtered', () => {
  const noiseVariants = [
    'Exit code 2',
    'exit status 127',
    'Command failed with exit code 2',
    'Process exited with code 1',
    'Bash command failed with exit code 2',
    'Error: Command failed with exit code 2',
    '[Error] Exit code 1',
    '(Exit code 2)',
    '  Exit code 2  ',
    'exit status: 127',
    'Command failed with exit code: 2',
  ];
  for (const variant of noiseVariants) {
    assert.strictEqual(isPureExitStatusNoise(variant), true, `Should identify "${variant}" as noise`);
  }
});

// ---------------------------------------------------------------------------
// Case 3: Short real errors are not killed
// ---------------------------------------------------------------------------
test('Case 3: Short real errors are not killed', () => {
  const realErrors = [
    'ENOENT',
    'EACCES',
    'Killed',
    'not found',
    'SIGTERM',
    'Error: ENOENT',
  ];
  for (const error of realErrors) {
    assert.strictEqual(isPureExitStatusNoise(error), false, `Should NOT identify "${error}" as noise`);
  }
});

// ---------------------------------------------------------------------------
// Case 4: Multiline with noise template and real error is not killed
// ---------------------------------------------------------------------------
test('Case 4: Multiline with noise template and real error is not killed', () => {
  const errorText = "Exit code 1\nTypeError: foo is undefined";
  assert.strictEqual(isPureExitStatusNoise(errorText), false, 'Should NOT identify multiline with real error as noise');
});

// ---------------------------------------------------------------------------
// Case 5: Class B cross-file aggregation
// ---------------------------------------------------------------------------
test('Case 5: Class B cross-file aggregation', () => {
  const errors = [
    { errorText: 'File has not been read yet', file: 'src/a.js' },
    { errorText: 'File has not been read yet', file: 'src/b.js' },
    { errorText: 'File has not been read yet', file: 'src/c.js' },
    { errorText: 'File has not been read yet', file: 'src/d.js' },
    { errorText: 'File has not been read yet', file: 'src/e.js' },
    { errorText: 'File has not been read yet', file: 'src/f.js' },
  ];
  const messages = makeRecoveryMessages('Bash', errors);
  const signals = extractErrorRecoveries(messages);
  
  assert.strictEqual(signals.length, 1, 'Should aggregate to 1 signal');
  assert.strictEqual(signals[0].error, 'File has not been read yet');
  assert.strictEqual(
    signals[0].context.includes('[repeat_count=6 same_session=true files=6]'),
    true,
    `Context should contain correct marker, got: ${signals[0].context}`
  );
});

// ---------------------------------------------------------------------------
// Case 6: count === 1 has no marker
// ---------------------------------------------------------------------------
test('Case 6: count === 1 has no marker', () => {
  const errors = [{ errorText: 'File has not been read yet', file: 'src/a.js' }];
  const messages = makeRecoveryMessages('Bash', errors);
  const signals = extractErrorRecoveries(messages);
  
  assert.strictEqual(signals.length, 1);
  assert.strictEqual(signals[0].context || '', '', 'Context should remain clean or unchanged without repeat marker');
});

// ---------------------------------------------------------------------------
// Case 7: Long common prefix different errors are not false-merged
// ---------------------------------------------------------------------------
test('Case 7: Long common prefix different errors are not false-merged', () => {
  const rawError1 = "failed to run custom build command for crate-A and it exited with some detailed rust compiler error because of lifetime issue";
  const rawError2 = "failed to run custom build command for crate-B and it exited with some detailed rust compiler error because of lifetime issue";
  const sig1 = recoverySignature('Bash', rawError1);
  const sig2 = recoverySignature('Bash', rawError2);
  assert.notStrictEqual(sig1, sig2, 'Signatures should differ for different crates');
});

// ---------------------------------------------------------------------------
// Case 8: Error code tokens are preserved
// ---------------------------------------------------------------------------
test('Case 8: Error code tokens are preserved', () => {
  assert.notStrictEqual(
    recoverySignature('Bash', 'HTTP 500 Internal Server Error'),
    recoverySignature('Bash', 'HTTP 404 Not Found'),
    'HTTP 500 and 404 should have different signatures'
  );
  assert.notStrictEqual(
    recoverySignature('Bash', 'error: E0277 trait bound not satisfied'),
    recoverySignature('Bash', 'error: E0308 mismatched types'),
    'E0277 and E0308 should have different signatures'
  );
});

// ---------------------------------------------------------------------------
// Case 9: Same error with different path/line numbers should merge
// ---------------------------------------------------------------------------
test('Case 9: Same error with different path/line numbers should merge', () => {
  const sig1 = recoverySignature('Bash', 'error at /a/foo.rs:12:3: compiler crash');
  const sig2 = recoverySignature('Bash', 'error at /b/foo.rs:88:1: compiler crash');
  assert.strictEqual(sig1, sig2, 'Should normalize path and line numbers and merge');
});

// ---------------------------------------------------------------------------
// Case 10: Different tools do not merge
// ---------------------------------------------------------------------------
test('Case 10: Different tools do not merge', () => {
  const sig1 = recoverySignature('Read', 'some error');
  const sig2 = recoverySignature('Bash', 'some error');
  assert.notStrictEqual(sig1, sig2, 'Signatures should include tool name');
});

// ---------------------------------------------------------------------------
// Case 11: Filtering happens before aggregation
// ---------------------------------------------------------------------------
test('Case 11: Filtering happens before aggregation', () => {
  const errors = [
    { errorText: 'Exit code 2', cmd: 'node run.js' },
    { errorText: 'Exit code 2', cmd: 'node run.js' },
    { errorText: 'TypeError: undefined is not a function', cmd: 'node test.js' }
  ];
  const messages = makeRecoveryMessages('Bash', errors);
  const signals = extractErrorRecoveries(messages);
  
  assert.strictEqual(signals.length, 1);
  assert.strictEqual(signals[0].error, 'TypeError: undefined is not a function');
  assert.strictEqual(signals[0].context || '', '', 'Filtered errors should not contribute to repeat count');
});

// ---------------------------------------------------------------------------
// Case 12: Signature uses raw error, not truncated error
// ---------------------------------------------------------------------------
test('Case 12: Signature uses raw error, not truncated error', () => {
  const base = 'a'.repeat(300);
  const error1 = base + 'x';
  const error2 = base + 'y';
  const sig1 = recoverySignature('Bash', error1);
  const sig2 = recoverySignature('Bash', error2);
  assert.notStrictEqual(sig1, sig2, 'Signatures must differ because tail character is different in raw text');
});

// ---------------------------------------------------------------------------
// Case 13: Path detector does not eat slash phrases like read/write
// ---------------------------------------------------------------------------
test('Case 13: Path detector does not eat slash phrases like read/write', () => {
  const sig1 = recoverySignature('Bash', 'read/write permission denied');
  const sig2 = recoverySignature('Bash', 'read/execute permission denied');
  assert.notStrictEqual(sig1, sig2, 'read/write and read/execute must not be normalized to <path> and merged');
});

// ---------------------------------------------------------------------------
// Case 14: Relative paths are normalized and merged
// ---------------------------------------------------------------------------
test('Case 14: Relative paths are normalized and merged', () => {
  const errors = [
    { errorText: 'src/foo.rs:1:1 error X', file: 'src/foo.rs' },
    { errorText: 'lib/foo.rs:9:9 error X', file: 'lib/foo.rs' },
  ];
  const messages = makeRecoveryMessages('Bash', errors);
  const signals = extractErrorRecoveries(messages);

  assert.strictEqual(signals.length, 1, 'Should merge relative paths');
  assert.strictEqual(
    signals[0].context.includes('[repeat_count=2 same_session=true files=2]'),
    true,
    'Context should indicate repeat count of 2 and files count of 2'
  );
});

// ---------------------------------------------------------------------------
// Case 15: _rawError must not leak in the output signals
// ---------------------------------------------------------------------------
test('Case 15: _rawError must not leak in the output signals', () => {
  const errors = [
    { errorText: 'File has not been read yet', file: 'src/a.js' },
    { errorText: 'File has not been read yet', file: 'src/b.js' },
  ];
  const messages = makeRecoveryMessages('Bash', errors);
  const signals = extractErrorRecoveries(messages);

  assert.strictEqual(signals.length, 1);
  assert.strictEqual(signals[0]._rawError, undefined, 'Output signal must not contain _rawError');
});

console.log(`\nTests completed: ${passedTests}/${totalTests} passed.`);
if (passedTests === totalTests) {
  console.log('ALL TESTS PASSED!');
  process.exit(0);
} else {
  console.error('SOME TESTS FAILED!');
  process.exit(1);
}
