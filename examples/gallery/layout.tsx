// Layout and text property demos.
import { signal, View, Text, TextField } from "bubble-shell";
import { Section } from "./shared";

export function LayoutDemo() {
  return (
    <View gap={12}>
      <Section title="Flex wrap grid">
        <View flexDirection="row" flexWrap="wrap" gap={8}>
          <View backgroundColor={0xff3b82f6} radius={6} width={64} height={40} />
          <View backgroundColor={0xff22c55e} radius={6} width={64} height={40} />
          <View backgroundColor={0xfff59e0b} radius={6} width={64} height={40} />
          <View backgroundColor={0xff8b5cf6} radius={6} width={64} height={40} />
          <View backgroundColor={0xff0ea5e9} radius={6} width={64} height={40} />
          <View backgroundColor={0xffef4444} radius={6} width={64} height={40} />
        </View>
      </Section>
      <Section title="Absolute positioning with insets">
        <View height={110} backgroundColor={0xff374151} radius={6}>
          <View position="absolute" top={8} end={8} backgroundColor={0xfff59e0b} radius={6} width={80} height={34} />
          <View position="absolute" bottom={8} start={8} backgroundColor={0xff0ea5e9} radius={6} width={80} height={34} />
        </View>
      </Section>
      <Section title="Overflow hidden vs scroll">
        <View flexDirection="row" gap={8}>
          <View width={120} height={96} overflow="hidden" backgroundColor={0xff374151} radius={6} padding={[6, 6, 6, 6]}>
            <View backgroundColor={0xff8b5cf6} width={130} height={130} radius={4} />
          </View>
          <View width={120} height={96} overflowY="scroll" backgroundColor={0xff374151} radius={6} padding={[6, 6, 6, 6]}>
            <View backgroundColor={0xff22c55e} width={130} height={130} radius={4} />
          </View>
        </View>
      </Section>
      <Section title="Aspect ratio, opacity, radius">
        <View flexDirection="row" gap={8} alignItems="center">
          <View aspectRatio={1} height={72} backgroundColor={0xff3b82f6} radius={14} />
          <View aspectRatio={16 / 9} height={72} backgroundColor={0xff22c55e} radius={8} opacity={0.6} />
        </View>
      </Section>
    </View>
  );
}

export function TextPropsDemo() {
  const size = signal(16);
  return (
    <View gap={12}>
      <Section title="Size, weight, alignment, lineHeight">
        <Text fontSize={12} color="#d1d5db">12px regular</Text>
        <Text fontSize={20} color="#d1d5db">20px regular</Text>
        <Text fontSize={28} fontWeight={700} color="#ffffff">28px bold</Text>
        <View width={240} backgroundColor={0xff374151} radius={6} padding={[6, 8, 8, 6]} gap={4}>
          <Text fontSize={14} color="#ffffff" textAlign="left">aligned left</Text>
          <Text fontSize={14} color="#ffffff" textAlign="center">aligned center</Text>
          <Text fontSize={14} color="#ffffff" textAlign="right">aligned right</Text>
        </View>
        <Text fontSize={14} color="#9ca3af" lineHeight={26}>
          lineHeight 26 with wrapping text that runs long enough to show the
          spacing between wrapped lines.
        </Text>
      </Section>
      <Section title="Live fontSize driven by a TextField">
        <TextField
          value="16"
          height={40}
          fontSize={15}
          color="#ffffff"
          backgroundColor={0xff4b5563}
          radius={6}
          padding={[8, 10, 10, 8]}
          keyboard="number"
          label="Font size"
          onChange={(e) => {
            const n = Number(e.value);
            if (n >= 8 && n <= 64) size.set(n);
          }}
        />
        <Text fontSize={size()} color="#ffffff">Live: {size()}px</Text>
      </Section>
    </View>
  );
}
