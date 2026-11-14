import { mount, signal, Text, View, TapGesture, DoubleTapGesture, LongPressGesture, PanGesture, TransformGesture, ViewStyle, createVar, ref, For, Dynamic } from "bubble-shell";

const SCENES = [
  { id: "tap", label: "Tap" },
  { id: "double", label: "Double tap" },
  { id: "long", label: "Long press" },
  { id: "pan", label: "Pan" },
  { id: "transform", label: "Transform" },
  { id: "parallel", label: "Pan + scroll" },
  { id: "reserve", label: "Hold + scroll" },
  { id: "contain", label: "Nested scroll" },
  { id: "dispose", label: "Dispose" },
];

const HINTS = {
  tap: "Tap the pad — press state and tap lifecycle are shown.",
  double: "Double tap the pad.",
  long: "Hold the pad ~500 ms without moving.",
  pan: "Drag the pad in any direction.",
  transform: "Drag, pinch, or rotate with two fingers.",
  parallel: "Scroll the list vertically; drag the pad sideways only.",
  reserve: "Scroll the list; holding ~2 s reserves the pad before the tap fires.",
  contain: "The inner list stops at its edge; the outer list then takes over.",
  dispose: "Start a drag — the pan gesture removes itself mid-drag.",
};

function describe(event) {
  let line = event.type;
  if (typeof event.translationX === "number") {
    line += ` dx=${Math.round(event.translationX)} dy=${Math.round(event.translationY)}`;
  }
  if (typeof event.scale === "number" && event.scale !== 1) {
    line += ` ×${event.scale.toFixed(2)}`;
  }
  if (typeof event.rotation === "number" && event.rotation !== 0) {
    line += ` ${Math.round((event.rotation * 180) / Math.PI)}°`;
  }
  return line;
}

// The compiler wraps non-element JSX attributes in Dynamic; unwrap to the
// signal the scene passed down.
function unwrap(value) {
  return value instanceof Dynamic ? value.run() : value;
}

function Pad(props) {
  const pad = ref();
  const pressed = createVar({ source: "pressed", target: pad });
  const live = unwrap(props.live);
  return (
    <View ref={pad} backgroundColor={props.color} radius={12} flexGrow={1} flexShrink={1} label={props.title}>
      <ViewStyle target={pad} when={pressed > 0} opacity={0.7} />
      {props.children}
      <Text color="#ffffff" fontSize={18} textAlign="center">{props.title}</Text>
      <Text color="#e5e7eb" fontSize={13} textAlign="center">{props.detail}</Text>
      <Text color="#fbbf24" fontSize={14} textAlign="center">{live()}</Text>
    </View>
  );
}

function SceneFrame(props) {
  const events = unwrap(props.events);
  return (
    <View flexGrow={1} flexShrink={1} gap={8}>
      <Text color="#9ca3af" fontSize={13}>{props.hint}</Text>
      {props.children}
      <View height={150} flexShrink={0} overflowY="scroll" backgroundColor="#111827" radius={8} padding={[8, 10, 10, 8]} label="Gesture log">
        <Text color="#cbd5e1" fontSize={12}>{events().length ? events().join("\n") : "no events yet"}</Text>
      </View>
    </View>
  );
}

function useRecorder() {
  const events = signal([]);
  const live = signal("");
  const record = (kind, event) => {
    events.update((list) => [...list.slice(-11), `${kind} → ${describe(event)}`]);
  };
  return { events, live, record };
}

function TapScene() {
  const { events, live, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.tap} events={events}>
      <Pad title="Tap pad" detail="down · up · tap · cancel" color="#26364a" live={live}>
        <TapGesture
          onTapStart={(e) => record("tap", e)}
          onTap={(e) => record("tap", e)}
          onTapEnd={(e) => record("tap", e)}
          onTapCancel={(e) => record("tap", e)}
        />
      </Pad>
    </SceneFrame>
  );
}

function DoubleScene() {
  const { events, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.double} events={events}>
      <Pad title="Double tap pad" detail="two quick taps" color="#31405a" live={signal("")}>
        <DoubleTapGesture onDoubleTap={(e) => record("double", e)} />
      </Pad>
    </SceneFrame>
  );
}

function LongScene() {
  const { events, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.long} events={events}>
      <Pad title="Long press pad" detail="hold 500 ms" color="#3d2f4d" live={signal("")}>
        <LongPressGesture
          duration={500}
          maxDistance={10}
          onLongPress={(e) => record("long", e)}
          onLongPressEnd={(e) => record("long", e)}
          onLongPressCancel={(e) => record("long", e)}
        />
      </Pad>
    </SceneFrame>
  );
}

function PanScene() {
  const { events, live, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.pan} events={events}>
      <Pad title="Pan pad" detail="any axis, 10 px threshold" color="#1f3d34" live={live}>
        <PanGesture
          axis="both"
          minDistance={10}
          onPanStart={(e) => {
            live.update(() => describe(e));
            record("pan", e);
          }}
          onPanUpdate={(e) => {
            live.update(() => describe(e));
            record("pan", e);
          }}
          onPanEnd={(e) => record("pan", e)}
          onPanCancel={(e) => record("pan", e)}
        />
      </Pad>
    </SceneFrame>
  );
}

function TransformScene() {
  const { events, live, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.transform} events={events}>
      <Pad title="Transform pad" detail="pan + pinch + rotate" color="#3d2f2f" live={live}>
        <TransformGesture
          mode={["pan", "pinch", "rotate"]}
          minDistance={10}
          onTransformStart={(e) => {
            live.update(() => describe(e));
            record("transform", e);
          }}
          onTransformUpdate={(e) => {
            live.update(() => describe(e));
            record("transform", e);
          }}
          onTransformEnd={(e) => record("transform", e)}
          onTransformCancel={(e) => record("transform", e)}
        />
      </Pad>
    </SceneFrame>
  );
}

function ParallelScene() {
  const { events, live, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.parallel} events={events}>
      <View height={260} flexShrink={0} overflowY="scroll" backgroundColor="#26364a" radius={12} padding={[8, 8, 8, 8]} gap={8}>
        <View height={140} flexShrink={0} backgroundColor="#4c1d95" radius={8} label="Pan pad">
          <PanGesture
            axis="x"
            minDistance={10}
            onPanUpdate={(e) => {
              live.update(() => describe(e));
              record("pan", e);
            }}
            onPanEnd={(e) => record("pan", e)}
          />
          <Text color="#ffffff" fontSize={14}>Horizontal pan pad</Text>
        </View>
        <Text color="#e5e7eb" fontSize={13}>Vertical scroll takes the other axis.</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
      </View>
    </SceneFrame>
  );
}

function ReserveScene() {
  const { events, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.reserve} events={events}>
      <View height={260} flexShrink={0} overflowY="scroll" backgroundColor="#26364a" radius={12} padding={[8, 8, 8, 8]} gap={8}>
        <View height={140} flexShrink={0} backgroundColor="#7c2d12" radius={8} label="Hold pad">
          <LongPressGesture
            duration={2000}
            maxDistance={60}
            captureAxis="y"
            onLongPress={(e) => record("hold", e)}
            onLongPressEnd={(e) => record("hold", e)}
            onLongPressCancel={(e) => record("hold", e)}
          />
          <TapGesture
            onTapStart={(e) => record("tap", e)}
            onTap={(e) => record("tap", e)}
            onTapEnd={(e) => record("tap", e)}
            onTapCancel={(e) => record("tap", e)}
          />
          <Text color="#ffffff" fontSize={14}>Hold 2 s or tap</Text>
        </View>
        <Text color="#e5e7eb" fontSize={13}>Scrolling stays live until the hold fires.</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
        <Text color="#e5e7eb" fontSize={13}>Filler row</Text>
      </View>
    </SceneFrame>
  );
}

function ContainScene() {
  const { events } = useRecorder();
  return (
    <SceneFrame hint={HINTS.contain} events={events}>
      <View height={280} flexShrink={0} overflowY="scroll" backgroundColor="#26364a" radius={12} padding={[8, 8, 8, 8]} gap={8}>
        <View height={120} flexShrink={0} overflowY="scroll" overscrollY={false} backgroundColor="#526b83" radius={8} label="Inner list">
          <Text color="#ffffff" fontSize={13}>Inner row 1</Text>
          <Text color="#ffffff" fontSize={13}>Inner row 2</Text>
          <Text color="#ffffff" fontSize={13}>Inner row 3</Text>
          <Text color="#ffffff" fontSize={13}>Inner row 4</Text>
          <Text color="#ffffff" fontSize={13}>Inner row 5</Text>
        </View>
        <View height={180} flexShrink={0} label="Outer list">
          <Text color="#ffffff" fontSize={13}>Outer row</Text>
          <Text color="#ffffff" fontSize={13}>Outer row</Text>
          <Text color="#ffffff" fontSize={13}>Outer row</Text>
        </View>
        <Text color="#e5e7eb" fontSize={13}>More outer content below the fold.</Text>
        <Text color="#e5e7eb" fontSize={13}>Outer row</Text>
        <Text color="#e5e7eb" fontSize={13}>Outer row</Text>
      </View>
    </SceneFrame>
  );
}

function DisposeScene() {
  const { events, live, record } = useRecorder();
  return (
    <SceneFrame hint={HINTS.dispose} events={events}>
      <Pad title="Dispose pad" detail="pan removes itself on first move" color="#134e4a" live={live}>
        <PanGesture
          axis="both"
          minDistance={10}
          onPanStart={(e) => record("pan", e)}
          onPanUpdate={(e) => {
            live.update(() => describe(e));
            record("pan", e);
          }}
          onPanEnd={(e) => record("pan", e)}
        />
      </Pad>
    </SceneFrame>
  );
}

function Scene(props) {
  if (props.name === "tap") return <TapScene />;
  if (props.name === "double") return <DoubleScene />;
  if (props.name === "long") return <LongScene />;
  if (props.name === "pan") return <PanScene />;
  if (props.name === "transform") return <TransformScene />;
  if (props.name === "parallel") return <ParallelScene />;
  if (props.name === "reserve") return <ReserveScene />;
  if (props.name === "contain") return <ContainScene />;
  return <DisposeScene />;
}

function Gestures() {
  const scene = signal("tap");
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={10}>
      <Text color="#ffffff" fontSize={20} fontWeight={700}>Gestures</Text>
      <View flexDirection="row" flexWrap="wrap" gap={6} flexShrink={0} label="Scenes">
        <For each={SCENES} by={(item) => item}>
          {(read) => (
            <View
              width={84}
              height={26}
              backgroundColor={scene() === read().id ? 0xff2563eb : 0xff374151}
              radius={6}
              padding={[5, 0, 0, 5]}
              label={read().label}
            >
              <TapGesture onTap={() => scene.update(() => read().id)} />
              <Text color="#ffffff" fontSize={11} textAlign="center">{read().label}</Text>
            </View>
          )}
        </For>
      </View>
      <For each={[scene()]} by={(name) => name}>
        {(read) => <Scene name={read()} />}
      </For>
    </View>
  );
}

mount(Gestures);
