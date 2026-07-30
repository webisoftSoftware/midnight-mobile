import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";

import {
  APPLE_CONFIGURATION,
  fail,
  FRAMEWORK_BUNDLE,
  FRAMEWORK_NAME,
  run,
  XCODE_BUILD_CONCURRENCY_ARGUMENTS,
} from "./apple-native.mjs";

function parsePlist(path) {
  return JSON.parse(run("plutil", ["-convert", "json", "-o", "-", path]));
}

export function cleanBuildEnvironment(root) {
  const nodeDirectory = dirname(process.execPath);
  const moduleCache = join(root, "module-cache");
  mkdirSync(moduleCache, { recursive: true });
  const environment = {
    ...process.env,
    CLANG_MODULE_CACHE_PATH: moduleCache,
    NODE_BINARY: process.execPath,
    PATH: `${nodeDirectory}:/usr/bin:/bin:/usr/sbin:/sbin`,
    SWIFT_MODULECACHE_PATH: moduleCache,
  };
  for (const name of [
    "CARGO",
    "CARGO_HOME",
    "RUSTC",
    "RUSTDOC",
    "RUSTFLAGS",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
  ]) {
    delete environment[name];
  }
  return environment;
}

export function assertRustUnavailable(environment) {
  for (const executable of ["cargo", "rustc", "rustup"]) {
    const result = spawnSync(executable, ["--version"], {
      encoding: "utf8",
      env: environment,
    });
    if (result.error?.code !== "ENOENT") {
      fail(`${executable} must be unavailable to the clean consumer`);
    }
  }
}

function selectFramework(xcframework, simulator) {
  const metadata = parsePlist(join(xcframework, "Info.plist"));
  const library = metadata.AvailableLibraries.find(
    (entry) => (entry.SupportedPlatformVariant === "simulator") === simulator,
  );
  if (library === undefined) fail("required XCFramework slice is missing");
  return join(xcframework, library.LibraryIdentifier, library.LibraryPath);
}

function validatePodspec(podspec) {
  const source = readFileSync(podspec, "utf8");
  if (
    /\b(?:cargo|rustc|rustup|script_phase|prepare_command)\b/iu.test(source)
  ) {
    fail("podspec must not invoke Rust or consumer build scripts");
  }
  const spec = JSON.parse(run("pod", ["ipc", "spec", podspec]));
  const expectedFramework = `build/${FRAMEWORK_NAME}.xcframework`;
  if (
    spec.platforms?.ios !== APPLE_CONFIGURATION.deploymentTarget ||
    spec.vendored_frameworks !== expectedFramework
  ) {
    fail("podspec deployment target or vendored XCFramework is incorrect");
  }
  const sources = Array.isArray(spec.source_files)
    ? spec.source_files
    : [spec.source_files];
  for (const required of [
    "ExpoMidnightNativeModule.swift",
    `generated/${FRAMEWORK_NAME}.swift`,
  ]) {
    if (!sources.includes(required))
      fail(`podspec source is missing ${required}`);
  }
}

function swiftConsumerSource() {
  return `import ${APPLE_CONFIGURATION.moduleName}

let version = ffi_midnight_native_runtime_uniffi_contract_version()
precondition(version == 30)
`;
}

function compileSwiftConsumers(root, xcframework, environment) {
  const source = join(root, "main.swift");
  const generated = join(dirname(dirname(xcframework)), "generated");
  writeFileSync(source, swiftConsumerSource());
  const variants = [
    {
      name: "simulator",
      sdk: "iphonesimulator",
      target: `arm64-apple-ios${APPLE_CONFIGURATION.deploymentTarget}-simulator`,
      framework: selectFramework(xcframework, true),
    },
    {
      name: "device",
      sdk: "iphoneos",
      target: `arm64-apple-ios${APPLE_CONFIGURATION.deploymentTarget}`,
      framework: selectFramework(xcframework, false),
    },
  ];
  for (const variant of variants) {
    run(
      "xcrun",
      [
        "--sdk",
        variant.sdk,
        "swiftc",
        "-target",
        variant.target,
        "-F",
        dirname(variant.framework),
        "-framework",
        FRAMEWORK_NAME,
        join(generated, `${FRAMEWORK_NAME}.swift`),
        source,
        "-o",
        join(root, `consumer-${variant.name}`),
      ],
      { env: environment },
    );
  }
}

function projectFile(xcframework) {
  const frameworkPath = xcframework.replaceAll('"', '\\"');
  return `// !$*UTF8*$!
{
  archiveVersion = 1;
  classes = {};
  objectVersion = 56;
  objects = {
    A10000000000000000000001 = {isa = PBXBuildFile; fileRef = A10000000000000000000002; };
    A10000000000000000000003 = {isa = PBXBuildFile; fileRef = A10000000000000000000004; };
    A10000000000000000000005 = {isa = PBXBuildFile; fileRef = A10000000000000000000004; settings = {ATTRIBUTES = (CodeSignOnCopy, RemoveHeadersOnCopy, ); }; };
    A10000000000000000000002 = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = main.swift; sourceTree = "<group>"; };
    A10000000000000000000004 = {isa = PBXFileReference; lastKnownFileType = wrapper.xcframework; name = ${FRAMEWORK_NAME}.xcframework; path = "${frameworkPath}"; sourceTree = "<absolute>"; };
    A10000000000000000000006 = {isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = AppleConsumer.app; sourceTree = BUILT_PRODUCTS_DIR; };
    A10000000000000000000007 = {isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = (A10000000000000000000003, ); runOnlyForDeploymentPostprocessing = 0; };
    A10000000000000000000008 = {isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = (A10000000000000000000001, ); runOnlyForDeploymentPostprocessing = 0; };
    A10000000000000000000009 = {isa = PBXCopyFilesBuildPhase; buildActionMask = 2147483647; dstPath = ""; dstSubfolderSpec = 10; files = (A10000000000000000000005, ); name = "Embed Frameworks"; runOnlyForDeploymentPostprocessing = 0; };
    A1000000000000000000000A = {isa = PBXGroup; children = (A10000000000000000000002, A10000000000000000000004, A1000000000000000000000B, ); sourceTree = "<group>"; };
    A1000000000000000000000B = {isa = PBXGroup; children = (A10000000000000000000006, ); name = Products; sourceTree = "<group>"; };
    A1000000000000000000000C = {isa = PBXNativeTarget; buildConfigurationList = A1000000000000000000000D; buildPhases = (A10000000000000000000008, A10000000000000000000007, A10000000000000000000009, ); buildRules = (); dependencies = (); name = AppleConsumer; productName = AppleConsumer; productReference = A10000000000000000000006; productType = "com.apple.product-type.application"; };
    A1000000000000000000000E = {isa = PBXProject; attributes = {BuildIndependentTargetsInParallel = YES; LastUpgradeCheck = 2660; TargetAttributes = {A1000000000000000000000C = {CreatedOnToolsVersion = 26.6; }; }; }; buildConfigurationList = A1000000000000000000000F; compatibilityVersion = "Xcode 14.0"; developmentRegion = en; hasScannedForEncodings = 0; knownRegions = (en, Base, ); mainGroup = A1000000000000000000000A; productRefGroup = A1000000000000000000000B; projectDirPath = ""; projectRoot = ""; targets = (A1000000000000000000000C, ); };
    A10000000000000000000010 = {isa = XCBuildConfiguration; buildSettings = {CODE_SIGN_STYLE = Manual; CURRENT_PROJECT_VERSION = 1; GENERATE_INFOPLIST_FILE = YES; IPHONEOS_DEPLOYMENT_TARGET = ${APPLE_CONFIGURATION.deploymentTarget}; MARKETING_VERSION = 1.0; PRODUCT_BUNDLE_IDENTIFIER = dev.oneam.midnight.consumer; PRODUCT_NAME = "$(TARGET_NAME)"; SDKROOT = iphoneos; SUPPORTED_PLATFORMS = "iphoneos iphonesimulator"; SWIFT_VERSION = 5.0; TARGETED_DEVICE_FAMILY = "1,2"; }; name = Release; };
    A10000000000000000000011 = {isa = XCBuildConfiguration; buildSettings = {CODE_SIGN_STYLE = Manual; CURRENT_PROJECT_VERSION = 1; GENERATE_INFOPLIST_FILE = YES; IPHONEOS_DEPLOYMENT_TARGET = ${APPLE_CONFIGURATION.deploymentTarget}; MARKETING_VERSION = 1.0; PRODUCT_BUNDLE_IDENTIFIER = dev.oneam.midnight.consumer; PRODUCT_NAME = "$(TARGET_NAME)"; SDKROOT = iphoneos; SUPPORTED_PLATFORMS = "iphoneos iphonesimulator"; SWIFT_VERSION = 5.0; TARGETED_DEVICE_FAMILY = "1,2"; }; name = Debug; };
    A10000000000000000000012 = {isa = XCBuildConfiguration; buildSettings = {CLANG_ENABLE_MODULES = YES; }; name = Release; };
    A10000000000000000000013 = {isa = XCBuildConfiguration; buildSettings = {CLANG_ENABLE_MODULES = YES; }; name = Debug; };
    A1000000000000000000000D = {isa = XCConfigurationList; buildConfigurations = (A10000000000000000000011, A10000000000000000000010, ); defaultConfigurationIsVisible = 0; defaultConfigurationName = Release; };
    A1000000000000000000000F = {isa = XCConfigurationList; buildConfigurations = (A10000000000000000000013, A10000000000000000000012, ); defaultConfigurationIsVisible = 0; defaultConfigurationName = Release; };
  };
  rootObject = A1000000000000000000000E;
}
`;
}

function schemeFile() {
  return `<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2660" version="1.7">
  <BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES">
    <BuildActionEntries>
      <BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES">
        <BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="A1000000000000000000000C" BuildableName="AppleConsumer.app" BlueprintName="AppleConsumer" ReferencedContainer="container:AppleConsumer.xcodeproj"/>
      </BuildActionEntry>
    </BuildActionEntries>
  </BuildAction>
  <LaunchAction buildConfiguration="Release"/>
  <ProfileAction buildConfiguration="Release"/>
  <AnalyzeAction buildConfiguration="Release"/>
  <ArchiveAction buildConfiguration="Release" revealArchiveInOrganizer="NO"/>
</Scheme>
`;
}

function createProject(root, xcframework) {
  const project = join(root, "AppleConsumer.xcodeproj");
  mkdirSync(join(project, "xcshareddata/xcschemes"), { recursive: true });
  writeFileSync(
    join(root, "main.swift"),
    swiftConsumerSource().replace(
      APPLE_CONFIGURATION.moduleName,
      FRAMEWORK_NAME,
    ),
  );
  writeFileSync(join(project, "project.pbxproj"), projectFile(xcframework));
  writeFileSync(
    join(project, "xcshareddata/xcschemes/AppleConsumer.xcscheme"),
    schemeFile(),
  );
  return project;
}

function xcodeBuild(project, root, destination, environment, action) {
  const derivedData = join(root, `derived-${action}-${destination}`);
  const argumentsList = [
    "-project",
    project,
    "-scheme",
    "AppleConsumer",
    "-configuration",
    "Release",
    ...XCODE_BUILD_CONCURRENCY_ARGUMENTS,
    "-destination",
    destination,
    "-derivedDataPath",
    derivedData,
    "CODE_SIGNING_ALLOWED=NO",
    "CODE_SIGNING_REQUIRED=NO",
  ];
  if (action === "archive") {
    argumentsList.push(
      "-archivePath",
      join(root, "AppleConsumer.xcarchive"),
      "archive",
    );
  } else {
    argumentsList.push("build");
  }
  run("xcodebuild", argumentsList, { env: environment });
  return { derivedData, archive: join(root, "AppleConsumer.xcarchive") };
}

export function signAndVerify(app, signedRoot, environment) {
  mkdirSync(signedRoot, { recursive: true });
  const signedApp = join(signedRoot, "AppleConsumer.app");
  cpSync(app, signedApp, { recursive: true });
  const framework = join(signedApp, "Frameworks", FRAMEWORK_BUNDLE);
  if (!existsSync(framework)) fail("consumer app did not embed the framework");
  run("codesign", ["--force", "--sign", "-", "--timestamp=none", framework], {
    env: environment,
  });
  run("codesign", ["--force", "--sign", "-", "--timestamp=none", signedApp], {
    env: environment,
  });
  run(
    "codesign",
    ["--verify", "--deep", "--strict", "--verbose=4", signedApp],
    {
      env: environment,
    },
  );
}

export function validateAppleConsumer(xcframework, podspec, root) {
  mkdirSync(root, { recursive: true });
  validatePodspec(podspec);
  const environment = cleanBuildEnvironment(root);
  assertRustUnavailable(environment);
  compileSwiftConsumers(root, xcframework, environment);
  const project = createProject(root, xcframework);
  const simulator = xcodeBuild(
    project,
    root,
    "generic/platform=iOS Simulator",
    environment,
    "simulator",
  );
  const simulatorApp = join(
    simulator.derivedData,
    "Build/Products/Release-iphonesimulator/AppleConsumer.app",
  );
  signAndVerify(simulatorApp, join(root, "signed-simulator"), environment);
  xcodeBuild(project, root, "generic/platform=iOS", environment, "device");
  const archive = xcodeBuild(
    project,
    root,
    "generic/platform=iOS",
    environment,
    "archive",
  ).archive;
  const archivedApp = join(archive, "Products/Applications/AppleConsumer.app");
  if (!existsSync(archivedApp))
    fail("xcodebuild produced no application archive");
  signAndVerify(archivedApp, join(root, "signed-archive"), environment);
  return {
    archive,
    signing: "ad-hoc strict verification",
    rustPath: "unavailable",
    swiftImports: APPLE_CONFIGURATION.moduleName,
  };
}
