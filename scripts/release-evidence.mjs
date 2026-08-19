import { createHash } from "node:crypto";

import { cargoComponents, npmComponents, sha256 } from "./release-policy.mjs";

function componentReference(component) {
  const identity = `${component.ecosystem}:${component.name}@${component.version}`;
  return `${identity}:${sha256(
    `${component.path ?? ""}\0${component.source}`,
  ).slice(0, 16)}`;
}

function packageUrl(component) {
  const name = encodeURIComponent(component.name).replaceAll("%2F", "/");
  return `pkg:${component.ecosystem}/${name}@${encodeURIComponent(
    component.version,
  )}`;
}

function cyclonedxComponent(component) {
  return {
    type: "library",
    "bom-ref": componentReference(component),
    group: component.name.startsWith("@")
      ? component.name.split("/")[0].slice(1)
      : undefined,
    name: component.name.startsWith("@")
      ? component.name.split("/").slice(1).join("/")
      : component.name,
    version: component.version,
    licenses: [{ expression: component.license }],
    purl: packageUrl(component),
    properties: [
      { name: "midnight:ecosystem", value: component.ecosystem },
      { name: "midnight:source", value: component.source },
    ],
  };
}

function removeUndefined(value) {
  if (Array.isArray(value)) return value.map(removeUndefined);
  if (typeof value !== "object" || value === null) return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, entry]) => entry !== undefined)
      .map(([key, entry]) => [key, removeUndefined(entry)]),
  );
}

function stableJson(value) {
  return `${JSON.stringify(removeUndefined(value), null, 2)}\n`;
}

function sbom(config, components, commitTimestamp) {
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.6",
    version: 1,
    metadata: {
      timestamp: commitTimestamp,
      component: {
        type: "library",
        name: config.package.name,
        version: config.package.version,
      },
      licenses: [{ expression: "MIT" }],
    },
    components: components.map(cyclonedxComponent),
  };
}

function licenseReport(config, components) {
  return {
    schemaVersion: 1,
    package: config.package,
    projectLicense: "MIT",
    componentCount: components.length,
    components,
  };
}

function provenance(config, context, subjects) {
  return {
    _type: "https://in-toto.io/Statement/v1",
    subject: subjects.map((entry) => ({
      name: entry.path,
      digest: { sha256: entry.sha256 },
    })),
    predicateType: "https://slsa.dev/provenance/v1",
    predicate: {
      buildDefinition: {
        buildType:
          "https://github.com/ADGLx/midnight-mobile/.github/workflows/release.yml",
        externalParameters: {
          package: config.package,
          compatibility: config.compatibility,
        },
        internalParameters: {
          sourceTreeSha256: context.sourceTreeSha256,
          toolchains: config.toolchains,
        },
        resolvedDependencies: [
          {
            uri: "git+https://github.com/ADGLx/midnight-mobile",
            digest: { gitCommit: context.gitCommit },
          },
          {
            uri: "git+https://github.com/midnightntwrk/midnight-ledger",
            digest: {
              gitCommit: config.compatibility.midnightLedgerRevision,
            },
          },
        ],
      },
      runDetails: {
        builder: {
          id: "https://github.com/ADGLx/midnight-mobile/actions",
        },
        metadata: {
          invocationId: `${context.gitCommit}:${context.sourceTreeSha256}`,
          startedOn: context.commitTimestamp,
          finishedOn: context.commitTimestamp,
        },
      },
    },
  };
}

function evidenceChecksums(artifactEntries, rendered) {
  const generated = Object.entries(rendered).map(([name, contents]) => ({
    path: `artifacts/release/${name}`,
    sha256: sha256(contents),
    size: Buffer.byteLength(contents),
  }));
  return {
    schemaVersion: 1,
    algorithm: "SHA-256",
    entries: [...artifactEntries, ...generated].sort((left, right) =>
      left.path.localeCompare(right.path),
    ),
  };
}

export function renderReleaseEvidence(inputs) {
  const components = [
    ...npmComponents(inputs.npmLockfile),
    ...cargoComponents(inputs.cargoMetadata),
  ];
  const sbomJson = stableJson(
    sbom(inputs.config, components, inputs.context.commitTimestamp),
  );
  const licensesJson = stableJson(licenseReport(inputs.config, components));
  const provenanceJson = stableJson(
    provenance(inputs.config, inputs.context, inputs.artifactEntries),
  );
  const rendered = {
    "sbom.cdx.json": sbomJson,
    "licenses.json": licensesJson,
    "provenance.json": provenanceJson,
  };
  return {
    ...rendered,
    "SHA256SUMS.json": stableJson(
      evidenceChecksums(inputs.artifactEntries, rendered),
    ),
  };
}

export function fingerprintEvidence(rendered) {
  const hash = createHash("sha256");
  for (const [name, contents] of Object.entries(rendered).sort(
    ([left], [right]) => left.localeCompare(right),
  )) {
    hash.update(name);
    hash.update("\0");
    hash.update(contents);
    hash.update("\0");
  }
  return hash.digest("hex");
}
