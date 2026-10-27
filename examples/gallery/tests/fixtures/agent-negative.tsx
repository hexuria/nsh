import { Scroll, mount, signal, Text, View } from "nsh";
import { Input } from "nsh/base";

function App() {
  const presses = signal(0);
  const value = signal("");
  const revealAttempts = signal(0);
  const hoverIns = signal(0);
  const hoverOuts = signal(0);

  return (
    <View style={{ flex: 1, padding: 12, gap: 8 }}>
      <Input
        testID="agent-input"
        value={value()}
        onChange={({ value: text }) => value.set(text)}
        style={{ height: 36 }}
      />
      <Text testID="agent-state">
        {`presses:${presses()} value:${value()} scrolls:${revealAttempts()}`}
      </Text>
      <Text testID="agent-hover-state">
        {`hoverIn:${hoverIns()} hoverOut:${hoverOuts()}`}
      </Text>
      <View style={{ height: 64, position: "relative" }}>
        <View
          testID="covered-target"
          onPress={() => presses.set(presses() + 1)}
          style={{ position: "absolute", left: 0, top: 0, width: 220, height: 56 }}
        >
          <Text>covered target</Text>
        </View>
        <View
          testID="cover"
          onPress={() => {}}
          onMouseEnter={() => hoverIns.set(hoverIns() + 1)}
          onMouseLeave={() => hoverOuts.set(hoverOuts() + 1)}
          style={{ position: "absolute", left: 0, top: 0, width: 220, height: 56 }}
        >
          <Text>cover</Text>
        </View>
      </View>
      <View
        testID="disabled-target"
        disabled
        onPress={() => presses.set(presses() + 1)}
        style={{ height: 40 }}
      >
        <Text>disabled target</Text>
      </View>
      <Scroll
        testID="reveal-scroll"
        style={{ height: 72 }}
        contentStyle={{ height: 100 }}
        scrollbar={false}
        onScroll={() => revealAttempts.set(revealAttempts() + 1)}
      >
        <View
          testID="scroll-clip"
          style={{ height: 100, overflow: "hidden", position: "relative" }}
        >
          <View
            testID="unreachable-target"
            onPress={() => presses.set(presses() + 1)}
            style={{ position: "absolute", left: 0, top: 500, width: 220, height: 40 }}
          >
            <Text>unreachable target</Text>
          </View>
        </View>
      </Scroll>
    </View>
  );
}

mount(App);
