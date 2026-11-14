import { mount, Image, Text, View } from "bubble-shell";
import logo from "../assets/logo.png";

function Media() {
  return (
    <View backgroundColor="#111827" flexGrow={1} flexShrink={1} padding={[16, 16, 16, 16]} gap={12}>
      <Text testID="title" color="#ffffff" fontSize={22}>Asset import</Text>
      <Image testID="logo" src={logo} width={64} height={64} label="logo" />
      <Text testID="path" color="#9ca3af" fontSize={14}>{logo}</Text>
    </View>
  );
}

mount(Media);
