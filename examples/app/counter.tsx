import { mount, signal, Text, View } from "bubble";

function Counter() {
  const count = signal(0);
  return (
    <View background={0xff111111} flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]}>
      <Text color={0xffffffff} fontSize={24}>Count: {count()}</Text>
      <View
        background={0xff3b82f6}
        radius={8}
        padding={[12, 8, 12, 8]}
        onPress={() => count.update((c) => c + 1)}
      >
        <Text color={0xffffffff} fontSize={16}>Increment</Text>
      </View>
      <View scrollY flexGrow={1} flexShrink={1}>
        <Text color={0xffdddddd} fontSize={18}>Alpha</Text>
        <Text color={0xffdddddd} fontSize={18}>Bravo</Text>
        <Text color={0xffdddddd} fontSize={18}>Charlie</Text>
        <Text color={0xffdddddd} fontSize={18}>Delta</Text>
        <Text color={0xffdddddd} fontSize={18}>Echo</Text>
        <Text color={0xffdddddd} fontSize={18}>Foxtrot</Text>
        <Text color={0xffdddddd} fontSize={18}>Golf</Text>
        <Text color={0xffdddddd} fontSize={18}>Hotel</Text>
        <Text color={0xffdddddd} fontSize={18}>India</Text>
        <Text color={0xffdddddd} fontSize={18}>Juliet</Text>
        <Text color={0xffdddddd} fontSize={18}>Kilo</Text>
        <Text color={0xffdddddd} fontSize={18}>Lima</Text>
      </View>
    </View>
  );
}

mount(Counter);
