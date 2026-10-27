// An inline web view beside a native field, for the GPUI agent's checks:
// typing into the native field while the page holds the Win32 focus must
// land in the field, not in the page.
import { mount, signal, Text, View, WebView } from "nsh";
import { Input } from "nsh/base";

const page = `<!doctype html><html><body style="margin:0;padding:12px;font-family:system-ui,sans-serif">
<input id="web" placeholder="web input" style="width:90%;height:28px;font-size:15px">
<script>
  const web = document.getElementById("web");
  web.addEventListener("input", () => window.nsh.postMessage(web.value));
</script>
</body></html>`;

function App() {
  const native = signal("");
  const web = signal("");
  return (
    <View style={{ flex: 1, padding: 12, gap: 8 }}>
      <WebView testID="agent-web" html={page} style={{ height: 80 }} onMessage={(e) => web.set(e.data)} />
      <Input
        testID="agent-native"
        value={native()}
        onChange={({ value: text }) => native.set(text)}
        style={{ height: 36 }}
      />
      <Text testID="agent-webview-state">{`native:${native()} web:${web()}`}</Text>
    </View>
  );
}

mount(App);
