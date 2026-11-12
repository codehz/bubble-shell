import { mount, signal, Text, View, TextField } from "bubble";

function Editing() {
  const name = signal("");
  const secret = signal("");
  const note = signal("");
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={8}>
      <Text color="#ffffff" fontSize={16}>Name</Text>
      <TextField
        value=""
        height={40}
        fontSize={18}
        color="#ffffff"
        backgroundColor={0xff4b5563}
        radius={6}
        padding={[8, 10, 10, 8]}
        label="Name"
        autofill="username"
        onChange={(event) => name.update(() => event.value)}
      />
      <Text color="#9ca3af" fontSize={14}>{name()}</Text>
      <Text color="#ffffff" fontSize={16}>Password</Text>
      <TextField
        value=""
        height={40}
        fontSize={18}
        color="#ffffff"
        backgroundColor={0xff4b5563}
        radius={6}
        padding={[8, 10, 10, 8]}
        secure
        label="Password"
        autofill="password"
        onChange={(event) => secret.update(() => secret().length === event.value.length ? secret() : "*".repeat(event.value.length))}
      />
      <Text color="#9ca3af" fontSize={14}>{secret()}</Text>
      <Text color="#ffffff" fontSize={16}>Note</Text>
      <TextField
        value=""
        height={72}
        fontSize={18}
        color="#ffffff"
        backgroundColor={0xff4b5563}
        radius={6}
        padding={[8, 10, 10, 8]}
        multiline
        label="Note"
        onChange={(event) => note.update(() => event.value)}
      />
    </View>
  );
}

mount(Editing);
