// ScrollBar demos: virtual thumbs on scroll containers and text fields.
import { signal, View, Text, TextField, ScrollBar, For } from "bubble-shell";
import { Section, Btn } from "./shared";

const ROWS = [];
for (let i = 1; i <= 16; i += 1) ROWS.push(i);
const CHIPS = [];
for (let i = 1; i <= 12; i += 1) CHIPS.push(i);
const LONG = "The quick brown fox jumps over the lazy dog. ".repeat(6);
const NOTE = "Field line\n".repeat(12);

export function ScrollBarDemo() {
  const pinned = signal(true);
  const live = signal(true);
  const thick = signal(false);
  return (
    <View gap={12}>
      <Section title="Vertical — thumb drag, width, autoHide, interactive">
        <View
          height={180}
          overflowY="scroll"
          backgroundColor={0xff374151}
          radius={6}
          padding={[8, 8, 8, 8]}
          gap={6}
        >
          <ScrollBar
            axis="vertical"
            scrollbarWidth={thick() ? 14 : 6}
            scrollbarMargin={3}
            scrollbarColor={0xff60a5fa}
            scrollbarRadius={4}
            scrollbarOpacity={1}
            scrollbarMinLength={36}
            autoHide={!pinned()}
            interactive={live()}
          />
          <For each={ROWS} by={(n) => n}>
            {(read) => <Text color="#d1d5db" fontSize={14}>Row {read()}</Text>}
          </For>
        </View>
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn
            label={pinned() ? "Pinned" : "Auto-hide"}
            color={pinned() ? 0xff2563eb : 0xff4b5563}
            onTap={() => pinned.update((v) => !v)}
          />
          <Btn
            label={live() ? "Draggable" : "Visual only"}
            color={live() ? 0xff2563eb : 0xff4b5563}
            onTap={() => live.update((v) => !v)}
          />
          <Btn
            label={thick() ? "Width 14" : "Width 6"}
            color={0xff4b5563}
            onTap={() => thick.update((v) => !v)}
          />
        </View>
        <Text color="#6b7280" fontSize={12}>
          The thumb shows only while this axis is scroll and the content overflows. autoHide hides it 1s after the last scroll. Drag maps thumb travel onto the scroll offset; interactive false still paints it. The left sidebar list uses the same component, pinned.
        </Text>
      </Section>
      <Section title="Horizontal — axis horizontal">
        <View
          height={56}
          width={280}
          overflowX="scroll"
          flexDirection="row"
          alignItems="center"
          gap={6}
          backgroundColor={0xff374151}
          radius={6}
          padding={[8, 8, 8, 8]}
        >
          <ScrollBar
            axis="horizontal"
            autoHide={false}
            scrollbarWidth={6}
            scrollbarMargin={2}
            scrollbarColor={0xff22c55e}
            scrollbarOpacity={1}
          />
          <For each={CHIPS} by={(n) => n}>
            {(read) => (
              <View backgroundColor={0xff1f2937} radius={6} padding={[6, 10, 6, 10]} flexShrink={0}>
                <Text color="#d1d5db" fontSize={13}>Chip {read()}</Text>
              </View>
            )}
          </For>
        </View>
        <Text color="#6b7280" fontSize={12}>A horizontal declaration ignores vertical overflow.</Text>
      </Section>
      <Section title="Both axes — two declarations on one view">
        <View
          height={150}
          width={320}
          overflowX="scroll"
          overflowY="scroll"
          backgroundColor={0xff374151}
          radius={6}
        >
          <ScrollBar
            axis="vertical"
            autoHide={false}
            scrollbarWidth={6}
            scrollbarMargin={2}
            scrollbarColor={0xff60a5fa}
            scrollbarOpacity={1}
          />
          <ScrollBar
            axis="horizontal"
            autoHide={false}
            scrollbarWidth={6}
            scrollbarMargin={2}
            scrollbarColor={0xff22c55e}
            scrollbarOpacity={1}
          />
          <View width={640} padding={[8, 8, 8, 8]} gap={6}>
            <For each={ROWS} by={(n) => n}>
              {(read) => (
                <Text color="#d1d5db" fontSize={13}>
                  Row {read()} — wide enough to scroll sideways, tall enough to scroll down.
                </Text>
              )}
            </For>
          </View>
        </View>
      </Section>
      <Section title="TextField — bar axis must match the field">
        <TextField
          value={LONG}
          width={260}
          height={40}
          fontSize={15}
          color="#ffffff"
          backgroundColor={0xff4b5563}
          radius={6}
          padding={[8, 8, 8, 8]}
          label="Single line"
        >
          <ScrollBar
            axis="horizontal"
            autoHide={false}
            scrollbarWidth={4}
            scrollbarColor={0xfff59e0b}
            scrollbarOpacity={1}
          />
        </TextField>
        <TextField
          value={NOTE}
          multiline
          width={260}
          height={88}
          fontSize={15}
          color="#ffffff"
          backgroundColor={0xff4b5563}
          radius={6}
          padding={[8, 8, 8, 8]}
          label="Multiline"
        >
          <ScrollBar
            axis="vertical"
            autoHide={false}
            scrollbarWidth={4}
            scrollbarColor={0xfff59e0b}
            scrollbarOpacity={1}
          />
        </TextField>
        <TextField
          value={LONG}
          width={260}
          height={40}
          fontSize={15}
          color="#ffffff"
          backgroundColor={0xff4b5563}
          radius={6}
          padding={[8, 8, 8, 8]}
          label="Mismatched axis"
        >
          <ScrollBar
            axis="vertical"
            autoHide={false}
            scrollbarWidth={8}
            scrollbarColor={0xffef4444}
            scrollbarOpacity={1}
          />
        </TextField>
        <Text color="#6b7280" fontSize={12}>
          Single-line fields scroll on x, multiline fields on y. The red vertical bar on the last field stays hidden because that axis does not scroll.
        </Text>
      </Section>
    </View>
  );
}
