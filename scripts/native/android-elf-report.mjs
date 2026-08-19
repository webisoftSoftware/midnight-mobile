function sameArray(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

export function parseElfReport(header, dynamic, symbols) {
  const field = (name) =>
    new RegExp(`^\\s*${name}:\\s*(.+)$`, "mu").exec(header)?.[1]?.trim() ?? "";
  const needed = [...dynamic.matchAll(/Shared library: \[([^\]]+)\]/gu)]
    .map((match) => match[1])
    .sort();
  const functions = [
    ...symbols.matchAll(
      /\buniffi_midnight_mobile_runtime_fn_func_([a-z0-9_]+)$/gmu,
    ),
  ]
    .map((match) => match[1])
    .sort();
  const localProverFunctions = [
    ...symbols.matchAll(/\b(midnight_mobile_local_prover_[a-z0-9_]+)$/gmu),
  ]
    .map((match) => match[1])
    .sort();
  return {
    type: field("Type"),
    machine: field("Machine"),
    needed,
    functions,
    localProverFunctions,
  };
}

export function validateElfReport(report, target, config) {
  const errors = [];
  if (!report.type.startsWith("DYN ")) {
    errors.push(`${target.abi}: ELF type must be DYN`);
  }
  if (report.machine !== target.machine) {
    errors.push(`${target.abi}: ELF machine must be ${target.machine}`);
  }
  if (!sameArray(report.needed, [...config.android.neededLibraries].sort())) {
    errors.push(`${target.abi}: shared-library dependencies drifted`);
  }
  const functions = [...config.rust.uniffiFunctions].sort();
  if (!sameArray(report.functions, functions)) {
    errors.push(`${target.abi}: exported UniFFI function ABI drifted`);
  }
  if (
    !sameArray(
      report.localProverFunctions,
      [...config.android.localProver.exportedFunctions].sort(),
    )
  ) {
    errors.push(`${target.abi}: exported local prover C ABI drifted`);
  }
  return errors;
}
