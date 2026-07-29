const config = {
  clearMocks: true,
  collectCoverage: true,
  collectCoverageFrom: [
    "packages/**/*.{js,jsx,ts,tsx}",
    "!packages/**/*.d.ts",
    "!packages/**/*.{test,spec}.{js,jsx,ts,tsx}",
    "!packages/react-native/**",
    "!packages/**/generated/**",
    "!packages/**/*.generated.*",
    "!packages/**/index.{js,jsx,ts,tsx}",
  ],
  coverageDirectory: "coverage",
  coveragePathIgnorePatterns: [
    "/node_modules/",
    "/\\.staging-build/",
    "/examples/",
    "/fixtures?/",
    "/generated/",
  ],
  modulePathIgnorePatterns: ["/\\.staging-build/"],
  coverageThreshold: {
    global: {
      branches: 75,
      functions: 85,
      lines: 85,
      statements: 85,
    },
  },
  testMatch: ["**/?(*.)+(spec|test).[jt]s?(x)"],
  testPathIgnorePatterns: [
    "/node_modules/",
    "/\\.staging-build/",
    "/packages/react-native/",
  ],
};

export default config;
