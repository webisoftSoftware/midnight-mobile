import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../..",
);

export function run(command, argumentsList, options = {}) {
  const result = spawnSync(command, argumentsList, {
    cwd: options.cwd ?? repositoryRoot,
    encoding: options.encoding ?? "utf8",
    env: options.env ?? process.env,
    input: options.input,
    maxBuffer: 64 * 1024 * 1024,
    stdio: options.inherit === true ? "inherit" : "pipe",
  });
  if (result.error !== undefined || result.status !== 0) {
    const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
    throw new Error(
      `${command} failed${detail.length === 0 ? "" : `:\n${detail}`}`,
    );
  }
  return result.stdout;
}

export function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function findPath(root, predicate) {
  if (!existsSync(root)) return undefined;
  for (const entry of readdirSync(root)) {
    const candidate = `${root}/${entry}`;
    const stat = statSync(candidate);
    if (predicate(candidate, stat)) return candidate;
    if (stat.isDirectory()) {
      const nested = findPath(candidate, predicate);
      if (nested !== undefined) return nested;
    }
  }
  return undefined;
}

export function treeBytes(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return stat.size;
  return readdirSync(path).reduce(
    (total, entry) => total + treeBytes(`${path}/${entry}`),
    0,
  );
}
