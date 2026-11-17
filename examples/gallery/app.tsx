// Bubble Shell gallery entry — sidebar navigation over demos split by concern.
//
// Run with: bubble dev examples/gallery/app.tsx
// Bundle with: bubble bundle examples/gallery/app.tsx --out target/gallery
import { signal, View, Text, TapGesture, For, Switch, Match, ScrollBar } from "bubble-shell";
import { ViewDemo, TextDemo, ImageDemo, TextFieldDemo } from "./primitives";
import { ScrollBarDemo } from "./scrollbar";
import {
  TapDemo,
  DoubleTapDemo,
  LongPressDemo,
  PanDemo,
  TransformDemo,
  GestureVarsDemo,
} from "./gestures";
import { ShowDemo, ForDemo, IndexDemo, SwitchDemo, SlotDemo } from "./control-flow";
import { LayoutDemo, TextPropsDemo } from "./layout";
import { PressedDemo, FocusedDemo, AnimatedDemo } from "./styling";
import {
  StorageDemo,
  ToastDemo,
  ClipboardDemo,
  FetchDemo,
  OpenNavDemo,
  HostDemo,
} from "./platform";

const demos = [
  { name: "View", demo: ViewDemo },
  { name: "Text", demo: TextDemo },
  { name: "Image", demo: ImageDemo },
  { name: "TextField", demo: TextFieldDemo },
  { name: "ScrollBar", demo: ScrollBarDemo },
  { name: "Tap", demo: TapDemo },
  { name: "DoubleTap", demo: DoubleTapDemo },
  { name: "LongPress", demo: LongPressDemo },
  { name: "Pan", demo: PanDemo },
  { name: "Transform", demo: TransformDemo },
  { name: "Gesture Vars", demo: GestureVarsDemo },
  { name: "Show", demo: ShowDemo },
  { name: "For", demo: ForDemo },
  { name: "Index", demo: IndexDemo },
  { name: "Switch", demo: SwitchDemo },
  { name: "Slot", demo: SlotDemo },
  { name: "Layout", demo: LayoutDemo },
  { name: "Text Props", demo: TextPropsDemo },
  { name: "Pressed", demo: PressedDemo },
  { name: "Focused", demo: FocusedDemo },
  { name: "Animated", demo: AnimatedDemo },
  { name: "Storage", demo: StorageDemo },
  { name: "Toast", demo: ToastDemo },
  { name: "Clipboard", demo: ClipboardDemo },
  { name: "Fetch", demo: FetchDemo },
  { name: "OpenURL / Navigate", demo: OpenNavDemo },
  { name: "Host Bridge", demo: HostDemo },
];

function App() {
  const selected = signal(0);
  return (
    <View backgroundColor="#0d0f14" flexGrow={1} flexShrink={1} flexDirection="row" padding={[10, 10, 10, 10]} gap={10}>
      <View backgroundColor="#111827" radius={8} width={190} overflowY="scroll" padding={[10, 10, 10, 10]} gap={4}>
        <ScrollBar
          axis="vertical"
          scrollbarWidth={5}
          scrollbarMargin={4}
          scrollbarColor={0xff60a5fa}
          scrollbarRadius={3}
          scrollbarOpacity={1}
          autoHide={false}
        />
        <Text color="#6b7280" fontSize={11}>bubble gallery · {demos.length} demos</Text>
        <For each={demos} by={(d) => d.name}>
          {(read, i) => (
            <View
              role="button"
              label={"Open " + read().name}
              backgroundColor={selected() === i ? "#2563eb" : "#1f2937"}
              radius={6}
              padding={[8, 8, 8, 8]}
            >
              <TapGesture onTap={() => selected.set(i)} />
              <Text color={selected() === i ? "#ffffff" : "#9ca3af"} fontSize={13}>{read().name}</Text>
            </View>
          )}
        </For>
      </View>
      <View
        backgroundColor="#111827"
        radius={8}
        flexGrow={1}
        flexShrink={1}
        padding={[12, 12, 12, 12]}
        gap={12}
        overflowY="scroll"
      >
        <Text color="#ffffff" fontSize={24} fontWeight={700}>{demos[selected()].name}</Text>
        <Switch>
          <Match when={selected() === 0}>{() => <ViewDemo />}</Match>
          <Match when={selected() === 1}>{() => <TextDemo />}</Match>
          <Match when={selected() === 2}>{() => <ImageDemo />}</Match>
          <Match when={selected() === 3}>{() => <TextFieldDemo />}</Match>
          <Match when={selected() === 4}>{() => <ScrollBarDemo />}</Match>
          <Match when={selected() === 5}>{() => <TapDemo />}</Match>
          <Match when={selected() === 6}>{() => <DoubleTapDemo />}</Match>
          <Match when={selected() === 7}>{() => <LongPressDemo />}</Match>
          <Match when={selected() === 8}>{() => <PanDemo />}</Match>
          <Match when={selected() === 9}>{() => <TransformDemo />}</Match>
          <Match when={selected() === 10}>{() => <GestureVarsDemo />}</Match>
          <Match when={selected() === 11}>{() => <ShowDemo />}</Match>
          <Match when={selected() === 12}>{() => <ForDemo />}</Match>
          <Match when={selected() === 13}>{() => <IndexDemo />}</Match>
          <Match when={selected() === 14}>{() => <SwitchDemo />}</Match>
          <Match when={selected() === 15}>{() => <SlotDemo />}</Match>
          <Match when={selected() === 16}>{() => <LayoutDemo />}</Match>
          <Match when={selected() === 17}>{() => <TextPropsDemo />}</Match>
          <Match when={selected() === 18}>{() => <PressedDemo />}</Match>
          <Match when={selected() === 19}>{() => <FocusedDemo />}</Match>
          <Match when={selected() === 20}>{() => <AnimatedDemo />}</Match>
          <Match when={selected() === 21}>{() => <StorageDemo />}</Match>
          <Match when={selected() === 22}>{() => <ToastDemo />}</Match>
          <Match when={selected() === 23}>{() => <ClipboardDemo />}</Match>
          <Match when={selected() === 24}>{() => <FetchDemo />}</Match>
          <Match when={selected() === 25}>{() => <OpenNavDemo />}</Match>
          <Match when={selected() === 26}>{() => <HostDemo />}</Match>
        </Switch>
      </View>
    </View>
  );
}

export default App;
