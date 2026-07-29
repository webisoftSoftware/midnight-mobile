/** @type {import("markdownlint-cli2").Configuration} */
const config = {
  config: {
    default: true,
    MD013: false,
    MD060: false,
  },
  ignores: [
    "node_modules/**",
    "target/**",
    "dist/**",
    "build/**",
    "artifacts/**",
    "coverage/**",
  ],
};

export default config;
