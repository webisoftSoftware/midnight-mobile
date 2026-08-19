import { rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const exampleDirectory = dirname(dirname(fileURLToPath(import.meta.url)));
const stagingDirectory = resolve(exampleDirectory, ".staging-build");

// TypeScript writes test JavaScript into this ignored directory. Remove it
// before compiling so deleted or renamed tests cannot remain in the test glob.
await rm(stagingDirectory, { recursive: true, force: true });
