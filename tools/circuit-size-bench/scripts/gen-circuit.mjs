// Writes a synthetic benchmark contract whose circuit size is set by a hash-round count.
//
// The padding chain is what drives `k`: each round is a fixed number of constraint rows,
// so doubling the rounds adds one to `k`. The seed is a witness so the chain depends on
// secret data and cannot be constant-folded away. Ledger slots are a separate knob for
// public-input count, held at zero for a pure size sweep.
//
// Usage: node scripts/gen-circuit.mjs <rounds> [slots] [output-directory]

import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const [roundsArgument, slotsArgument, directoryArgument] =
  process.argv.slice(2);
const rounds = Number(roundsArgument);
const slots = Number(slotsArgument ?? 0);
const directory = resolve(directoryArgument ?? "src");

if (!Number.isInteger(rounds) || rounds < 1) {
  throw new Error("rounds must be a positive integer");
}
if (!Number.isInteger(slots) || slots < 0) {
  throw new Error("slots must be a non-negative integer");
}

const lines = (count, line) =>
  Array.from({ length: count }, (_, index) => line(index));

const chain = lines(
  rounds,
  (index) =>
    `  const h${String(index + 1)} = persistentHash<Vector<2, Bytes<32>>>(` +
    `[h${String(index)}, h0]);`,
).join("\n");

const slotDeclarations = lines(
  slots,
  (index) => `export ledger slot${String(index)}: Bytes<32>;`,
).join("\n");

const slotWrites = lines(
  slots,
  (index) => `  slot${String(index)} = h${String(rounds)};`,
).join("\n");

const name = `BenchR${String(rounds)}_S${String(slots)}`;
const source = `// GENERATED FILE -- do not edit by hand. Produced by gen-circuit.mjs.
// Synthetic benchmark circuit for local-prover circuit-size measurement.
//   hash rounds : ${String(rounds)}   (drives circuit size k)
//   ledger slots: ${String(slots)}    (drives public input count)

pragma language_version >= 0.23.0;

import CompactStandardLibrary;

// Accumulator for the padding chain. One write, constant across all variants, so it does
// not pollute the public-input count when sweeping rounds.
export ledger acc: Bytes<32>;
${slots > 0 ? `\n${slotDeclarations}\n` : ""}
witness wit_seed(): Bytes<32>;

export circuit run(): [] {
  const h0 = disclose(wit_seed());
${chain}

  acc = h${String(rounds)};
${slots > 0 ? `${slotWrites}\n` : ""}}
`;

mkdirSync(directory, { recursive: true });
const path = resolve(directory, `${name}.compact`);
writeFileSync(path, source);
console.log(JSON.stringify({ name, rounds, slots, path }));
