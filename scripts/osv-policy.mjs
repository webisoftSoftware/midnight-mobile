function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactBraceFinding(package_, vulnerability, policy) {
  const advisory = policy.braceExpansionAdvisory;
  const identities = new Set([
    vulnerability.id,
    ...(Array.isArray(vulnerability.aliases) ? vulnerability.aliases : []),
  ]);
  return (
    package_.name === "brace-expansion" &&
    package_.ecosystem === "npm" &&
    advisory.acceptedVersions.includes(package_.version) &&
    identities.size === 2 &&
    identities.has(advisory.advisory) &&
    identities.has(advisory.cve)
  );
}

export function validateOsvReport(
  report,
  policy,
  dependencyVerificationErrors = [],
) {
  const errors = [...dependencyVerificationErrors];
  if (!isObject(report) || !Array.isArray(report.results)) {
    return [...errors, "OSV report must contain a results array"];
  }
  for (const result of report.results) {
    if (!isObject(result) || !Array.isArray(result.packages)) {
      errors.push("OSV result packages must be an array");
      continue;
    }
    for (const affected of result.packages) {
      if (
        !isObject(affected) ||
        !isObject(affected.package) ||
        !Array.isArray(affected.vulnerabilities)
      ) {
        errors.push("OSV affected package entry is invalid");
        continue;
      }
      for (const vulnerability of affected.vulnerabilities) {
        if (
          !isObject(vulnerability) ||
          !exactBraceFinding(affected.package, vulnerability, policy)
        ) {
          errors.push(
            `${String(affected.package.name)}@${String(
              affected.package.version,
            )}: unapproved OSV finding ${String(vulnerability?.id)}`,
          );
        }
      }
    }
  }
  return [...new Set(errors)];
}
