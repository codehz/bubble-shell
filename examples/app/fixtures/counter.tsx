import { mount, signal, Text, View, TapGesture } from "bubble-shell";

function Row(props) {
  return <Text color="#dddddd" fontSize={18}>{props.label}</Text>;
}

function Counter() {
  const count = signal(0);
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={8}>
      <Text color="#ffffff" fontSize={24} lineHeight={30} textAlign="center" fontWeight={700}><StateStyle when="pressed" color="#f87171" />Count: {count()}</Text>
      <View backgroundColor={0xff3b82f6} radius={8} padding={[12, 8, 8, 12]}>
        <StateStyle when="pressed" opacity={0.7} />
        <TapGesture onTap={() => count.update((c) => c + 1)} />
        <Text color="#ffffff" fontSize={16}>Increment</Text>
      </View>
      <View overflowY="scroll" flexGrow={1} flexShrink={1}>
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
        <Row label="Item 13" />
        <Row label="Item 14" />
        <Row label="Item 15" />
        <Row label="Item 16" />
        <Row label="Item 17" />
        <Row label="Item 18" />
        <Row label="Item 19" />
        <Row label="Item 20" />
        <Row label="Item 21" />
        <Row label="Item 22" />
        <Row label="Item 23" />
        <Row label="Item 24" />
        <Row label="Item 25" />
        <Row label="Item 26" />
        <Row label="Item 27" />
        <Row label="Item 28" />
        <Row label="Item 29" />
        <Row label="Item 30" />
        <Row label="Item 31" />
        <Row label="Item 32" />
        <Row label="Item 33" />
        <Row label="Item 34" />
        <Row label="Item 35" />
        <Row label="Item 36" />
        <Row label="Item 37" />
        <Row label="Item 38" />
        <Row label="Item 39" />
        <Row label="Item 40" />
        <Row label="Item 41" />
        <Row label="Item 42" />
        <Row label="Item 43" />
        <Row label="Item 44" />
        <Row label="Item 45" />
        <Row label="Item 46" />
        <Row label="Item 47" />
        <Row label="Item 48" />
      </View>
    </View>
  );
}

mount(Counter);
