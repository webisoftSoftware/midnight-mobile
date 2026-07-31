/** @type {import("markdownlint-cli2").Configuration} */
const config = {
  config: {
    default: true,
    MD013: false,
    MD060: false,
  },
  // These globs must stay aligned with .gitignore. Generated trees can contain
  // Markdown that the tool would otherwise lint: `expo prebuild` and
  // `expo run:*` write .expo/ and native project directories with their own
  // README files, which are not maintained documentation.
  ignores: [
    "node_modules/**",
    "target/**",
    "dist/**",
    "build/**",
    "artifacts/**",
    "coverage/**",
    "**/.expo/**",
    "examples/expo/android/**",
    "examples/expo/ios/**",
  ],
};

export default config;
