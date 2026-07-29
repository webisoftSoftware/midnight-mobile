# Generated Swift binding location

`npm run generate:bindings` regenerates the UniFFI Swift source, C header, and
module map here from the sanitized runtime ABI. `npm run check:bindings` repeats
generation twice and rejects drift before comparing these reviewed files
byte-for-byte. Native libraries are not stored in this directory.
