// Styling demos: state sources and animated variables.
import {
  signal,
  View,
  Text,
  TextField,
  TapGesture,
  ViewStyle,
  createSharedValue,
  scheduleOnUI,
  withSpring,
  withTiming,
  ref,
} from "bubble-shell";
import { Section, Btn } from "./shared";

export function PressedDemo() {
  const btnRef = ref();
  const press = createSharedValue({ source: "pressed", target: btnRef });
  const taps = signal(0);
  return (
    <View gap={12}>
      <Section title="Pressed source — opacity dips while held">
        <View
          ref={btnRef}
          role="button"
          label="Press and hold"
          backgroundColor={0xff3b82f6}
          radius={8}
          width={190}
          height={48}
          justifyContent="center"
          alignItems="center"
        >
          <TapGesture onTap={() => taps.update((c) => c + 1)} />
          <Text color="#ffffff" fontSize={15}>{taps() + " taps — hold me"}</Text>
          <ViewStyle target={btnRef} when={press > 0} opacity={1 - press * 0.4} />
        </View>
        <Text color="#6b7280" fontSize={12}>The pressed progress source is driven by the UI thread; the declaration patches opacity from 1 down to 0.6.</Text>
      </Section>
    </View>
  );
}

export function FocusedDemo() {
  const fieldRef = ref();
  const focused = createSharedValue({ source: "focused", target: fieldRef });
  const editing = createSharedValue({ source: "editing", target: fieldRef });
  const echo = signal("");
  return (
    <View gap={12}>
      <Section title="Focused and editing sources">
        <TextField
          ref={fieldRef}
          value=""
          height={44}
          fontSize={15}
          color="#ffffff"
          backgroundColor={0xff4b5563}
          radius={6}
          padding={[8, 8, 8, 8]}
          label="Focus me"
          onChange={(e) => echo.set(e.value)}
        >
          <ViewStyle target={fieldRef} when={focused} backgroundColor={0xff2563eb} />
          <ViewStyle target={fieldRef} when={editing} opacity={0.85} />
        </TextField>
        <Text color="#9ca3af" fontSize={13}>Echo: {echo()}</Text>
        <Text color="#6b7280" fontSize={12}>Tap the field: background turns blue while focused, and dims while an edit session is active.</Text>
      </Section>
    </View>
  );
}

export function AnimatedDemo() {
  const springBox = ref();
  const tweenBox = ref();
  const slide = createSharedValue(0);
  const glide = createSharedValue(0);
  // Animations start on the UI thread: a JS tap schedules a worklet whose
  // assignment of withTiming/withSpring drives the value. The destination is
  // a worklet parameter — scheduleOnUI's extra args must match that arity.
  const springTo = (target) => {
    scheduleOnUI((target) => {
      "worklet";
      slide.value = withSpring(target, { stiffness: 200, damping: 18 });
    }, target);
  };
  const tweenTo = (target) => {
    scheduleOnUI((target) => {
      "worklet";
      glide.value = withTiming(target, { duration: 400, easing: "easeInOut" });
    }, target);
  };
  return (
    <View gap={12}>
      <Section title="Spring animation on a script-driven var">
        <View ref={springBox} backgroundColor={0xff22c55e} radius={10} width={56} height={56} />
        <ViewStyle target={springBox} when={true} translateX={slide} />
        <View flexDirection="row" gap={8}>
          <Btn label="Spring to 100" color={0xff22c55e} onTap={() => springTo(100)} />
          <Btn label="Spring to 0" color={0xff4b5563} onTap={() => springTo(0)} />
        </View>
      </Section>
      <Section title="Tween animation on a script-driven var">
        <View ref={tweenBox} backgroundColor={0xfff59e0b} radius={10} width={56} height={56} />
        <ViewStyle target={tweenBox} when={true} translateX={glide} />
        <View flexDirection="row" gap={8}>
          <Btn label="Tween to 100" color={0xfff59e0b} onTap={() => tweenTo(100)} />
          <Btn label="Tween to 0" color={0xff4b5563} onTap={() => tweenTo(0)} />
        </View>
      </Section>
    </View>
  );
}
