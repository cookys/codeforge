// Mirror of tests/fixtures/session-id-vectors.json (the mod cannot import files outside its folder or
// non-code files). src/live.rs test `forge_stats_vectors_mirror_the_fixture` keeps the two equal.
export const VECTORS: { input: string; expected: string }[] = [
  { "input": "93196c52-25cb-47ca-821c-cec391832eed", "expected": "93196c52-25cb-47ca-821c-cec391832eed" },
  { "input": "a/b:c d", "expected": "a_b_c_d" },
  { "input": "會議123", "expected": "__123" },
  { "input": "😀🎉session", "expected": "__session" },
  { "input": "", "expected": "unknown" },
  { "input": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "expected": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
]
