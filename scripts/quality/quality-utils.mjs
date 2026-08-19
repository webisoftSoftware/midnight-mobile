import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, rmSync } from "node:fs";

// Removes a build-output tree. Cargo target directories, Xcode build folders,
// and staged consumer node_modules are large enough that a plain recursive
// rmSync intermittently fails with ENOTEMPTY or EBUSY on macOS, particularly
// after an interrupted build left the tree half-written. Node retries those
// transient rmdir failures when maxRetries is set, so use this instead of
// calling rmSync directly on anything a build produced.
export function removeTree(path) {
  rmSync(path, {
    force: true,
    recursive: true,
    maxRetries: 10,
    retryDelay: 100,
  });
}

export function listRepositoryFiles() {
  try {
    const output = execFileSync(
      "git",
      ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
      { encoding: "utf8" },
    );

    return output
      .split("\0")
      .filter((path) => path.length > 0 && existsSync(path));
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
