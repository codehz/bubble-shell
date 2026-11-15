// Bubble Shell gallery — one app demonstrating every primitive, gesture,
// control-flow construct, style declaration, shared variable source, and host
// API. Sidebar navigation on the left, active demo on the right.
//
// Run with: bubble dev examples/gallery/app.tsx
// Bundle with: bubble bundle examples/gallery/app.tsx --out target/gallery
import {
  mount,
  signal,
  View,
  Text,
  TextField,
  Image,
  TapGesture,
  DoubleTapGesture,
  LongPressGesture,
  PanGesture,
  TransformGesture,
  Show,
  For,
  Index,
  Switch,
  Match,
  Slot,
  ViewStyle,
  createVar,
  ref,
  storage,
  clipboard,
  toast,
  fetch,
  openURL,
  navigate,
  host,
  view,
  text,
  panGesture,
  primitive,
} from "bubble-shell";
import logo from "./assets/logo.png";

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

function Section(props) {
  return (
    <View backgroundColor="#1f2937" radius={8} padding={[12, 12, 12, 14]} gap={8}>
      <Text color="#fbbf24" fontSize={13} fontWeight={700}>{props.title()}</Text>
      {props.children}
    </View>
  );
}

function Btn(props) {
  return (
    <View
      role="button"
      label={props.label()}
      backgroundColor={props.color()}
      radius={8}
      padding={[10, 10, 10, 14]}
    >
      <TapGesture onTap={props.onTap} />
      <Text color="#ffffff" fontSize={14}>{props.label()}</Text>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

function ViewDemo() {
  const numbers = [];
  for (let i = 1; i <= 20; i += 1) numbers.push(i);
  return (
    <View gap={12}>
      <Section title="Row — flexDirection row + gap">
        <View flexDirection="row" gap={8}>
          <View backgroundColor={0xff3b82f6} radius={6} width={60} height={40} />
          <View backgroundColor={0xff22c55e} radius={6} width={60} height={40} />
          <View backgroundColor={0xfff59e0b} radius={6} width={60} height={40} />
        </View>
      </Section>
      <Section title="Column — alignItems center, justifyContent space-between">
        <View
          flexDirection="column"
          alignItems="center"
          justifyContent="space-between"
          height={120}
          backgroundColor={0xff374151}
          radius={6}
          padding={[8, 8, 8, 8]}
        >
          <View backgroundColor={0xff8b5cf6} width={80} height={24} radius={4} />
          <View backgroundColor={0xff8b5cf6} width={80} height={24} radius={4} />
          <View backgroundColor={0xff8b5cf6} width={80} height={24} radius={4} />
        </View>
      </Section>
      <Section title="Scroll — overflowY scroll over 20 items">
        <View
          height={160}
          overflowY="scroll"
          backgroundColor={0xff374151}
          radius={6}
          padding={[8, 8, 8, 10]}
          gap={6}
        >
          <For each={numbers} by={(n) => n}>
            {(read) => <Text color="#d1d5db" fontSize={14}>Scroll item {read()}</Text>}
          </For>
        </View>
      </Section>
      <Section title="Nested flex — column of rows with grow">
        <View flexDirection="column" gap={6} backgroundColor={0xff374151} radius={6} padding={[8, 8, 8, 10]}>
          <View flexDirection="row" gap={6}>
            <View backgroundColor={0xff0ea5e9} flexGrow={1} height={28} radius={4} />
            <View backgroundColor={0xff0ea5e9} flexGrow={2} height={28} radius={4} />
          </View>
          <View flexDirection="row" gap={6}>
            <View backgroundColor={0xff64748b} flexGrow={1} height={28} radius={4} />
            <View backgroundColor={0xff64748b} flexGrow={1} height={28} radius={4} />
          </View>
        </View>
      </Section>
    </View>
  );
}

function TextDemo() {
  const count = signal(0);
  return (
    <View gap={12}>
      <Section title="Font sizes and weights">
        <Text fontSize={12} color="#d1d5db">12px regular</Text>
        <Text fontSize={16} color="#d1d5db">16px regular</Text>
        <Text fontSize={20} color="#d1d5db">20px regular</Text>
        <Text fontSize={24} fontWeight={700} color="#ffffff">24px bold</Text>
        <Text fontSize={32} fontWeight={700} color="#ffffff">32px bold</Text>
      </Section>
      <Section title="Colors — hex, rgb, oklch">
        <Text fontSize={15} color="#22c55e">hex green</Text>
        <Text fontSize={15} color="rgb(96, 165, 250)">rgb blue</Text>
        <Text fontSize={15} color="oklch(0.72 0.19 328)">oklch pink</Text>
      </Section>
      <Section title="Alignment">
        <View width={240} backgroundColor={0xff374151} radius={6} padding={[6, 8, 8, 6]} gap={4}>
          <Text fontSize={14} color="#ffffff" textAlign="left">aligned left</Text>
          <Text fontSize={14} color="#ffffff" textAlign="center">aligned center</Text>
          <Text fontSize={14} color="#ffffff" textAlign="right">aligned right</Text>
        </View>
      </Section>
      <Section title="Reactive content — signal drives text">
        <Btn label="Increment" color={0xff3b82f6} onTap={() => count.update((c) => c + 1)} />
        <Text color="#ffffff" fontSize={18}>Count: {count()}</Text>
      </Section>
      <Section title="Line height">
        <Text fontSize={14} color="#9ca3af" lineHeight={26}>
          lineHeight 26 with wrapping text that runs long enough to show the
          spacing between the wrapped lines of this paragraph.
        </Text>
      </Section>
    </View>
  );
}

function ImageDemo() {
  return (
    <View gap={12}>
      <Section title="Image — fit contain">
        <View backgroundColor={0xff374151} radius={6} width={240} height={120} padding={[8, 8, 8, 8]}>
          <Image src={logo} width={224} height={104} fit="contain" label="logo contained" />
        </View>
      </Section>
      <Section title="Image — fit cover vs fill">
        <View flexDirection="row" gap={8}>
          <View backgroundColor={0xff374151} radius={6} width={110} height={90} padding={[6, 6, 6, 6]}>
            <Image src={logo} width={98} height={78} fit="cover" label="logo covered" />
          </View>
          <View backgroundColor={0xff374151} radius={6} width={110} height={90} padding={[6, 6, 6, 6]}>
            <Image src={logo} width={98} height={78} fit="fill" label="logo filled" />
          </View>
        </View>
      </Section>
      <Section title="Resolved src path">
        <Text color="#9ca3af" fontSize={12}>{logo}</Text>
      </Section>
    </View>
  );
}

function Field(props) {
  return (
    <TextField
      value={props.value ? props.value() : ""}
      height={props.height ? props.height() : 40}
      fontSize={15}
      color="#ffffff"
      backgroundColor={0xff4b5563}
      radius={6}
      padding={[8, 10, 10, 8]}
      label={props.label()}
      secure={props.secure ? props.secure() : false}
      multiline={props.multiline ? props.multiline() : false}
      autofill={props.autofill ? props.autofill() : ""}
      keyboard={props.keyboard ? props.keyboard() : ""}
      maxLength={props.maxLength ? props.maxLength() : 0}
      onChange={props.onChange}
    />
  );
}

function TextFieldDemo() {
  const name = signal("");
  const secret = signal("");
  const note = signal("");
  const email = signal("");
  return (
    <View gap={12}>
      <Section title="Single line — onChange echo">
        <Field label="Name" autofill="username" onChange={(e) => name.set(e.value)} />
        <Text color="#9ca3af" fontSize={14}>Echo: {name()}</Text>
      </Section>
      <Section title="Secure + autofill password">
        <Field label="Password" secure autofill="password" onChange={(e) => secret.set("*".repeat(e.value.length))} />
        <Text color="#9ca3af" fontSize={14}>Echo: {secret()}</Text>
      </Section>
      <Section title="Multiline">
        <Field label="Note" multiline height={88} onChange={(e) => note.set(e.value)} />
        <Text color="#9ca3af" fontSize={14}>Echo: {note()}</Text>
      </Section>
      <Section title="Keyboard types + maxLength">
        <Field label="Email" keyboard="email" onChange={(e) => email.set(e.value)} />
        <Field label="Digits" keyboard="number" maxLength={12} onChange={(e) => email.set(e.value)} />
        <Text color="#9ca3af" fontSize={14}>Echo: {email()}</Text>
      </Section>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Gestures
// ---------------------------------------------------------------------------

function TapDemo() {
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

function DoubleTapDemo() {
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

function LongPressDemo() {
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

function PanDemo() {
  const boxRef = ref();
  const panX = createVar(0);
  const panY = createVar(0);
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
              onPanStart={() => {
                panX.set(0);
                panY.set(0);
                status.set("panning");
              }}
              onPanUpdate={(e) => {
                panX.set(e.translationX);
                panY.set(e.translationY);
              }}
              onPanEnd={() => status.set("panned, card stays where dropped")}
              onPanCancel={() => status.set("pan cancelled")}
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

function TransformDemo() {
  const boxRef = ref();
  const scaleV = createVar(1);
  const rotateV = createVar(0);
  const status = signal("idle");
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
              onTransformStart={() => status.set("transforming")}
              onTransformUpdate={(e) => {
                scaleV.set(e.scale);
                rotateV.set(e.rotation);
                status.set("scale " + e.scale.toFixed(2) + " rotate " + e.rotation.toFixed(1));
              }}
              onTransformEnd={() => status.set("transform ended")}
              onTransformCancel={() => status.set("transform cancelled")}
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
// frame, so this demo builds its subtree with the plain runtime calls the
// compiler generates for JSX.
function GestureVarsDemo() {
  const cardRef = ref();
  const scrollRef = ref();
  const barRef = ref();
  return view({ gap: 12 }, () => {
    view({ backgroundColor: "#1f2937", radius: 8, padding: [12, 12, 12, 14], gap: 8 }, () => {
      text({ color: "#fbbf24", fontSize: 13, fontWeight: 700 }, "Pan follows gesture-source variables");
      text(
        { color: "#9ca3af", fontSize: 13 },
        "The card reads gesture.translationX/Y straight from the recognizer — it never lags a frame behind.",
      );
      view(
        { backgroundColor: 0xff8b5cf6, radius: 8, width: 150, height: 72, justifyContent: "center", alignItems: "center", ref: cardRef },
        () => {
          const pan = panGesture({ onPanUpdate: () => {}, onPanEnd: () => {} });
          const tx = createVar({ source: "gesture.translationX", target: pan });
          const ty = createVar({ source: "gesture.translationY", target: pan });
          primitive("ViewStyle", { target: cardRef, when: true, translateX: tx, translateY: ty });
          text({ color: "#ffffff", fontSize: 14 }, "drag me");
        },
      );
    });
    view({ backgroundColor: "#1f2937", radius: 8, padding: [12, 12, 12, 14], gap: 8 }, () => {
      text({ color: "#fbbf24", fontSize: 13, fontWeight: 700 }, "Bar color tracks scroll.y");
      text(
        { color: "#9ca3af", fontSize: 13 },
        "Scroll the list; the bar mixes blue to green over 200 px of scroll offset.",
      );
      const sy = createVar({ source: "scroll.y", target: scrollRef });
      view(
        { ref: scrollRef, height: 140, overflowY: "scroll", backgroundColor: 0xff374151, radius: 6, padding: [8, 8, 8, 10], gap: 6 },
        () => {
          for (let i = 1; i <= 14; i += 1) {
            text({ color: "#d1d5db", fontSize: 14 }, "Scroll item " + i);
          }
        },
      );
      view({ ref: barRef, height: 8, width: 180, radius: 4, backgroundColor: 0xff3b82f6 }, () => {});
      primitive("ViewStyle", {
        target: barRef,
        when: true,
        backgroundColor: ["mix", 0xff3b82f6, 0xff22c55e, ["clamp", ["div", sy, 200], 0, 1]],
      });
    });
  });
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

function ShowDemo() {
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

function ForDemo() {
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

function IndexDemo() {
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

function SwitchDemo() {
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

function SlotDemo() {
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

// ---------------------------------------------------------------------------
// Layout and text properties
// ---------------------------------------------------------------------------

function LayoutDemo() {
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

function TextPropsDemo() {
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

// ---------------------------------------------------------------------------
// Styling — state sources and animated variables
// ---------------------------------------------------------------------------

function PressedDemo() {
  const btnRef = ref();
  const press = createVar({ source: "pressed", target: btnRef });
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

function FocusedDemo() {
  const fieldRef = ref();
  const focused = createVar({ source: "focused", target: fieldRef });
  const editing = createVar({ source: "editing", target: fieldRef });
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
          padding={[8, 10, 10, 8]}
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

function AnimatedDemo() {
  const springBox = ref();
  const tweenBox = ref();
  const slide = createVar(0, { anim: { type: "spring", stiffness: 200, damping: 18 } });
  const glide = createVar(0, { anim: { type: "tween", duration: 400, easing: "easeInOut" } });
  return (
    <View gap={12}>
      <Section title="Spring animation on a script-driven var">
        <View ref={springBox} backgroundColor={0xff22c55e} radius={10} width={56} height={56} />
        <ViewStyle target={springBox} when={true} translateX={slide} />
        <View flexDirection="row" gap={8}>
          <Btn label="Spring to 100" color={0xff22c55e} onTap={() => slide.set(100)} />
          <Btn label="Spring to 0" color={0xff4b5563} onTap={() => slide.set(0)} />
        </View>
      </Section>
      <Section title="Tween animation on a script-driven var">
        <View ref={tweenBox} backgroundColor={0xfff59e0b} radius={10} width={56} height={56} />
        <ViewStyle target={tweenBox} when={true} translateX={glide} />
        <View flexDirection="row" gap={8}>
          <Btn label="Tween to 100" color={0xfff59e0b} onTap={() => glide.set(100)} />
          <Btn label="Tween to 0" color={0xff4b5563} onTap={() => glide.set(0)} />
        </View>
      </Section>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Platform APIs
// ---------------------------------------------------------------------------

function describe(error) {
  return "error: " + (error && error.message ? error.message : String(error));
}

function StorageDemo() {
  const key = signal("greeting");
  const value = signal("hello");
  const result = signal("no calls yet");
  const run = (fn) => {
    try {
      result.set(String(fn()));
    } catch (error) {
      result.set(describe(error));
    }
  };
  return (
    <View gap={12}>
      <Section title="storage — get / set / remove / keys">
        <Field label="Key" value="greeting" onChange={(e) => key.set(e.value)} />
        <Field label="Value" value="hello" onChange={(e) => value.set(e.value)} />
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn
            label="Save"
            color={0xff22c55e}
            onTap={() => run(() => {
              storage.set(key(), value());
              return "saved " + key();
            })}
          />
          <Btn label="Load" color={0xff3b82f6} onTap={() => run(() => "loaded: " + storage.get(key()))} />
          <Btn
            label="Remove"
            color={0xfff59e0b}
            onTap={() => run(() => {
              storage.remove(key());
              return "removed " + key();
            })}
          />
          <Btn label="Keys" color={0xff8b5cf6} onTap={() => run(() => "keys: " + JSON.stringify(storage.keys()))} />
        </View>
        <Text color="#d1d5db" fontSize={14}>Result: {result()}</Text>
        <Text color="#6b7280" fontSize={12}>Web host persists keys in localStorage across reloads. The bubble dev page runs without an embedded host, so these calls report an error there.</Text>
      </Section>
    </View>
  );
}

function ToastDemo() {
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="toast">
        <Btn
          label="Show toast"
          color={0xff22c55e}
          onTap={() => {
            status.set("called");
            toast("Hello from gallery!").then(() => status.set("toast shown")).catch((e) => status.set(describe(e)));
          }}
        />
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
      </Section>
    </View>
  );
}

function ClipboardDemo() {
  const clip = signal("");
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="clipboard — readText / writeText">
        <Field label="Clipboard text" onChange={(e) => clip.set(e.value)} />
        <View flexDirection="row" gap={8}>
          <Btn
            label="Write"
            color={0xff22c55e}
            onTap={() => clipboard.writeText(clip()).then(() => status.set("written: " + clip())).catch((e) => status.set(describe(e)))}
          />
          <Btn
            label="Read"
            color={0xff3b82f6}
            onTap={() => clipboard.readText().then((v) => status.set("read: " + v)).catch((e) => status.set(describe(e)))}
          />
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
        <Text color="#6b7280" fontSize={12}>Clipboard needs a secure context and a user gesture; the browser may ask for permission on read.</Text>
      </Section>
    </View>
  );
}

function FetchDemo() {
  const url = signal("https://jsonplaceholder.typicode.com/todos/1");
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="fetch">
        <Field label="URL" value="https://jsonplaceholder.typicode.com/todos/1" onChange={(e) => url.set(e.value)} />
        <Btn
          label="Fetch"
          color={0xff3b82f6}
          onTap={() => {
            status.set("fetching " + url());
            fetch(url()).then((res) => status.set("status " + res.status + " · " + String(res.text).slice(0, 140))).catch((e) => status.set(describe(e)));
          }}
        />
        <Text color="#d1d5db" fontSize={13}>{status()}</Text>
        <Text color="#6b7280" fontSize={12}>The web host maps fetch to window.fetch; cross-origin URLs need CORS on the server.</Text>
      </Section>
    </View>
  );
}

function OpenNavDemo() {
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="openURL / navigate">
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn
            label="openURL example.com"
            color={0xff3b82f6}
            onTap={() => openURL("https://example.com").then(() => status.set("openURL ok")).catch((e) => status.set(describe(e)))}
          />
          <Btn
            label="navigate home"
            color={0xff8b5cf6}
            onTap={() => navigate({ screen: "home" }).then(() => status.set("navigate ok")).catch((e) => status.set(describe(e)))}
          />
        </View>
        <Text color="#d1d5db" fontSize={14}>Status: {status()}</Text>
        <Text color="#6b7280" fontSize={12}>openURL opens a browser tab on web. navigate needs a host navigate handler; without one the call reports an error.</Text>
      </Section>
    </View>
  );
}

function HostDemo() {
  const status = signal("idle");
  return (
    <View gap={12}>
      <Section title="host bridge — params, call, post, onBack">
        <View flexDirection="row" gap={8} flexWrap="wrap">
          <Btn
            label="Read host.params"
            color={0xff3b82f6}
            onTap={() => {
              try {
                status.set("params: " + JSON.stringify(host.params));
              } catch (error) {
                status.set(describe(error));
              }
            }}
          />
          <Btn
            label="host.call custom"
            color={0xff22c55e}
            onTap={() => host.call("custom", { data: 1 }).then((res) => status.set("call ok: " + JSON.stringify(res))).catch((e) => status.set(describe(e)))}
          />
          <Btn
            label="host.post event"
            color={0xfff59e0b}
            onTap={() => {
              try {
                host.post("gallery-event", { at: Date.now() });
                status.set("posted gallery-event");
              } catch (error) {
                status.set(describe(error));
              }
            }}
          />
          <Btn
            label="Register onBack"
            color={0xff8b5cf6}
            onTap={() => {
              try {
                host.onBack(() => status.set("back pressed"));
                status.set("onBack registered");
              } catch (error) {
                status.set(describe(error));
              }
            }}
          />
        </View>
        <Text color="#d1d5db" fontSize={13}>Status: {status()}</Text>
        <Text color="#6b7280" fontSize={12}>host.call with no registered handler rejects. host.onBack fires on system back — browser history back on web.</Text>
      </Section>
    </View>
  );
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

const demos = [
  { name: "View", demo: ViewDemo },
  { name: "Text", demo: TextDemo },
  { name: "Image", demo: ImageDemo },
  { name: "TextField", demo: TextFieldDemo },
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
      <View backgroundColor="#111827" radius={8} width={190} overflowY="scroll" padding={[10, 8, 8, 10]} gap={4}>
        <Text color="#6b7280" fontSize={11}>bubble gallery · {demos.length} demos</Text>
        <For each={demos} by={(d) => d.name}>
          {(read, i) => (
            <View
              role="button"
              label={"Open " + read().name}
              backgroundColor={selected() === i ? "#2563eb" : "#1f2937"}
              radius={6}
              padding={[9, 6, 6, 10]}
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
        padding={[14, 12, 12, 14]}
        gap={12}
        overflowY="scroll"
      >
        <Text color="#ffffff" fontSize={24} fontWeight={700}>{demos[selected()].name}</Text>
        <Switch>
          <Match when={selected() === 0}>{() => <ViewDemo />}</Match>
          <Match when={selected() === 1}>{() => <TextDemo />}</Match>
          <Match when={selected() === 2}>{() => <ImageDemo />}</Match>
          <Match when={selected() === 3}>{() => <TextFieldDemo />}</Match>
          <Match when={selected() === 4}>{() => <TapDemo />}</Match>
          <Match when={selected() === 5}>{() => <DoubleTapDemo />}</Match>
          <Match when={selected() === 6}>{() => <LongPressDemo />}</Match>
          <Match when={selected() === 7}>{() => <PanDemo />}</Match>
          <Match when={selected() === 8}>{() => <TransformDemo />}</Match>
          <Match when={selected() === 9}>{() => <GestureVarsDemo />}</Match>
          <Match when={selected() === 10}>{() => <ShowDemo />}</Match>
          <Match when={selected() === 11}>{() => <ForDemo />}</Match>
          <Match when={selected() === 12}>{() => <IndexDemo />}</Match>
          <Match when={selected() === 13}>{() => <SwitchDemo />}</Match>
          <Match when={selected() === 14}>{() => <SlotDemo />}</Match>
          <Match when={selected() === 15}>{() => <LayoutDemo />}</Match>
          <Match when={selected() === 16}>{() => <TextPropsDemo />}</Match>
          <Match when={selected() === 17}>{() => <PressedDemo />}</Match>
          <Match when={selected() === 18}>{() => <FocusedDemo />}</Match>
          <Match when={selected() === 19}>{() => <AnimatedDemo />}</Match>
          <Match when={selected() === 20}>{() => <StorageDemo />}</Match>
          <Match when={selected() === 21}>{() => <ToastDemo />}</Match>
          <Match when={selected() === 22}>{() => <ClipboardDemo />}</Match>
          <Match when={selected() === 23}>{() => <FetchDemo />}</Match>
          <Match when={selected() === 24}>{() => <OpenNavDemo />}</Match>
          <Match when={selected() === 25}>{() => <HostDemo />}</Match>
        </Switch>
      </View>
    </View>
  );
}

mount(App);
