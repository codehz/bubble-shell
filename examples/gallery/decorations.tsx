// Decoration demos: concentric rings, outer shadows, and node transitions.
import {
  signal,
  ref,
  View,
  Text,
  TextField,
  Image,
  TapGesture,
  Border,
  Outline,
  Shadow,
  createSharedValue,
} from "bubble-shell";
import logo from "./assets/logo.png";
import { Section, Btn } from "./shared";

function AlignCard(props) {
  return (
    <View width={88} height={64} backgroundColor={0xff0f172a} radius={12} justifyContent="center" alignItems="center">
      <Border color="#38bdf8" width={6} align={props.align()} />
      <Text color="#e2e8f0" fontSize={12}>{props.align()}</Text>
    </View>
  );
}

export function DecorationsDemo() {
  const card = ref();
  const press = createSharedValue({ source: "pressed", target: card });
  const fieldRef = ref();
  const focused = createSharedValue({ source: "focused", target: fieldRef });
  const width = signal(2);
  return (
    <View gap={12}>
      <Section title="Border, outline, and shadow">
        <View width={220} height={96} backgroundColor={0xff0f172a} radius={18} justifyContent="center" alignItems="center">
          <Shadow color="#020617" offsetY={10} blur={18} />
          <Border color="#38bdf8" width={3} align="inside" />
          <Outline color="#f59e0b" width={2} offset={6} />
          <Text color="#f8fafc" fontSize={16}>stacked</Text>
        </View>
        <Text color="#6b7280" fontSize={12}>
          The shadow paints behind the fill and is knocked out by the host shape. The border stays inside the box. The outline sits outside it.
        </Text>
      </Section>
      <Section title="Border align">
        <View flexDirection="row" gap={12}>
          <AlignCard align="inside" />
          <AlignCard align="center" />
          <AlignCard align="outside" />
        </View>
      </Section>
      <Section title="Pressed border uses the view transition">
        <View
          ref={card}
          role="button"
          label="Hold"
          width={200}
          height={52}
          backgroundColor={0xff1e293b}
          radius={12}
          justifyContent="center"
          alignItems="center"
          transition={{ duration: 180 }}
        >
          <TapGesture onTap={() => {}} />
          <Border width={3} color={press > 0 ? "#f59e0b" : "#64748b"} />
          <Text color="#f8fafc" fontSize={15}>hold</Text>
        </View>
      </Section>
      <Section title="Focused outline">
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
        >
          <Outline color={focused ? "#38bdf8" : "#334155"} width={2} offset={3} />
        </TextField>
      </Section>
      <Section title="Image shadow and text outline">
        <Image src={logo} width={72} height={72} fit="contain" label="logo">
          <Shadow color="#020617" offsetY={6} blur={12} />
        </Image>
        <Text color="#f8fafc" fontSize={22}>
          <Outline color="#0ea5e9" width={1} offset={2} />
          outlined
        </Text>
      </Section>
      <Section title="Live border width">
        <View width={180} height={48} backgroundColor={0xff14532d} radius={10} justifyContent="center" alignItems="center">
          <Border color="#86efac" width={width()} />
          <Text color="#f0fdf4" fontSize={14}>{width() + " px"}</Text>
        </View>
        <Btn label="Toggle width" color="#166534" onTap={() => width.set(width() === 2 ? 8 : 2)} />
      </Section>
    </View>
  );
}
