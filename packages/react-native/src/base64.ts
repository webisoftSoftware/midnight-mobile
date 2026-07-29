import { MidnightRuntimeError } from "./errors.js";

const ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

export function encodeBase64(bytes: Uint8Array): string {
  let result = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index] ?? 0;
    const second = bytes[index + 1] ?? 0;
    const third = bytes[index + 2] ?? 0;
    result += ALPHABET.charAt((first >> 2) & 0x3f);
    result += ALPHABET.charAt(((first << 4) | (second >> 4)) & 0x3f);
    result +=
      index + 1 < bytes.length
        ? ALPHABET.charAt(((second << 2) | (third >> 6)) & 0x3f)
        : "=";
    result += index + 2 < bytes.length ? ALPHABET.charAt(third & 0x3f) : "=";
  }
  return result;
}

function decodeCharacter(value: string, index: number): number {
  if (value.charAt(index) === "=") return 0;
  const decoded = ALPHABET.indexOf(value.charAt(index));
  if (decoded < 0) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return decoded;
}

function decodeQuartet(
  value: string,
  index: number,
): readonly [number, number, number] {
  const a = decodeCharacter(value, index);
  const b = decodeCharacter(value, index + 1);
  const c = decodeCharacter(value, index + 2);
  const d = decodeCharacter(value, index + 3);
  return [
    (a << 2) | (b >> 4),
    ((b & 0x0f) << 4) | (c >> 2),
    ((c & 0x03) << 6) | d,
  ];
}

function writeDecodedBytes(
  output: Uint8Array,
  offset: number,
  bytes: readonly [number, number, number],
): number {
  const remaining = output.length - offset;
  output.set(bytes.slice(0, Math.min(remaining, bytes.length)), offset);
  return offset + Math.min(remaining, bytes.length);
}

export function decodeBase64(value: string): Uint8Array {
  if (
    value.length % 4 !== 0 ||
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
      value,
    )
  ) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  const padding = value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0;
  const output = new Uint8Array((value.length / 4) * 3 - padding);
  let offset = 0;
  for (let index = 0; index < value.length; index += 4) {
    offset = writeDecodedBytes(output, offset, decodeQuartet(value, index));
  }
  if (encodeBase64(output) !== value) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return output;
}
