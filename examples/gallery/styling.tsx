// Styling demos: state sources, declarative style transitions, and animated variables.
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
          transition={{ duration: 160 }}
          transition:opacity={{ duration: 100 }}
        >
          <TapGesture onTap={() => taps.update((c) => c + 1)} />
          <Text color="#ffffff" fontSize={15}>{taps() + " taps — hold me"}</Text>
          <ViewStyle when={press > 0} backgroundColor="#1e40af" opacity={1 - press * 0.4} />
        </View>
        <Text color="#6b7280" fontSize={12}>Hold the button: background and opacity ease toward the pressed style, then back. Opacity uses the shorter per-property duration.</Text>
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
          transition:backgroundColor={{ duration: 160 }}
          onChange={(e) => echo.set(e.value)}
        >
          <ViewStyle when={focused} backgroundColor={0xff2563eb} />
          <ViewStyle when={editing} opacity={0.85} />
        </TextField>
        <Text color="#9ca3af" fontSize={13}>Echo: {echo()}</Text>
        <Text color="#6b7280" fontSize={12}>Tap the field: the background eases to blue while focused, and back when focus leaves.</Text>
      </Section>
    </View>
  );
}

export function AnimatedDemo() {
  const slide = createSharedValue(0);
  const glide = createSharedValue(0);
  const active = signal(false);
  const transitionsEnabled = signal(true);
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
        <View backgroundColor={0xff22c55e} radius={10} width={56} height={56}>
          <ViewStyle translateX={slide} />
        </View>
        <View flexDirection="row" gap={8}>
          <Btn label="Spring to 100" color={0xff22c55e} onTap={() => springTo(100)} />
          <Btn label="Spring to 0" color={0xff4b5563} onTap={() => springTo(0)} />
        </View>
      </Section>
      <Section title="Tween animation on a script-driven var">
        <View backgroundColor={0xfff59e0b} radius={10} width={56} height={56}>
          <ViewStyle translateX={glide} />
        </View>
        <View flexDirection="row" gap={8}>
          <Btn label="Tween to 100" color={0xfff59e0b} onTap={() => tweenTo(100)} />
          <Btn label="Tween to 0" color={0xff4b5563} onTap={() => tweenTo(0)} />
        </View>
      </Section>
      <Section title="Declarative style transition">
        <View
          backgroundColor={active() ? "#2563eb" : "#f59e0b"}
          opacity={active() ? 1 : 0.4}
          radius={active() ? 24 : 8}
          width={56}
          height={56}
          transition={transitionsEnabled() ? { duration: 1000, easing: "linear" } : false}
          transition:opacity={{ duration: 500 }}
          transition:translateX={false}
        />
        <Text
          color={active() ? "#2563eb" : "#f59e0b"}
          fontSize={14}
          transition={transitionsEnabled() ? { duration: 1000, easing: "linear" } : false}
        >
          {active() ? "active" : "rest"}
        </Text>
        <View flexDirection="row" gap={8}>
          <Btn label="Toggle target" color={0xff2563eb} onTap={() => active.set(!active())} />
          <Btn
            label={transitionsEnabled() ? "Disable transitions" : "Enable transitions"}
            color={0xff4b5563}
            onTap={() => transitionsEnabled.set(!transitionsEnabled())}
          />
        </View>
      </Section>
    </View>
  );
}

// Layout frame and scroll-content size, sampled on the UI thread. The corner
// badge follows the frame through style transitions; the travel marker only
// moves when a worklet reads the measured width.
export function MeasuredDemo() {
  const boxRef = ref();
  const scrollRef = ref();
  const boxWidth = createSharedValue({ source: "layout.width", target: boxRef });
  const boxHeight = createSharedValue({ source: "layout.height", target: boxRef });
  const viewportWidth = createSharedValue({ source: "layout.width", target: scrollRef });
  const viewportHeight = createSharedValue({ source: "layout.height", target: scrollRef });
  const contentWidth = createSharedValue({ source: "scroll.contentWidth", target: scrollRef });
  const contentHeight = createSharedValue({ source: "scroll.contentHeight", target: scrollRef });
  const scrollX = createSharedValue({ source: "scroll.x", target: scrollRef });
  const scrollY = createSharedValue({ source: "scroll.y", target: scrollRef });
  const travel = createSharedValue(0);
  const wide = signal(false);
  const fitted = signal(false);
  const animateEdge = () => {
    scheduleOnUI(() => {
      "worklet";
      travel.value = withTiming(boxWidth.value - 20, { duration: 300, easing: "linear" });
    });
  };
  const resetTravel = () => {
    scheduleOnUI(() => {
      "worklet";
      travel.value = withTiming(0, { duration: 300, easing: "linear" });
    });
  };
  return (
    <View gap={12}>
      <Section title="Measured frame — layout.width / layout.height">
        <Text color="#9ca3af" fontSize={13}>
          The amber badge eases to the measured corner. The blue marker moves only when Animate reads that width on the UI thread.
        </Text>
        <View
          ref={boxRef}
          width={wide() ? 240 : 160}
          height={wide() ? 120 : 80}
          flexShrink={0}
          backgroundColor={0xff172554}
          radius={6}
        >
          <View
            position="absolute"
            start={0}
            top={0}
            width={20}
            height={20}
            backgroundColor={0xfff59e0b}
            transition:translateX={{ duration: 300, easing: "linear" }}
            transition:translateY={{ duration: 300, easing: "linear" }}
          >
            <ViewStyle
              translateX={max(0, boxWidth - 20)}
              translateY={max(0, boxHeight - 20)}
            />
            <ViewStyle when={boxWidth >= 200} backgroundColor={0xff22c55e} />
          </View>
          <View
            position="absolute"
            start={0}
            top={0}
            width={20}
            height={20}
            backgroundColor={0xff38bdf8}
          >
            <ViewStyle translateX={travel} />
          </View>
        </View>
        <View flexDirection="row" flexWrap="wrap" gap={8}>
          <Btn label="Resize box" color={0xff2563eb} onTap={() => wide.set(!wide())} />
          <Btn label="Animate to measured edge" color={0xff0ea5e9} onTap={() => animateEdge()} />
          <Btn label="Reset animation" color={0xff4b5563} onTap={() => resetTravel()} />
        </View>
      </Section>
      <Section title="Scroll content size">
        <Text color="#9ca3af" fontSize={13}>
          Content size starts at 360×260 and is 180×120 after fit. Scroll offset does not change those sizes.
        </Text>
        <View flexDirection="row" flexWrap="wrap" gap={8}>
          <View backgroundColor={0xff6b7280} radius={6} padding={[6, 10, 6, 10]}>
            <ViewStyle when={viewportWidth == 180} backgroundColor={0xff22c55e} />
            <Text color="#ffffff" fontSize={12}>viewport width 180</Text>
          </View>
          <View backgroundColor={0xff6b7280} radius={6} padding={[6, 10, 6, 10]}>
            <ViewStyle when={viewportHeight == 120} backgroundColor={0xff22c55e} />
            <Text color="#ffffff" fontSize={12}>viewport height 120</Text>
          </View>
          <View backgroundColor={0xff6b7280} radius={6} padding={[6, 10, 6, 10]}>
            <ViewStyle when={contentWidth > viewportWidth} backgroundColor={0xff22c55e} />
            <Text color="#ffffff" fontSize={12}>content wider</Text>
          </View>
          <View backgroundColor={0xff6b7280} radius={6} padding={[6, 10, 6, 10]}>
            <ViewStyle when={contentHeight > viewportHeight} backgroundColor={0xff22c55e} />
            <Text color="#ffffff" fontSize={12}>content taller</Text>
          </View>
        </View>
        <View
          ref={scrollRef}
          width={180}
          height={120}
          flexShrink={0}
          overflowX="scroll"
          overflowY="scroll"
          backgroundColor={0xff172554}
          radius={6}
        >
          <View
            width={fitted() ? 100 : 360}
            height={fitted() ? 80 : 260}
            flexShrink={0}
            backgroundColor={0xff1f2937}
          />
        </View>
        <View position="relative" width={100} height={14} backgroundColor={0xff374151} radius={4}>
          <View position="absolute" start={0} top={0} width={10} height={10} backgroundColor={0xfff59e0b}>
            <ViewStyle
              translateX={90 * clamp(scrollX / max(1, contentWidth - viewportWidth), 0, 1)}
            />
          </View>
        </View>
        <View position="relative" width={100} height={14} backgroundColor={0xff374151} radius={4}>
          <View position="absolute" start={0} top={0} width={10} height={10} backgroundColor={0xff38bdf8}>
            <ViewStyle
              translateX={90 * clamp(scrollY / max(1, contentHeight - viewportHeight), 0, 1)}
            />
          </View>
        </View>
        <Btn
          label="Fit content"
          color={0xff2563eb}
          onTap={() => fitted.set(!fitted())}
        />
      </Section>
    </View>
  );
}
