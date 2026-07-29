const config = {
  clearMocks: true,
  collectCoverage: true,
  collectCoverageFrom: [
    "packages/**/*.{js,jsx,ts,tsx}",
    "!packages/**/*.d.ts",
    "!packages/**/*.{test,spec}.{js,jsx,ts,tsx}",
    "!packages/**/generated/**",
    "!packages/**/*.generated.*",
    "!packages/**/index.{js,jsx,ts,tsx}",
  ],
  coverageDirectory: "coverage",
  coveragePathIgnorePatterns: [
    "/node_modules/",
    "/examples/",
    "/fixtures?/",
    "/generated/",
  ],
  coverageThreshold: {
    global: {
      branches: 75,
      functions: 85,
      lines: 85,
      statements: 85,
    },
  },
  testMatch: ["**/?(*.)+(spec|test).[jt]s?(x)"],
};

export default config;
