// Control-flow demos: Show, For, Index, Switch/Match, Slot.
import { signal, View, Text, Show, For, Index, Switch, Match, Slot } from "bubble-shell";
import { Section, Btn } from "./shared";

export function ShowDemo() {
  const visible = signal(true);
  return (
    <View gap={12}>
      <Section title="Show with fallback">
        <Btn label={visible() ? "Hide" : "Show"} color={0xff3b82f6} onTap={() => visible.update((v) => !v)} />
        <Show
          when={visible()}
          fallback={<Text color="#6b7280" fontSize={14}>Hidden — this is the fallback branch.</Text>}
        >
          {() => (
            <View backgroundColor={0xff22c55e} radius={6} padding={[10, 10, 10, 12]}>
              <Text color="#ffffff" fontSize={14}>Visible content</Text>
            </View>
          )}
        </Show>
      </Section>
    </View>
  );
}

export function ForDemo() {
  const items = signal(["one", "two", "three"]);
  let next = 4;
  return (
    <View gap={12}>
      <Section title="For — keyed, add and remove">
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn label="Add" color={0xff22c55e} onTap={() => items.update((list) => [...list, "fresh " + next++])} />
          <Btn label="Remove last" color={0xfff59e0b} onTap={() => items.update((list) => list.slice(0, -1))} />
        </View>
        <View backgroundColor={0xff374151} radius={6} padding={[8, 8, 8, 10]} gap={4}>
          <For each={items()} by={(item) => item}>
            {(read) => <Text color="#a3e635" fontSize={14}>item {read()}</Text>}
          </For>
        </View>
      </Section>
    </View>
  );
}

export function IndexDemo() {
  const items = signal(["alpha", "beta", "gamma"]);
  return (
    <View gap={12}>
      <Section title="Index — rows reused by position">
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn label="Reverse" color={0xff3b82f6} onTap={() => items.set(["gamma", "beta", "alpha"])} />
          <Btn label="Replace first" color={0xff22c55e} onTap={() => items.set(["first", "beta", "gamma"])} />
        </View>
        <View backgroundColor={0xff374151} radius={6} padding={[8, 8, 8, 10]} gap={4}>
          <Index each={items()}>
            {(read, i) => <Text color="#93c5fd" fontSize={14}>{i + ": " + read()}</Text>}
          </Index>
        </View>
      </Section>
    </View>
  );
}

export function SwitchDemo() {
  const mode = signal("a");
  return (
    <View gap={12}>
      <Section title="Switch / Match with fallback">
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn label="Mode A" color={0xff3b82f6} onTap={() => mode.set("a")} />
          <Btn label="Mode B" color={0xff22c55e} onTap={() => mode.set("b")} />
          <Btn label="Mode C" color={0xfff59e0b} onTap={() => mode.set("c")} />
        </View>
        <Switch fallback={<Text color="#6b7280" fontSize={14}>Mode C matches nothing — Switch fallback.</Text>}>
          <Match when={mode() === "a"}>
            {() => <Text color="#60a5fa" fontSize={16}>Mode A content</Text>}
          </Match>
          <Match when={mode() === "b"}>
            {() => <Text color="#4ade80" fontSize={16}>Mode B content</Text>}
          </Match>
        </Switch>
      </Section>
    </View>
  );
}

function SlotCard(props) {
  return (
    <View backgroundColor={0xff1b1e27} radius={8} padding={[12, 12, 12, 14]} gap={8}>
      <Slot of={props.header} fallback={<Text color="#666677" fontSize={12}>no header slot</Text>} />
      {props.children}
      <Text color="#8892a0" fontSize={12}>tail</Text>
    </View>
  );
}

function ForwardingCard(props) {
  return <SlotCard>{props.children}</SlotCard>;
}

export function SlotDemo() {
  return (
    <View gap={12}>
      <Section title="Slot — named slot, fallback, forwarding">
        <SlotCard header={<Text color="#f87171" fontSize={14} fontWeight={700}>Named header</Text>}>
          <Text color="#ffffff" fontSize={14}>Body children</Text>
        </SlotCard>
        <SlotCard>
          <Text color="#60a5fa" fontSize={14}>No header passed — fallback rendered instead.</Text>
        </SlotCard>
        <ForwardingCard>
          <Text color="#a3e635" fontSize={14}>Children forwarded through two components.</Text>
        </ForwardingCard>
      </Section>
    </View>
  );
}
