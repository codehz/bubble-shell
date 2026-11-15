// Primitive demos: View, Text, Image, TextField.
import { signal, View, Text, Image, For } from "bubble-shell";
import logo from "./assets/logo.png";
import { Section, Btn, Field } from "./shared";

export function ViewDemo() {
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

export function TextDemo() {
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

export function ImageDemo() {
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

export function TextFieldDemo() {
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
