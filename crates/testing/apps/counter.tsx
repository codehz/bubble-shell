import { mount, signal, Text, View, TapGesture } from "bubble-shell";

function Row(props) {
  return <Text color="#dddddd" fontSize={18}>{props.label}</Text>;
}

function Counter() {
  const count = signal(0);
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={8}>
      <Text testID="count" color="#ffffff" fontSize={24}>Count: {count()}</Text>
      <View testID="increment" backgroundColor={0xff3b82f6} radius={8} padding={[12, 8, 8, 12]}>
        <TapGesture onTap={() => count.update((c) => c + 1)} />
        <Text color="#ffffff" fontSize={16}>Increment</Text>
      </View>
      <View testID="list" overflowY="scroll" flexGrow={1} flexShrink={1}>
        <Row label="Item 1" />
        <Row label="Item 2" />
        <Row label="Item 3" />
        <Row label="Item 4" />
        <Row label="Item 5" />
        <Row label="Item 6" />
        <Row label="Item 7" />
        <Row label="Item 8" />
        <Row label="Item 9" />
        <Row label="Item 10" />
        <Row label="Item 11" />
        <Row label="Item 12" />
      </View>
    </View>
  );
}

mount(Counter);
