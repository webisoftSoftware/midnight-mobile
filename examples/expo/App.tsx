import {
  MidnightRuntimeError,
  MidnightRuntimeProvider,
  useMidnightRuntime,
} from "@1am/midnight-mobile";
import { useMemo, useState } from "react";
import {
  Button,
  Platform,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";

import { createMockExampleRuntime } from "./src/example-runtime";
import {
  runMockedWalletLifecycle,
  type MockLifecycleReport,
} from "./src/lifecycle";
import { cleartextEndpoints, readLiveConfig } from "./src/live-config";
import { runLiveProbe } from "./src/live-probe";
import { runLocalProverSmokeTest } from "./src/local-prover-demo";
import { runNativeSmokeTest, type NativeSmokeReport } from "./src/native-smoke";

function StepList({ report }: { readonly report: NativeSmokeReport }) {
  return (
    <View style={styles.section}>
      <Text style={report.passed ? styles.pass : styles.error}>
        {report.passed ? "PASSED" : "FAILED"}
      </Text>
      {report.steps.map((step) => (
        <Text
          key={step.name}
          style={step.ok ? styles.body : styles.error}
        >{`${step.ok ? "✓" : "✗"} ${step.name} — ${step.detail}`}</Text>
      ))}
    </View>
  );
}

function LiveProbeSection() {
  const config = useMemo(() => readLiveConfig(), []);
  const [report, setReport] = useState<NativeSmokeReport | null>(null);
  const [running, setRunning] = useState(false);

  const runProbe = () => {
    if (!config.ok) return;
    setRunning(true);
    setReport(null);
    void runLiveProbe(config.network)
      .then(setReport)
      .finally(() => {
        setRunning(false);
      });
  };

  const cleartext = config.ok ? cleartextEndpoints(config.network) : [];

  return (
    <View style={styles.section}>
      <Text style={styles.heading}>Live preview network</Text>
      {config.ok ? (
        <>
          <Text style={styles.body}>
            Checks endpoint reachability, asks the node to identify itself over
            JSON-RPC, then syncs the shielded and dust streams through
            applySyncBatch. Check and prove are exercised by the native local
            prover section above. Use a disposable wallet only; these seeds are
            synthetic.
          </Text>
          {cleartext.length === 0 ? null : (
            <Text style={styles.note}>
              {`Cleartext endpoints: ${cleartext.join(", ")}. Debug builds already permit cleartext, and a local service also needs a port forward (adb reverse tcp:6300 tcp:6300). Release builds block it.`}
            </Text>
          )}
          <Button
            title={running ? "Probing…" : "Run live probe"}
            disabled={running}
            onPress={runProbe}
          />
          {report === null ? null : <StepList report={report} />}
        </>
      ) : (
        <View style={styles.section}>
          <Text style={styles.body}>
            Not configured. Set these before starting the bundler:
          </Text>
          {config.problems.map((problem) => (
            <Text key={problem} style={styles.error}>
              {problem}
            </Text>
          ))}
        </View>
      )}
    </View>
  );
}

function NativeSmokeSection() {
  const [report, setReport] = useState<NativeSmokeReport | null>(null);
  const [running, setRunning] = useState(false);

  const runSmoke = () => {
    setRunning(true);
    setReport(null);
    void runNativeSmokeTest()
      .then(setReport)
      .finally(() => {
        setRunning(false);
      });
  };

  return (
    <View style={styles.section}>
      <Text style={styles.heading}>Prebuilt native runtime</Text>
      <Text style={styles.body}>
        Runs pure Rust operations against the packaged native library. No
        indexer, proof service, or node is contacted. Requires a development
        build; this fails in Expo Go.
      </Text>
      <Button
        title={running ? "Running…" : "Run native smoke test"}
        disabled={running}
        onPress={runSmoke}
      />
      {report === null ? null : (
        <View testID="native-smoke-report">
          <StepList report={report} />
        </View>
      )}
    </View>
  );
}

function LocalProverSection({
  platform,
}: {
  readonly platform: "ios" | "android";
}) {
  const [report, setReport] = useState<NativeSmokeReport | null>(null);
  const [running, setRunning] = useState(false);

  const runSmoke = () => {
    setRunning(true);
    setReport(null);
    void runLocalProverSmokeTest(platform)
      .then(setReport)
      .finally(() => {
        setRunning(false);
      });
  };

  return (
    <View style={styles.section}>
      <Text style={styles.heading}>Native local prover</Text>
      <Text style={styles.body}>
        Runs the SDK&apos;s real local check and prove operations against the
        verified spend circuit. Prepare artifacts before the development build;
        no proof server is contacted. This fails when the artifacts or native
        module are unavailable.
      </Text>
      <Button
        title={running ? "Running…" : "Run local prover"}
        disabled={running}
        onPress={runSmoke}
      />
      {report === null ? null : (
        <View testID="local-prover-report">
          <StepList report={report} />
        </View>
      )}
    </View>
  );
}

function WalletLifecycleDemo({
  environment,
}: {
  readonly environment: ReturnType<typeof createMockExampleRuntime>;
}) {
  const { controller, error, status } = useMidnightRuntime();
  const [report, setReport] = useState<MockLifecycleReport | null>(null);
  const [running, setRunning] = useState(false);
  const [lifecycleError, setLifecycleError] = useState<string | null>(null);

  const runLifecycle = () => {
    setRunning(true);
    setReport(null);
    setLifecycleError(null);
    void runMockedWalletLifecycle(environment, controller)
      .then(setReport)
      .catch((failure: unknown) => {
        setLifecycleError(
          failure instanceof MidnightRuntimeError
            ? failure.code
            : "UNEXPECTED_EXAMPLE_FAILURE",
        );
      })
      .finally(() => {
        setRunning(false);
      });
  };

  return (
    <View style={styles.screen}>
      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.title}>Midnight Mobile mock lifecycle</Text>
        <Text style={styles.body}>
          Mock services are active. No service URL, credential, wallet key, or
          persistent store is bundled.
        </Text>
        <View style={styles.row}>
          <Text>Status</Text>
          <Text testID="runtime-status">{status}</Text>
        </View>
        <Button
          title={running ? "Running…" : "Run mocked lifecycle"}
          disabled={running}
          onPress={runLifecycle}
        />
        {report === null ? null : (
          <Text testID="lifecycle-report" style={styles.report}>
            {JSON.stringify(report, null, 2)}
          </Text>
        )}
        {error === null ? null : <Text style={styles.error}>{error.code}</Text>}
        {lifecycleError === null ? null : (
          <Text testID="lifecycle-error" style={styles.error}>
            {lifecycleError}
          </Text>
        )}
        <NativeSmokeSection />
        <LocalProverSection platform={environment.platform} />
        <LiveProbeSection />
      </ScrollView>
    </View>
  );
}

export default function App() {
  const platform = Platform.OS === "android" ? "android" : "ios";
  const environment = useMemo(
    () => createMockExampleRuntime(platform),
    [platform],
  );
  return (
    <MidnightRuntimeProvider
      api={environment.api}
      transport={environment.transport}
      checkpointStore={environment.checkpointStore}
    >
      <WalletLifecycleDemo environment={environment} />
    </MidnightRuntimeProvider>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: "#f7f5ef" },
  content: { gap: 18, padding: 24 },
  title: { color: "#161615", fontSize: 28, fontWeight: "700" },
  body: { color: "#484741", fontSize: 16, lineHeight: 24 },
  row: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
  },
  report: {
    backgroundColor: "#e7e2d6",
    borderRadius: 8,
    fontFamily: Platform.select({ ios: "Menlo", android: "monospace" }),
    fontSize: 12,
    padding: 16,
  },
  error: { color: "#a12718", fontWeight: "600" },
  // An advisory, not a failure: cleartext may be expected for local indexers.
  note: { color: "#7a6a2f", fontSize: 14, lineHeight: 20 },
  heading: { color: "#161615", fontSize: 20, fontWeight: "700" },
  pass: { color: "#1d6a3f", fontWeight: "700" },
  section: { gap: 10 },
});
