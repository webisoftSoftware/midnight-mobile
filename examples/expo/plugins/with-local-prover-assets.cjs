const fs = require("node:fs");
const path = require("node:path");

const {
  withAppBuildGradle,
  withDangerousMod,
  withXcodeProject,
} = require("@expo/config-plugins");

const SOURCE_DIRECTORY = "assets/local-prover";
const IOS_RESOURCE_DIRECTORY = "LocalProverArtifacts";

function filesUnder(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const source = path.join(directory, entry.name);
    return entry.isDirectory() ? filesUnder(source) : [source];
  });
}

function sourceFiles(projectRoot) {
  return filesUnder(path.resolve(projectRoot, SOURCE_DIRECTORY));
}

function projectName(iosRoot) {
  const project = fs
    .readdirSync(iosRoot)
    .find((entry) => entry.endsWith(".xcodeproj"));
  return project === undefined ? undefined : project.slice(0, -10);
}

function withAndroidAssets(config) {
  const withAssets = withDangerousMod(config, [
    "android",
    async (current) => {
      const sourceRoot = path.resolve(
        current.modRequest.projectRoot,
        SOURCE_DIRECTORY,
      );
      if (!fs.existsSync(sourceRoot)) return current;
      const destination = path.resolve(
        current.modRequest.projectRoot,
        "android/app/src/main/assets/local-prover",
      );
      fs.rmSync(destination, { recursive: true, force: true });
      fs.cpSync(sourceRoot, destination, { recursive: true });
      return current;
    },
  ]);
  return withAppBuildGradle(withAssets, (current) => {
    const marker = 'noCompress "bls_midnight_2p15"';
    if (current.modResults.contents.includes(marker)) return current;
    current.modResults.contents = current.modResults.contents.replace(
      /androidResources\s*\{\n/u,
      'androidResources {\n        noCompress "", "bls_midnight_2p15", "prover", "verifier", "bzkir"\n',
    );
    return current;
  });
}

function withIosAssets(config) {
  return withDangerousMod(config, [
    "ios",
    async (current) => {
      const sourceRoot = path.resolve(
        current.modRequest.projectRoot,
        SOURCE_DIRECTORY,
      );
      const iosRoot = path.resolve(current.modRequest.projectRoot, "ios");
      const name = projectName(iosRoot);
      if (!fs.existsSync(sourceRoot) || name === undefined) return current;
      const destination = path.resolve(iosRoot, name, IOS_RESOURCE_DIRECTORY);
      fs.rmSync(destination, { recursive: true, force: true });
      fs.cpSync(sourceRoot, destination, { recursive: true });
      return current;
    },
  ]);
}

function addIosResources(config) {
  return withXcodeProject(config, (current) => {
    const files = sourceFiles(current.modRequest.projectRoot);
    if (files.length === 0) return current;
    const firstTarget = current.modResults.getFirstTarget();
    const iosRoot = path.resolve(current.modRequest.projectRoot, "ios");
    const name = projectName(iosRoot);
    if (name === undefined) return current;
    const appGroup = current.modResults.findPBXGroupKey({ name });
    if (appGroup === undefined) return current;
    // A folder reference preserves zswap/9/... inside the app bundle. Adding
    // individual files to Copy Bundle Resources can flatten nested paths.
    const resource = current.modResults.addFile(
      path.join(name, IOS_RESOURCE_DIRECTORY),
      appGroup,
      { lastKnownFileType: "folder" },
    );
    if (resource === null) return current;
    resource.uuid = current.modResults.generateUuid();
    resource.target = firstTarget.uuid;
    current.modResults.addToPbxBuildFileSection(resource);
    current.modResults.addToPbxResourcesBuildPhase(resource);
    return current;
  });
}

module.exports = function withLocalProverAssets(config) {
  return addIosResources(withIosAssets(withAndroidAssets(config)));
};
