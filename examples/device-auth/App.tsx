import { deviceAuth, deviceAuthAvailable, effect, lifecycle, mount, signal, Text, View } from "nsh";

function App() {
  const status = signal("starting");
  const run = async () => {
    const available = deviceAuthAvailable();
    console.log(`NSHDEVAUTH available ${available}`);
    status.set(`available: ${available}`);
    const result = await deviceAuth("Unlock the example", false);
    console.log(`NSHDEVAUTH result ${result}`);
    status.set(`available: ${available}; result: ${result}`);
  };
  let wasActive = false;
  effect(() => {
    const active = lifecycle() === "active";
    if (active && !wasActive) void run();
    wasActive = active;
  });
  return (
    <View style={{ flex: 1, padding: 32, paddingTop: 80, backgroundColor: "#ffffff" }}>
      <Text style={{ fontSize: 22, fontWeight: "bold" }}>Device authentication</Text>
      <Text style={{ marginTop: 20, fontSize: 16 }}>{status()}</Text>
    </View>
  );
}

mount(App);
