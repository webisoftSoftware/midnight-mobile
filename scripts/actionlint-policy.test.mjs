import assert from "node:assert/strict";
import test from "node:test";

import { acceptedCurrentPermission } from "./actionlint-policy.mjs";

const message =
  'unknown permission scope "artifact-metadata". all available permission scopes are "actions", "attestations", "checks", "contents", "deployments", "discussions", "id-token", "issues", "packages", "pages", "pull-requests", "repository-projects", "security-events", "statuses"';
const contents = `permissions:
  artifact-metadata: write
uses: actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6
`;

await test("current GitHub artifact permission bridges stale embedded metadata", () => {
  assert.equal(
    acceptedCurrentPermission(".github/workflows/release.yml", contents, {
      line: 2,
      kind: "permissions",
      message,
    }),
    true,
  );
});

await test("the compatibility bridge is exact and fails closed", () => {
  assert.equal(
    acceptedCurrentPermission(".github/workflows/quality.yml", contents, {
      line: 2,
      kind: "permissions",
      message,
    }),
    false,
  );
  assert.equal(
    acceptedCurrentPermission(".github/workflows/release.yml", contents, {
      line: 2,
      kind: "permissions",
      message: "different failure",
    }),
    false,
  );
});
