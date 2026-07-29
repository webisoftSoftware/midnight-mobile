function binaryString(bytes: Uint8Array): string {
  let value = "";
  for (const byte of bytes) value += String.fromCharCode(byte);
  return value;
}

export function encodeMockBase64(bytes: Uint8Array): string {
  return btoa(binaryString(bytes));
}

export function decodeMockBase64(value: string): Uint8Array {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
