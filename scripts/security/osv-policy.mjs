function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function validateOsvReport(report, dependencyVerificationErrors = []) {
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
        if (!isObject(vulnerability)) {
          errors.push("OSV vulnerability entry is invalid");
        } else {
          errors.push(
            `${String(affected.package.name)}@${String(
              affected.package.version,
            )}: OSV finding ${String(vulnerability.id)}`,
          );
        }
      }
    }
  }
  return [...new Set(errors)];
}
