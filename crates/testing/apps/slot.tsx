import { signal, Text, View, For } from "bubble-shell";

// Container component: children render where the slot expression sits, not
// where <Card> is written. Header is a named slot; the fallback covers both a
// missing header prop and a missing children slot.
function Card(props) {
  return (
    <View backgroundColor="#1b1e27" padding={[12, 12, 12, 12]} gap={8} flexGrow={1} flexShrink={1}>
      <Slot fallback={<Text color="#666677" fontSize={12}>no header</Text>}>{props.header}</Slot>
      {props.children}
      <Text color="#8892a0" fontSize={12}>tail</Text>
    </View>
  );
}

// Forwarding chain: this component's children land inside Card's slot.
function Panel(props) {
  return <Card>{props.children}</Card>;
}

function App() {
  const items = signal(["one", "two"]);
  return (
    <View backgroundColor="#0d0f14" padding={[16, 16, 16, 16]} gap={12} flexGrow={1} flexShrink={1}>
      <Card header={<Text color="#f87171" fontSize={14} fontWeight={700}>head</Text>}>
        <Text color="#ffffff" fontSize={14}>body</Text>
      </Card>
      <Card>
        <For each={items()} by={(item) => item}>
          {(read) => <Text color="#a3e635" fontSize={14}>item {read()}</Text>}
        </For>
      </Card>
      <Panel>
        <Text color="#60a5fa" fontSize={14}>chained</Text>
      </Panel>
    </View>
  );
}

export default App;
