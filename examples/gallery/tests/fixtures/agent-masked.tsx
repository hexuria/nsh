// A masked input for the GPUI agent's checks: what is typed into it never
// shows, only how long it is.
import { mount, signal, Text, View } from "nsh";
import { Input } from "nsh/base";

function App() {
  const secret = signal("");
  return (
    <View style={{ flex: 1, padding: 12, gap: 8 }}>
      <Input
        testID="agent-secret"
        masked
        value={secret()}
        onChange={({ value: text }) => secret.set(text)}
        style={{ height: 36 }}
      />
      <Text testID="agent-secret-length">{`length:${secret().length}`}</Text>
    </View>
  );
}

mount(App);
