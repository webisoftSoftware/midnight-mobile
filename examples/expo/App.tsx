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
});
