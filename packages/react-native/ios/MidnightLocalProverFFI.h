#pragma once

#include <stddef.h>
#include <stdint.h>

typedef struct MidnightLocalProverParameterDescriptor {
  uint32_t k;
  const uint8_t *_Nonnull bytes;
  size_t bytes_len;
  const uint8_t *_Nonnull sha256;
} MidnightLocalProverParameterDescriptor;

typedef struct MidnightLocalProverCircuitDescriptor {
  const uint8_t *_Nonnull key_location;
  size_t key_location_len;
  const uint8_t *_Nonnull prover_key;
  size_t prover_key_len;
  const uint8_t *_Nonnull prover_key_sha256;
  const uint8_t *_Nonnull verifier_key;
  size_t verifier_key_len;
  const uint8_t *_Nonnull verifier_key_sha256;
  const uint8_t *_Nonnull ir;
  size_t ir_len;
  const uint8_t *_Nonnull ir_sha256;
} MidnightLocalProverCircuitDescriptor;

typedef struct MidnightLocalProverResponse {
  uint8_t *_Nullable bytes;
  size_t bytes_len;
} MidnightLocalProverResponse;

int32_t midnight_local_prover_configure(
    const MidnightLocalProverParameterDescriptor *_Nullable parameters,
    size_t parameter_count,
    const MidnightLocalProverCircuitDescriptor *_Nullable circuits,
    size_t circuit_count,
    uint64_t *_Nonnull output_handle);

int32_t midnight_local_prover_check(
    uint64_t handle,
    const uint8_t *_Nonnull request,
    size_t request_len,
    MidnightLocalProverResponse *_Nonnull output);

int32_t midnight_local_prover_prove(
    uint64_t handle,
    const uint8_t *_Nonnull request,
    size_t request_len,
    MidnightLocalProverResponse *_Nonnull output);

int32_t midnight_local_prover_close(uint64_t handle);

void midnight_local_prover_free(uint8_t *_Nullable bytes, size_t bytes_len);
