import { mount, signal, Text, View, TapGesture, DoubleTapGesture, LongPressGesture, PanGesture } from "bubble-shell";

function Gestures() {
  const status = signal("");
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={8}>
      <Text testID="status" color="#ffffff" fontSize={20}>{status()}</Text>
      <View testID="tap-area" backgroundColor={0xff3b82f6} radius={8} padding={[12, 12, 12, 12]}>
        <TapGesture onTap={() => status.update(() => "tapped")} />
        <Text color="#ffffff" fontSize={16}>Tap area</Text>
      </View>
      <View testID="double-area" backgroundColor={0xff22c55e} radius={8} padding={[12, 12, 12, 12]}>
        <DoubleTapGesture onDoubleTap={() => status.update(() => "double-tapped")} />
        <Text color="#ffffff" fontSize={16}>Double tap area</Text>
      </View>
      <View testID="long-area" backgroundColor={0xfff59e0b} radius={8} padding={[12, 12, 12, 12]}>
        <LongPressGesture duration={400} onLongPress={() => status.update(() => "long-pressed")} />
        <Text color="#ffffff" fontSize={16}>Long press area</Text>
      </View>
      <View testID="pan-area" backgroundColor={0xff8b5cf6} radius={8} height={120}>
        <PanGesture axis="both" minDistance={10} onPanEnd={() => status.update(() => "panned")} />
        <Text color="#ffffff" fontSize={16}>Pan area</Text>
      </View>
    </View>
  );
}

mount(Gestures);
