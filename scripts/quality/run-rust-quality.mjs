import {
  listRepositoryFiles,
  printNotApplicable,
  runCommand,
} from "./quality-utils.mjs";

const files = listRepositoryFiles();
const hasRustWorkstream = files.some(
  (file) =>
    file === "Cargo.toml" ||
    file.endsWith("/Cargo.toml") ||
    file.endsWith(".rs"),
);

if (!hasRustWorkstream) {
  printNotApplicable();
} else if (runCommand("cargo", ["fmt", "--all", "--", "--check"])) {
  const sharedLints = [
    "-D",
    "warnings",
    "-D",
    "clippy::panic",
    "-D",
    "clippy::todo",
    "-D",
    "clippy::unimplemented",
    "-D",
    "clippy::dbg_macro",
    "-D",
    "clippy::print_stdout",
    "-D",
    "clippy::print_stderr",
    "-D",
    "clippy::too_many_lines",
    "-D",
    "clippy::too_many_arguments",
    "-D",
    "unsafe_op_in_unsafe_fn",
  ];

  const allTargetsPassed = runCommand("cargo", [
    "clippy",
    "--workspace",
    "--all-targets",
    "--all-features",
    "--",
    ...sharedLints,
  ]);

  if (allTargetsPassed) {
    runCommand("cargo", [
      "clippy",
      "--workspace",
      "--all-features",
      "--lib",
      "--bins",
      "--examples",
      "--",
      ...sharedLints,
      "-D",
      "clippy::unwrap_used",
      "-D",
      "clippy::expect_used",
    ]);
  }
}
