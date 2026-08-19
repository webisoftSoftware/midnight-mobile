const REVIEWED_ANDROID_ABIS = new Set(["arm64-v8a", "x86_64"]);

export function createAndroidBuildPlan(config, argumentsList = []) {
  let singlePass = false;
  let abi;
  for (let index = 0; index < argumentsList.length; index += 1) {
    const argument = argumentsList[index];
    if (argument === "--single-pass") {
      if (singlePass) {
        throw new Error("--single-pass may be provided only once");
      }
      singlePass = true;
      continue;
    }
    if (argument === "--abi") {
      if (abi !== undefined) throw new Error("--abi may be provided only once");
      abi = argumentsList[index + 1];
      if (abi === undefined || abi.startsWith("--")) {
        throw new Error("--abi requires an Android ABI");
      }
      if (!REVIEWED_ANDROID_ABIS.has(abi)) {
        throw new Error(`unsupported Android ABI: ${abi}`);
      }
      index += 1;
      continue;
    }
    throw new Error(`unsupported argument: ${argument}`);
  }
  if (abi !== undefined && !singlePass) {
    throw new Error(
      "--abi is available only with explicit --single-pass consumer builds",
    );
  }
  const targets = config.android.targets.filter(
    (target) => abi === undefined || target.abi === abi,
  );
  if (targets.length === 0) {
    throw new Error("Android build selected no targets");
  }
  return { targets, verifyReproducible: !singlePass };
}
