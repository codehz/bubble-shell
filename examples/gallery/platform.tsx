// Platform API demos: storage, toast, clipboard, fetch, URL/navigation, host bridge.
import {
  signal,
  View,
  Text,
  storage,
  clipboard,
  toast,
  fetch,
  openURL,
  navigate,
  host,
} from "bubble-shell";
import { Section, Btn, Field } from "./shared";

function describe(error) {
  return "error: " + (error && error.message ? error.message : String(error));
}

export function StorageDemo() {
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

export function ToastDemo() {
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

export function ClipboardDemo() {
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

export function FetchDemo() {
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

export function OpenNavDemo() {
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

export function HostDemo() {
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
