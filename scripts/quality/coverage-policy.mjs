const COVERAGE_POLICY = Object.freeze({
  typescript: Object.freeze({
    runner: "compiled-production-node-tests",
    include: "packages/react-native/.staging-build/src/**/*.js",
    setupImport: "packages/react-native/test-support/register-peer-stubs.mjs",
    minimumPercent: Object.freeze({
      lines: 85,
      branches: 75,
      functions: 85,
    }),
  }),
  rust: Object.freeze({
    runner: "cargo-llvm-cov",
    include: "crates/runtime/src/**/*.rs",
    ignoreFilenameRegex:
      "(^|/)(tools/bindgen|generated|fixtures|examples|tests)(/|$)|(^|/)tests?\\.rs$|\\.cargo/registry",
    minimumPercent: 80,
    baselinePercent: 81.31,
    enforcedPercent: 81.31,
  }),
});

export function loadCoveragePolicy() {
  return COVERAGE_POLICY;
}
