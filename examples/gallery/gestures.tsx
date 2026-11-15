// Gesture demos: tap, double tap, long press, pan, transform, gesture variables.
import {
  signal,
  View,
  Text,
  TapGesture,
  DoubleTapGesture,
  LongPressGesture,
  PanGesture,
  TransformGesture,
  ViewStyle,
  For,
  createSharedValue,
  scheduleOnUI,
  ref,
} from "bubble-shell";
import { Section } from "./shared";

export function TapDemo() {
  const taps = signal(0);
  return (
    <View gap={12}>
      <Section title="TapGesture — onTap">
        <View role="button" label="Tap card" backgroundColor={0xff3b82f6} radius={8} padding={[14, 12, 12, 16]}>
          <TapGesture onTap={() => taps.update((c) => c + 1)} />
          <Text color="#ffffff" fontSize={16}>Tap me</Text>
        </View>
        <Text color="#d1d5db" fontSize={14}>Taps: {taps()}</Text>
      </Section>
    </View>
  );
}

export function DoubleTapDemo() {
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="DoubleTapGesture — onDoubleTap">
        <View role="button" label="Double tap card" backgroundColor={0xff22c55e} radius={8} padding={[14, 12, 12, 16]}>
          <DoubleTapGesture onDoubleTap={() => status.set("double-tapped")} />
          <Text color="#ffffff" fontSize={16}>Double tap me</Text>
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
      </Section>
    </View>
  );
}

export function LongPressDemo() {
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="LongPressGesture — 500 ms">
        <View role="button" label="Long press card" backgroundColor={0xfff59e0b} radius={8} padding={[14, 12, 12, 16]}>
          <LongPressGesture
            duration={500}
            onLongPress={() => status.set("long-pressed")}
            onLongPressEnd={() => status.set("long press ended")}
            onLongPressCancel={() => status.set("long press cancelled")}
          />
          <Text color="#ffffff" fontSize={16}>Hold me 500 ms</Text>
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
      </Section>
    </View>
  );
}

export function PanDemo() {
  const boxRef = ref();
  const panX = createSharedValue(0);
  const panY = createSharedValue(0);
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="PanGesture — drag moves the card via script vars">
        <View backgroundColor={0xff111827} radius={8} height={230} padding={[8, 8, 8, 8]}>
          <View
            ref={boxRef}
            backgroundColor={0xff8b5cf6}
            radius={8}
            width={130}
            height={72}
            justifyContent="center"
            alignItems="center"
          >
            <PanGesture
              axis="both"
              minDistance={5}
              onPanStart={(e) => {
                "worklet";
                scheduleOnJS(status.set, "panning");
              }}
              onPanUpdate={(e) => {
                "worklet";
                panX.value += e.deltaX;
                panY.value += e.deltaY;
              }}
              onPanEnd={(e) => {
                "worklet";
                scheduleOnJS(status.set, "panned, card stays where dropped");
              }}
              onPanCancel={(e) => {
                "worklet";
                scheduleOnJS(status.set, "pan cancelled");
              }}
            />
            <ViewStyle target={boxRef} when={true} translateX={panX} translateY={panY} />
            <Text color="#ffffff" fontSize={14}>drag me</Text>
          </View>
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
      </Section>
    </View>
  );
}

export function TransformDemo() {
  const boxRef = ref();
  const scaleV = createSharedValue(1);
  const rotateV = createSharedValue(0);
  const status = signal("idle");
  // JS-side formatter: the worklet calls it through scheduleOnJS with the
  // event numbers as arguments.
  const describe = (scale, rotation) =>
    status.set("scale " + scale.toFixed(2) + " rotate " + rotation.toFixed(1));
  return (
    <View gap={12}>
      <Section title="TransformGesture — pinch zoom, rotate, pan">
        <View backgroundColor={0xff111827} radius={8} height={240} justifyContent="center" alignItems="center">
          <View
            ref={boxRef}
            backgroundColor={0xff0ea5e9}
            radius={10}
            width={140}
            height={90}
            justifyContent="center"
            alignItems="center"
          >
            <TransformGesture
              mode={["pan", "pinch", "rotate"]}
              minDistance={5}
              onTransformStart={(e) => {
                "worklet";
                scheduleOnJS(status.set, "transforming");
              }}
              onTransformUpdate={(e) => {
                "worklet";
                scaleV.value = e.scale;
                rotateV.value = e.rotation;
                scheduleOnJS(describe, e.scale, e.rotation);
              }}
              onTransformEnd={(e) => {
                "worklet";
                scheduleOnJS(status.set, "transform ended");
              }}
              onTransformCancel={(e) => {
                "worklet";
                scheduleOnJS(status.set, "transform cancelled");
              }}
            />
            <ViewStyle target={boxRef} when={true} scale={scaleV} rotate={rotateV} />
            <Text color="#ffffff" fontSize={14}>pinch / rotate</Text>
          </View>
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
        <Text color="#6b7280" fontSize={12}>Pinch and rotate need two fingers on a touch device — the recognizer only pairs two pointers, so a mouse cannot drive this gesture.</Text>
      </Section>
    </View>
  );
}

// Gesture and scroll state sources live on the UI thread: no JS runs per
// frame. The pan card binds its shared values to the recognizer through the
// gesture ref; the bar mixes scroll.y in a style declaration.
const SCROLL_ROWS = Array.from({ length: 14 }, (_, index) => index + 1);

export function GestureVarsDemo() {
  const cardRef = ref();
  const panRef = ref();
  const scrollRef = ref();
  const barRef = ref();
  const tx = createSharedValue({ source: "gesture.translationX", target: panRef });
  const ty = createSharedValue({ source: "gesture.translationY", target: panRef });
  const sy = createSharedValue({ source: "scroll.y", target: scrollRef });
  return (
    <View gap={12}>
      <View backgroundColor="#1f2937" radius={8} padding={[12, 12, 12, 14]} gap={8}>
        <Text color="#fbbf24" fontSize={13} fontWeight={700}>Pan follows gesture-source variables</Text>
        <Text color="#9ca3af" fontSize={13}>
          The card reads gesture.translationX/Y straight from the recognizer — it never lags a frame behind.
        </Text>
        <View
          ref={cardRef}
          backgroundColor={0xff8b5cf6}
          radius={8}
          width={150}
          height={72}
          justifyContent="center"
          alignItems="center"
        >
          <PanGesture ref={panRef} onPanUpdate={() => {}} onPanEnd={() => {}} />
          <ViewStyle target={cardRef} when={true} translateX={tx} translateY={ty} />
          <Text color="#ffffff" fontSize={14}>drag me</Text>
        </View>
      </View>
      <View backgroundColor="#1f2937" radius={8} padding={[12, 12, 12, 14]} gap={8}>
        <Text color="#fbbf24" fontSize={13} fontWeight={700}>Bar color tracks scroll.y</Text>
        <Text color="#9ca3af" fontSize={13}>
          Scroll the list; the bar mixes blue to green over 200 px of scroll offset.
        </Text>
        <View
          ref={scrollRef}
          height={140}
          overflowY="scroll"
          backgroundColor={0xff374151}
          radius={6}
          padding={[8, 8, 8, 10]}
          gap={6}
        >
          <For each={SCROLL_ROWS} by={(row) => row}>
            {(read) => <Text color="#d1d5db" fontSize={14}>{"Scroll item " + read()}</Text>}
          </For>
        </View>
        <View ref={barRef} height={8} width={180} radius={4} backgroundColor={0xff3b82f6} />
        <ViewStyle
          target={barRef}
          when={true}
          backgroundColor={mix(0xff3b82f6, 0xff22c55e, clamp(sy / 200, 0, 1))}
        />
      </View>
    </View>
  );
}
