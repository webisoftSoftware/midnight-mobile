import { existsSync, readFileSync } from "node:fs";
import { extname } from "node:path";

import { listRepositoryFiles } from "./quality-utils.mjs";

const MAX_PHYSICAL_LINES = 500;
// Rust gets a higher ceiling than the other languages. At 500 lines the runtime
// was forced into statement-level `include!("...")` splices to stay compliant,
// which is not real module structure: the included fragments are not valid Rust
// on their own and confuse rustfmt, clippy spans, and IDE navigation. The
// ceiling exists to stop files becoming unreviewable, not to push code into text
// substitution. Unwinding the existing splices into ordinary `mod` items is
// tracked separately; this limit is the precondition for that work.
const MAX_PHYSICAL_LINES_RUST = 800;

function maxPhysicalLines(file) {
  return extname(file) === ".rs" ? MAX_PHYSICAL_LINES_RUST : MAX_PHYSICAL_LINES;
}
const EXCEPTIONS_PATH = "scripts/source-policy-exceptions.json";
const MILESTONES = ["M0", "M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9"];
const SOURCE_EXTENSIONS = new Set([
  ".gradle",
  ".js",
  ".kt",
  ".mjs",
  ".rs",
  ".swift",
  ".ts",
  ".tsx",
]);
const issuePattern =
  /(?:#[1-9][0-9]*|https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/issues\/[1-9][0-9]*)/u;
const globPattern = /[*?[\]{}!]/u;

function addError(errors, file, message, line) {
  const location = line === undefined ? file : `${file}:${line}`;
  errors.push(`${location}: ${message}`);
}

function countPhysicalLines(contents) {
  if (contents.length === 0) {
    return 0;
  }

  const lines = contents.split(/\r\n|\n|\r/u);
  return contents.endsWith("\n") || contents.endsWith("\r")
    ? lines.length - 1
    : lines.length;
}

function isMaintainedSource(file) {
  return SOURCE_EXTENSIONS.has(extname(file)) || file.endsWith(".gradle.kts");
}

function parseExceptionPolicy(errors) {
  if (!existsSync(EXCEPTIONS_PATH)) {
    addError(
      errors,
      EXCEPTIONS_PATH,
      "required source-policy configuration is missing",
    );
    return { currentMilestone: "M0", exceptions: [] };
  }

  try {
    const policy = JSON.parse(readFileSync(EXCEPTIONS_PATH, "utf8"));
    if (
      typeof policy !== "object" ||
      policy === null ||
      !Array.isArray(policy.exceptions) ||
      typeof policy.currentMilestone !== "string"
    ) {
      throw new Error("expected currentMilestone and exceptions fields");
    }
    return policy;
  } catch (error) {
    const detail =
      error instanceof Error ? error.message : "unknown parse error";
    addError(errors, EXCEPTIONS_PATH, `invalid JSON policy: ${detail}`);
    return { currentMilestone: "M0", exceptions: [] };
  }
}

function validateExceptionPath(path, label, files, errors) {
  if (typeof path !== "string" || path.length === 0 || globPattern.test(path)) {
    addError(errors, label, "path must be an exact repository-relative path");
  } else if (!files.has(path)) {
    addError(errors, label, `stale exception path does not exist: ${path}`);
  }
}

function validateExceptionMetadata(entry, label, errors) {
  const { rule, justification, trackingIssue } = entry;
  if (rule !== "max-physical-lines") {
    addError(errors, label, "rule must be max-physical-lines");
  }
  if (typeof justification !== "string" || justification.trim().length < 20) {
    addError(
      errors,
      label,
      "justification must be concrete and at least 20 characters",
    );
  }
  if (typeof trackingIssue !== "string" || !issuePattern.test(trackingIssue)) {
    addError(
      errors,
      label,
      "trackingIssue must be a GitHub issue URL or #number",
    );
  }
}

function validateExceptionExpiry(entry, policy, label, errors) {
  const { expiryMilestone, removalCondition } = entry;
  const hasExpiry = typeof expiryMilestone === "string";
  const hasCondition =
    typeof removalCondition === "string" &&
    removalCondition.trim().length >= 10;
  if (!hasExpiry && !hasCondition) {
    addError(errors, label, "expiryMilestone or removalCondition is required");
  }
  if (hasExpiry) {
    const currentIndex = MILESTONES.indexOf(policy.currentMilestone);
    const expiryIndex = MILESTONES.indexOf(expiryMilestone);
    if (currentIndex === -1 || expiryIndex === -1) {
      addError(errors, label, "milestones must be one of M0 through M9");
    } else if (expiryIndex < currentIndex) {
      addError(errors, label, `exception expired at ${expiryMilestone}`);
    }
  }
}

function validateException(entry, index, policy, files, errors) {
  const label = `${EXCEPTIONS_PATH}#exceptions[${index}]`;
  if (typeof entry !== "object" || entry === null) {
    addError(errors, label, "exception must be an object");
    return undefined;
  }

  const { path, rule } = entry;
  validateExceptionPath(path, label, files, errors);
  validateExceptionMetadata(entry, label, errors);
  validateExceptionExpiry(entry, policy, label, errors);
  return typeof path === "string" && rule === "max-physical-lines"
    ? `${path}:${rule}`
    : undefined;
}

function validateExceptions(policy, repositoryFiles, errors) {
  const keys = new Set();
  policy.exceptions.forEach((entry, index) => {
    const key = validateException(
      entry,
      index,
      policy,
      repositoryFiles,
      errors,
    );
    if (key !== undefined) {
      if (keys.has(key)) {
        addError(errors, EXCEPTIONS_PATH, `duplicate exception: ${key}`);
      }
      keys.add(key);
    }
  });
  return keys;
}

function hasQualifiedDirective(line, directive) {
  const directiveIndex = line.indexOf(directive);
  if (directiveIndex === -1) {
    return true;
  }
  const explanation = line.slice(directiveIndex + directive.length);
  return (
    explanation.includes("--") &&
    explanation.trim().length >= 12 &&
    issuePattern.test(line)
  );
}

function checkSuppressionPolicy(file, contents, errors) {
  const lines = contents.split(/\r\n|\n|\r/u);
  const tsIgnore = "@ts-" + "ignore";
  const tsExpectError = "@ts-" + "expect-error";
  const eslintDisable = "eslint-" + "disable";
  const narrowEslintDisable = new RegExp(
    eslintDisable + "-(?:next-)?line",
    "u",
  );
  const debtMarker = new RegExp("\\b(?:" + "TO" + "DO|FIX" + "ME)\\b", "u");

  lines.forEach((line, index) => {
    const lineNumber = index + 1;
    if (line.includes(tsIgnore)) {
      addError(errors, file, `${tsIgnore} is forbidden`, lineNumber);
    }
    if (
      line.includes(tsExpectError) &&
      !hasQualifiedDirective(line, tsExpectError)
    ) {
      addError(
        errors,
        file,
        `${tsExpectError} requires an explanation and tracking issue`,
        lineNumber,
      );
    }
    if (line.includes(eslintDisable)) {
      const isNarrow = narrowEslintDisable.test(line);
      if (!isNarrow || !hasQualifiedDirective(line, eslintDisable)) {
        addError(
          errors,
          file,
          "ESLint suppression must be line-scoped with an explanation and tracking issue",
          lineNumber,
        );
      }
    }
    if (debtMarker.test(line) && !issuePattern.test(line)) {
      addError(
        errors,
        file,
        "debt marker requires a GitHub issue reference",
        lineNumber,
      );
    }
    if (file.endsWith(".rs") && /#!\s*\[\s*allow\s*\(/u.test(line)) {
      addError(
        errors,
        file,
        "crate-wide Rust allow attributes are forbidden",
        lineNumber,
      );
    }
  });
}

function checkUnsafeBlocks(file, contents, errors) {
  if (!file.endsWith(".rs")) {
    return;
  }

  const lines = contents.split(/\r\n|\n|\r/u);
  lines.forEach((line, index) => {
    if (!/\bunsafe\s*\{/u.test(line)) {
      return;
    }
    const previousLine = index === 0 ? "" : (lines[index - 1] ?? "");
    if (!/^\s*\/\/\s*SAFETY:\s+\S/u.test(previousLine)) {
      addError(
        errors,
        file,
        "unsafe block requires a SAFETY: invariant immediately above it",
        index + 1,
      );
    }
  });
}

function checkSources(files, exceptionKeys, errors) {
  const usedExceptions = new Set();
  files.filter(isMaintainedSource).forEach((file) => {
    const contents = readFileSync(file, "utf8");
    const lines = countPhysicalLines(contents);
    const exceptionKey = `${file}:max-physical-lines`;
    const maximum = maxPhysicalLines(file);
    if (lines > maximum) {
      if (exceptionKeys.has(exceptionKey)) {
        usedExceptions.add(exceptionKey);
      } else {
        addError(
          errors,
          file,
          `has ${lines} physical lines; maximum is ${maximum}`,
        );
      }
    }
    checkSuppressionPolicy(file, contents, errors);
    checkUnsafeBlocks(file, contents, errors);
  });

  exceptionKeys.forEach((key) => {
    if (!usedExceptions.has(key)) {
      addError(
        errors,
        EXCEPTIONS_PATH,
        `exception does not waive a current violation: ${key}`,
      );
    }
  });
}

const errors = [];
const repositoryFiles = new Set(listRepositoryFiles());
const policy = parseExceptionPolicy(errors);
const exceptionKeys = validateExceptions(policy, repositoryFiles, errors);
checkSources([...repositoryFiles], exceptionKeys, errors);

if (errors.length > 0) {
  errors.sort().forEach((error) => console.error(error));
  process.exitCode = 1;
} else {
  console.log("source policy passed");
}
