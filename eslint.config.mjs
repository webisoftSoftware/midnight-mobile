import expoConfig from "eslint-config-expo/flat.js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint, { configs as tseslintConfigs } from "typescript-eslint";

const sourceFiles = ["**/*.{js,mjs,cjs,jsx,ts,tsx}"];
const typedFiles = ["**/*.{ts,tsx}"];
const javascriptFiles = ["**/*.{js,mjs,cjs,jsx}"];
const applicationFiles = [
  "packages/**/*.{js,jsx,ts,tsx}",
  "examples/**/*.{js,jsx,ts,tsx}",
];

export default tseslint.config(
  {
    ignores: [
      "**/node_modules/**",
      "**/target/**",
      "**/dist/**",
      "**/build/**",
      "**/artifacts/**",
      "**/coverage/**",
      "**/generated/**",
      "**/*.generated.*",
    ],
  },
  ...expoConfig,
  ...tseslintConfigs.strictTypeChecked.map((config) => ({
    ...config,
    files: typedFiles,
  })),
  ...tseslintConfigs.stylisticTypeChecked.map((config) => ({
    ...config,
    files: typedFiles,
  })),
  {
    files: typedFiles,
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/ban-ts-comment": [
        "error",
        {
          "ts-check": false,
          "ts-expect-error": {
            descriptionFormat: "^: .+ #[0-9]+$",
            minimumDescriptionLength: 10,
          },
          "ts-ignore": true,
          "ts-nocheck": true,
        },
      ],
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { fixStyle: "inline-type-imports", prefer: "type-imports" },
      ],
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/no-unnecessary-condition": "error",
      "@typescript-eslint/no-unsafe-argument": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-call": "error",
      "@typescript-eslint/no-unsafe-member-access": "error",
      "@typescript-eslint/no-unsafe-return": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", caughtErrors: "all" },
      ],
      "@typescript-eslint/switch-exhaustiveness-check": "error",
      "no-non-null-assertion": "off",
      "no-unused-vars": "off",
    },
  },
  {
    files: javascriptFiles,
    ...tseslintConfigs.disableTypeChecked,
    languageOptions: {
      globals: globals.node,
    },
  },
  {
    files: sourceFiles,
    linterOptions: {
      reportUnusedDisableDirectives: "error",
      reportUnusedInlineConfigs: "error",
    },
    rules: {
      complexity: ["error", 15],
      "import/no-relative-packages": "error",
      "max-depth": ["error", 4],
      "max-lines-per-function": [
        "error",
        { max: 80, skipBlankLines: true, skipComments: true },
      ],
      "max-statements-per-line": ["error", { max: 1 }],
    },
    settings: {
      react: {
        version: "19.0",
      },
    },
  },
  {
    files: applicationFiles,
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: reactHooks.configs.flat.recommended.rules,
  },
);
