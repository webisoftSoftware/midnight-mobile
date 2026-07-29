import { execFileSync, spawnSync } from "node:child_process";

export function listRepositoryFiles() {
  try {
    const output = execFileSync(
      "git",
      ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
      { encoding: "utf8" },
    );

    return output.split("\0").filter(Boolean);
  } catch {
    throw new Error("unable to enumerate repository files with git");
  }
}

export function printNotApplicable() {
  console.log("not applicable: no matching files");
}

export function runCommand(command, argumentsList) {
  const result = spawnSync(command, argumentsList, {
    stdio: "inherit",
  });

  if (result.error) {
    const detail =
      result.error.code === "ENOENT"
        ? `required executable not found: ${command}`
        : `unable to execute ${command}: ${result.error.message}`;
    console.error(detail);
    process.exitCode = 1;
    return false;
  }

  if (result.status !== 0) {
    process.exitCode = result.status ?? 1;
    return false;
  }

  return true;
}
