import { mount, signal, Text, View, TextField, TapGesture } from "bubble-shell";

function Focus() {
  const opened = signal(false);
  const nested = signal(false);
  return (
    <View backgroundColor="#111827" flexGrow={1} flexShrink={1} padding={[16, 16, 16, 16]} gap={12}>
      <Text color="#ffffff" fontSize={22}>Modal focus</Text>
      <View role="button" label="Open dialog" backgroundColor="#2563eb" padding={[12, 12, 12, 12]}>
        <TapGesture onTap={() => opened.update(() => true)} />
        <Text color="#ffffff">Open dialog</Text>
      </View>
      <TextField label="Background field" value="Background" height={40} color="#ffffff" />
      <Show when={opened()}>
        {() => (
        <View modal role="dialog" label="Settings" backgroundColor="#374151" padding={[12, 12, 12, 12]} gap={12}>
          <Text color="#ffffff" fontSize={20}>Settings</Text>
          <TextField label="Dialog field" value="Edit here" height={40} color="#ffffff" />
          <View role="button" label="Open nested dialog" backgroundColor="#2563eb" padding={[12, 12, 12, 12]}>
            <TapGesture onTap={() => nested.update(() => true)} />
            <Text color="#ffffff">Open nested dialog</Text>
          </View>
          <View role="button" label="Close dialog" backgroundColor="#4b5563" padding={[12, 12, 12, 12]}>
            <TapGesture onTap={() => opened.update(() => false)} />
            <Text color="#ffffff">Close dialog</Text>
          </View>
          <Show when={nested()}>
            {() => (
            <View modal role="dialog" label="Confirmation" backgroundColor="#1f2937" padding={[12, 12, 12, 12]} gap={12}>
              <Text color="#ffffff">Confirmation</Text>
              <View role="button" label="Close nested dialog" backgroundColor="#2563eb" padding={[12, 12, 12, 12]}>
                <TapGesture onTap={() => nested.update(() => false)} />
                <Text color="#ffffff">Close nested dialog</Text>
              </View>
            </View>
            )}
          </Show>
        </View>
        )}
      </Show>
    </View>
  );
}

mount(Focus);
