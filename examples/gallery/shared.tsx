// Building blocks shared by the gallery demos.
import { View, Text, TextField, TapGesture } from "bubble-shell";

export function Section(props) {
  return (
    <View backgroundColor="#1f2937" radius={8} padding={[12, 12, 12, 12]} gap={8}>
      <Text color="#fbbf24" fontSize={13} fontWeight={700}>{props.title()}</Text>
      {props.children}
    </View>
  );
}

export function Btn(props) {
  return (
    <View
      role="button"
      label={props.label()}
      backgroundColor={props.color()}
      radius={8}
      padding={[12, 12, 12, 12]}
    >
      <TapGesture onTap={props.onTap} />
      <Text color="#ffffff" fontSize={14}>{props.label()}</Text>
    </View>
  );
}

export function Field(props) {
  return (
    <TextField
      value={props.value ? props.value() : ""}
      height={props.height ? props.height() : 40}
      width={props.width ? props.width() : null}
      fontSize={15}
      color="#ffffff"
      backgroundColor={0xff4b5563}
      radius={6}
      padding={[8, 8, 8, 8]}
      label={props.label()}
      secure={props.secure ? props.secure() : false}
      multiline={props.multiline ? props.multiline() : false}
      autofill={props.autofill ? props.autofill() : ""}
      keyboard={props.keyboard ? props.keyboard() : ""}
      maxLength={props.maxLength ? props.maxLength() : 0}
      fadeEdges={props.fadeEdges ? props.fadeEdges() : 0}
      onChange={props.onChange}
    />
  );
}
