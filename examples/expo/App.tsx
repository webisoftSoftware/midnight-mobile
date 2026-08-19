import {
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
import { DEMO_WALLET } from "./src/demo-wallet";
import { runSdkExample, type SdkExampleReport } from "./src/sdk-tour";

function ExampleReport({ report }: { readonly report: SdkExampleReport }) {
  return (
    <View style={styles.report} testID="sdk-example-report">
      <Text style={report.passed ? styles.passed : styles.failed}>
        {report.passed ? "Example complete" : "Example needs attention"}
      </Text>
      {report.steps.map((step) => (
        <View key={step.id} style={styles.step} testID={`sdk-step-${step.id}`}>
          <Text style={step.passed ? styles.passed : styles.failed}>
            {step.passed ? "✓" : "✗"} {step.label}
          </Text>
          <Text style={styles.detail}>{step.detail}</Text>
        </View>
      ))}
    </View>
  );
}

function WalletExample({
  environment,
}: {
  readonly environment: ReturnType<typeof createMockExampleRuntime>;
}) {
  const { controller, error, status } = useMidnightRuntime();
  const [report, setReport] = useState<SdkExampleReport | null>(null);
  const [running, setRunning] = useState(false);

  const runExample = () => {
    setRunning(true);
    setReport(null);
    void runSdkExample(environment, environment.platform, controller)
      .then(setReport)
      .finally(() => {
        setRunning(false);
      });
  };

  return (
    <View style={styles.screen}>
      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.eyebrow}>MIDNIGHT MOBILE</Text>
        <Text style={styles.title}>SDK example</Text>
        <Text style={styles.body}>
          One deterministic tour of the wallet runtime, host boundary, native
          runtime, and local prover.
        </Text>

        <View style={styles.walletCard}>
          <Text style={styles.cardLabel}>PUBLIC TEST WALLET</Text>
          <Text selectable style={styles.address}>
            {DEMO_WALLET.unshieldedAddress}
          </Text>
          <Text style={styles.note}>
            Synthetic keys. Never fund or reuse this wallet. No mnemonic is
            included; applications provide their own key-management layer.
          </Text>
        </View>

        <View style={styles.statusRow}>
          <Text style={styles.statusLabel}>Provider status</Text>
          <Text testID="runtime-status" style={styles.statusValue}>
            {status}
          </Text>
        </View>

        <Button
          title={running ? "Running example…" : "Run SDK example"}
          disabled={running}
          onPress={runExample}
        />

        {report === null ? (
          <Text style={styles.hint}>
            The host flow uses deterministic mock services. Native runtime and
            prover steps require a development build with staged artifacts.
          </Text>
        ) : (
          <ExampleReport report={report} />
        )}
        {error === null ? null : (
          <Text style={styles.failed}>{error.code}</Text>
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
      <WalletExample environment={environment} />
    </MidnightRuntimeProvider>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: "#f7f5ef" },
  content: { gap: 18, padding: 24 },
  eyebrow: {
    color: "#7a6a2f",
    fontSize: 12,
    fontWeight: "700",
    letterSpacing: 1.4,
  },
  title: { color: "#161615", fontSize: 32, fontWeight: "700" },
  body: { color: "#484741", fontSize: 16, lineHeight: 24 },
  walletCard: {
    backgroundColor: "#e7e2d6",
    borderRadius: 12,
    gap: 8,
    padding: 16,
  },
  cardLabel: {
    color: "#7a6a2f",
    fontSize: 12,
    fontWeight: "700",
    letterSpacing: 1,
  },
  address: {
    color: "#161615",
    fontFamily: Platform.select({ ios: "Menlo", android: "monospace" }),
    fontSize: 14,
  },
  note: { color: "#665d45", fontSize: 13, lineHeight: 19 },
  statusRow: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
  },
  statusLabel: { color: "#665d45", fontSize: 14 },
  statusValue: { color: "#161615", fontSize: 14, fontWeight: "700" },
  hint: { color: "#665d45", fontSize: 14, lineHeight: 20 },
  report: {
    backgroundColor: "#fffdf8",
    borderColor: "#ded8cb",
    borderRadius: 12,
    borderWidth: 1,
    gap: 14,
    padding: 16,
  },
  step: { gap: 4 },
  passed: { color: "#1d6a3f", fontWeight: "700" },
  failed: { color: "#a12718", fontWeight: "700" },
  detail: { color: "#484741", fontSize: 13, lineHeight: 19 },
});
