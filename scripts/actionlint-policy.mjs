export function acceptedCurrentPermission(path, contents, result) {
  const line = contents.split(/\r?\n/u)[result.line - 1]?.trim();
  return (
    path === ".github/workflows/release.yml" &&
    result.kind === "permissions" &&
    result.message ===
      'unknown permission scope "artifact-metadata". all available permission scopes are "actions", "attestations", "checks", "contents", "deployments", "discussions", "id-token", "issues", "packages", "pages", "pull-requests", "repository-projects", "security-events", "statuses"' &&
    line === "artifact-metadata: write" &&
    contents.includes(
      "uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6",
    )
  );
}
